use std::{borrow::Cow, os::unix::ffi::OsStrExt, path::PathBuf, sync::Arc};

use anyhow::{bail, Context, Result};

use hyper::Uri;
use tokio::sync::mpsc::Sender;

use crate::{downloaders::ParserTask, request::request};

use super::{CollectResponse, GetHtmlTag, TagWithParser};

pub(crate) struct DownloadCtx {
    pages_count: usize,
    img_url_pattern: Uri,
}

impl DownloadCtx {
    pub(crate) fn get_urls(&self) -> Result<Vec<Uri>> {
        use hyper::http::uri::PathAndQuery;

        let mut urls = Vec::<Uri>::with_capacity(self.pages_count);

        let path = PathBuf::from(self.img_url_pattern.path());
        let prefix = path.parent().context("failed to get parrent")?;
        let ext = path.extension().context("failed to get image extension")?;

        for page in 1..=self.pages_count {
            let path_and_query = prefix.join(page.to_string()).with_extension(ext);

            let path_and_query = path_and_query.as_os_str().as_bytes();

            let mut parts = self.img_url_pattern.clone().into_parts();
            parts.path_and_query = Some(PathAndQuery::try_from(path_and_query)?);
            urls.push(Uri::from_parts(parts)?);
        }

        Ok(urls)
    }
}

pub struct GalleryInfo {
    pub pages_count: usize,
    pub title: String,
}

#[async_trait::async_trait]
pub(crate) trait CommonUrlPatternDownloader: Sync + Send {
    fn get_first_image_url(&self, html: &TagWithParser<'_, '_>) -> Result<Uri>;
    fn get_info(&self, info_tag: &TagWithParser<'_, '_>) -> Result<GalleryInfo>;
    async fn get_image_pattern_from_first_image_page(&self, first_image_page: &Uri) -> Result<Uri>;

    async fn parse_ctx(
        &self,
        gallery_uri: &Uri,
        gallery_page: &[u8],
    ) -> Result<(String, DownloadCtx)> {
        let page = String::from_utf8_lossy(gallery_page);

        let (title, pages_count, first_image) = {
            let dom = tl::parse(&page, Default::default())
                .with_context(|| format!("failed to parse page for {gallery_uri:?}"))?;

            let html = &dom.get_html_tag()?;

            let info = self.get_info(html)?;

            let first_image = self.get_first_image_url(html)?;
            let first_image = super::merge_uris(&first_image, gallery_uri);

            (info.title, info.pages_count, first_image)
        };

        let img_url_pattern = self
            .get_image_pattern_from_first_image_page(&first_image)
            .await?;

        Ok((
            title,
            DownloadCtx {
                pages_count,
                img_url_pattern,
            },
        ))
    }
}

#[async_trait::async_trait]
impl<T: CommonUrlPatternDownloader> ParserTask for T {
    async fn try_start_parser_task(
        self: Arc<Self>,
        tx: Sender<crate::downloaders::Msg>,
        gallery: Arc<Uri>,
    ) -> Result<()> {
        let mut gallery = Cow::Borrowed(gallery.as_ref());
        if !gallery.path().ends_with('/') {
            use hyper::http::uri::PathAndQuery;
            let mut parts = gallery.into_owned().into_parts();

            let path_and_query = parts.path_and_query.unwrap();
            let path = path_and_query.path();
            let query = path_and_query.query();
            let mut path_and_query = String::with_capacity(path.len() + 1);

            path_and_query.push_str(path);
            path_and_query.push('/');
            if let Some(query) = query {
                path_and_query.push('?');
                path_and_query.push_str(query);
            }

            parts.path_and_query = Some(PathAndQuery::try_from(path_and_query)?);

            gallery = Cow::Owned(Uri::from_parts(parts)?);
        }

        let response = request(&gallery).await?;
        let code = response.status();
        if code != hyper::http::StatusCode::OK {
            bail!(format!("failed to get {gallery}: status code: {code}"))
        }

        let page = response.collect_response().await?;

        use crate::downloaders::Msg;

        let (title, ctx) = self.parse_ctx(&gallery, &page).await?;

        tx.send(Msg::Title(title)).await?;
        tx.send(Msg::Images(ctx.get_urls()?)).await?;

        Ok(())
    }
}
