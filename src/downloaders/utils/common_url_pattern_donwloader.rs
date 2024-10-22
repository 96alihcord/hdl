use std::{os::unix::ffi::OsStrExt, path::PathBuf};

use anyhow::{bail, Context, Result};

use hyper::Uri;
use tl::VDom;

use crate::{downloaders::GetImageUrls, request::request};

use super::CollectResponse;

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

#[async_trait::async_trait]
pub(crate) trait CommonUrlPatternDownloader: Sync + Send {
    fn get_pages_count(&self, dom: &VDom<'_>) -> Result<usize>;
    fn get_first_image_url(&self, dom: &VDom<'_>) -> Result<Uri>;
    async fn get_image_pattern_from_first_image_page(&self, first_image_page: &Uri) -> Result<Uri>;

    async fn parse_ctx(&self, gallery_uri: &Uri, gallery_page: &[u8]) -> Result<DownloadCtx> {
        let page = String::from_utf8_lossy(gallery_page);

        let (pages_count, first_image) = {
            let dom = tl::parse(&page, Default::default())
                .with_context(|| format!("failed to parse page for {gallery_uri:?}"))?;

            let pages_count = self.get_pages_count(&dom)?;

            let first_image = self.get_first_image_url(&dom)?;
            let first_image = super::merge_uris(&first_image, gallery_uri);

            (pages_count, first_image)
        };

        let img_url_pattern = self
            .get_image_pattern_from_first_image_page(&first_image)
            .await?;

        Ok(DownloadCtx {
            pages_count,
            img_url_pattern,
        })
    }
}

#[async_trait::async_trait]
impl<T: CommonUrlPatternDownloader> GetImageUrls for T {
    async fn get_image_urls(&self, gallery: &Uri) -> Result<Vec<Uri>> {
        // TODO: handle gallery url that not ends wtih /

        let response = request(&gallery).await?;
        let code = response.status();
        if code != hyper::http::StatusCode::OK {
            bail!(format!("failed to get {gallery}: status code: {code}"))
        }

        let page = response.collect_response().await?;

        let ctx = self.parse_ctx(gallery, &page).await?;

        ctx.get_urls()
    }
}
