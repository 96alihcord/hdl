use std::borrow::Cow;

use anyhow::{Context, Result};
use hyper::Uri;
use regex::Regex;
use tl::VDom;

use crate::{downloaders::Downloader, request::request};

use super::{
    utils::{self, CollectResponse, GetHtmlTag, QuerySelectorMutliple},
    GetImageUrls,
};

pub struct Ehentai {
    name: &'static str,
    authority: &'static str,

    path_re: Regex,

    gallery_selector: &'static str,
    gallery_link_selector: &'static str,

    next_page_selector: &'static [&'static str],

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

            next_page_selector: &["div.gtb", "td", "a"],

            image_selector: "img#img",
        }
    }

    #[inline]
    fn is_gallery_path_match(&self, uri: &Uri) -> bool {
        self.path_re.is_match(uri.path())
    }

    fn get_next_page_url(&self, dom: &VDom<'_>) -> Option<Uri> {
        let parser = dom.parser();
        let href = dom
            .get_html_tag()
            .ok()?
            .query_selector_mutliple(parser, self.next_page_selector.iter())
            .map(|tag| tag.attributes())
            .ok()
            .and_then(|attrs| attrs.get("href"))??;

        Uri::try_from(href.as_bytes()).ok()
    }

    // TODO: make downloader return pages in junks so it can start downloading images earlier
    /// returns: (urls, next page url)
    async fn get_page_img_urls(&self, page_url: &Uri) -> Result<(Vec<Uri>, Option<Uri>)> {
        let page = request(page_url).await?.collect_response().await?;
        let page = String::from_utf8_lossy(&page);

        let dom = tl::parse(&page, Default::default())?;
        let parser = dom.parser();

        let gallery = dom
            .query_selector(self.gallery_selector)
            .and_then(|mut q| q.next())
            .and_then(|node| node.get(parser))
            .map(|node| node.inner_html(parser))
            .with_context(|| format!("selector not found: {}", self.gallery_selector))?;

        let gallery_dom = tl::parse(&gallery, Default::default())?;
        let parser = gallery_dom.parser();

        let urls = gallery_dom
            .query_selector(self.gallery_link_selector)
            .map(|q| {
                q.map(|node| -> Result<_> {
                    let href = node
                        .get(parser)
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

        let next = self.get_next_page_url(&dom);
        Ok((urls, next))
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
impl GetImageUrls for Ehentai {
    async fn get_image_urls(&self, gallery: &Uri) -> Result<Vec<Uri>> {
        let mut urls = Vec::new();
        let mut page_url = Cow::Borrowed(gallery);

        loop {
            let (page_urls, next) = self.get_page_img_urls(&page_url).await?;
            urls.extend(page_urls);

            if let Some(next) = next {
                page_url = Cow::Owned(next);
            } else {
                break;
            }
        }

        Ok(urls)
    }
}
