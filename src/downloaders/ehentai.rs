use std::{borrow::Cow, sync::Arc};

use anyhow::{Context, Result};
use hyper::Uri;
use regex::Regex;
use tokio::sync::{mpsc::Sender, OnceCell};

use crate::{downloaders::Downloader, request::request};

use super::{
    utils::{self, CollectResponse, GetHtmlTag, TagWithParser},
    Msg, ParserTask,
};

pub struct Ehentai {
    name: &'static str,
    authority: &'static str,

    path_re: Regex,

    gallery_selector: &'static str,
    gallery_link_selector: &'static str,

    next_page_selector: &'static [&'static str],
    title_selector: &'static [&'static str],

    image_selector: &'static str,
}

impl Ehentai {
    pub fn new() -> Self {
        Self {
            name: "Ehentai",
            authority: "e-hentai.org",

            path_re: Regex::new(r"^/g/(?P<gallery_id>\d+)/(?P<gellery_hex>[[:xdigit:]]+)/?$")
                .unwrap(),

            gallery_selector: "div#gdt",
            gallery_link_selector: "a",

            next_page_selector: &["div.gtb", "table"],
            title_selector: &["div.gm", "h1#gn"],

            image_selector: "img#img",
        }
    }

    #[inline]
    fn is_gallery_path_match(&self, uri: &Uri) -> bool {
        self.path_re.is_match(uri.path())
    }

    fn get_next_page_url(&self, html: &TagWithParser<'_, '_>) -> Option<Uri> {
        let table = html
            .query_selector_mutliple(self.next_page_selector.iter())
            .ok()?;

        let last_td = table
            .query_selector("td")
            .and_then(|q| q.last())
            .and_then(|node| node.get(html.parser))
            .and_then(|node| node.as_tag())?;

        let href = last_td
            .query_selector(html.parser, "a")
            .and_then(|mut q| q.next())
            .and_then(|node| node.get(html.parser))
            .and_then(|node| node.as_tag())
            .map(|tag| tag.attributes())
            .and_then(|attrs| attrs.get("href"))??;

        Uri::try_from(href.as_bytes()).ok()
    }

    fn get_title(&self, html: &TagWithParser<'_, '_>) -> Result<String> {
        let name = html
            .query_selector_mutliple(self.title_selector.iter())?
            .tag
            .inner_text(html.parser);
        Ok(name.to_string())
    }

    /// returns: (title, urls, next page url)
    async fn get_page_img_urls(
        &self,
        need_name: bool,
        page_url: &Uri,
    ) -> Result<(Option<String>, Vec<Uri>, Option<Uri>)> {
        let page = request(page_url).await?.collect_response().await?;
        let page = String::from_utf8_lossy(&page);

        let dom = tl::parse(&page, Default::default())?;
        let html = &dom.get_html_tag()?;

        let gallery = html
            .query_selector(self.gallery_selector)
            .and_then(|mut q| q.next())
            .and_then(|node| node.get(html.parser))
            .and_then(|node| node.as_tag())
            .with_context(|| format!("selector not found: {}", self.gallery_selector))?;

        let urls = gallery
            .query_selector(html.parser, self.gallery_link_selector)
            .map(|q| {
                q.map(|node| -> Result<_> {
                    let href = node
                        .get(html.parser)
                        .and_then(|node| node.as_tag())
                        .map(|tag| tag.attributes())
                        .map(|attrs| attrs.get("href"))
                        .with_context(|| {
                            format!("failed to get {} node", self.gallery_link_selector)
                        })?
                        .context("failed to get 'href' attribute")?
                        .context("empty 'href' attribute")?;
                    Ok(Uri::try_from(href.as_bytes())?)
                })
            })
            .with_context(|| format!("failed to query selector: {}", self.gallery_link_selector))?
            .collect::<Result<Vec<_>>>()?;

        let next = self.get_next_page_url(html);
        let name = if need_name {
            Some(self.get_title(html)?)
        } else {
            None
        };
        Ok((name, urls, next))
    }
}

#[async_trait::async_trait]
impl Downloader for Ehentai {
    fn name(&self) -> &'static str {
        self.name
    }

    fn is_gallery_match(&self, gallery: &Uri) -> bool {
        utils::is_supported_scheme(gallery)
            && utils::is_proper_authority(gallery, self.authority)
            && self.is_gallery_path_match(gallery)
    }

    async fn resolve_image_url<'a>(&self, url: &'a Uri) -> Result<Cow<'a, Uri>> {
        let page = request(url).await?.collect_response().await?;
        let page = String::from_utf8_lossy(&page);

        let dom = tl::parse(&page, Default::default())?;
        let parser = dom.parser();

        let img = dom
            .query_selector(self.image_selector)
            .and_then(|mut q| q.next())
            .and_then(|node| node.get(parser))
            .and_then(|node| node.as_tag())
            .map(|tag| tag.attributes())
            .and_then(|attrs| attrs.get("src"))
            .with_context(|| format!("failed to query selector: {}", self.image_selector))?
            .context("empty 'src' attribute")?;

        let url = Uri::try_from(img.as_bytes())?;

        Ok(Cow::Owned(url))
    }
}

#[async_trait::async_trait]
impl ParserTask for Ehentai {
    async fn try_start_parser_task(
        self: Arc<Self>,
        tx: Sender<Msg>,
        gallery: Arc<Uri>,
    ) -> Result<()> {
        let mut page_url = Cow::Borrowed(gallery.as_ref());

        let once = OnceCell::new();
        loop {
            let need_name = !once.initialized();
            let (title, page_urls, next) = self.get_page_img_urls(need_name, &page_url).await?;

            once.get_or_try_init(|| async {
                let title = title.expect("no title parsed");
                tx.send(Msg::Title(title)).await
            })
            .await?;

            tx.send(Msg::Images(page_urls)).await?;

            if let Some(next) = next {
                page_url = Cow::Owned(next);
            } else {
                break;
            }
        }

        Ok(())
    }
}
