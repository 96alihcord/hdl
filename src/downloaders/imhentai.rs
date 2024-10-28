use anyhow::{Context, Result};
use regex::Regex;

use hyper::Uri;
use tl::VDom;

use crate::{downloaders::Downloader, request::request};

use crate::downloaders::utils;

use super::utils::common_url_pattern_donwloader::CommonUrlPatternDownloader;
use super::utils::{CollectResponse, TagWithParser};

pub struct Imhentai {
    name: &'static str,
    authority: &'static str,
    path_re: Regex,
    pages_re: Regex,
    pages_selector: &'static str,
    title_selector: &'static [&'static str],
    gallery_selector: &'static str,
    full_image_url_attr: &'static str,
    full_image_selector: &'static str,
}

impl Imhentai {
    pub fn new() -> Self {
        Self {
            name: "Imhentai",
            authority: "imhentai.xxx",
            path_re: Regex::new(r"^/gallery/(?P<gallery_id>\d+)/?$").unwrap(),
            pages_selector: "li.pages",
            pages_re: Regex::new(r"Pages:\s+(?P<pages>\d+)").unwrap(),

            title_selector: &["div.right_details", "h1"],

            gallery_selector: "div#append_thumbs",

            full_image_url_attr: "data-src",
            full_image_selector: "img[data-src]",
        }
    }

    #[inline]
    fn is_gallery_path_match(&self, uri: &Uri) -> bool {
        self.path_re.is_match(uri.path())
    }
}

#[async_trait::async_trait]
impl CommonUrlPatternDownloader for Imhentai {
    fn get_pages_count(&self, dom: &VDom<'_>) -> Result<usize> {
        let selector = self.pages_selector;

        let parser = dom.parser();

        let mut pages = dom
            .query_selector(selector)
            .with_context(|| format!("failed to find selector: {selector}"))?;

        let node = pages
            .next()
            .with_context(|| format!("selector not found: {selector}"))?;

        let pages_txt = node
            .get(parser)
            .with_context(|| format!("failed to get node for: {selector}"))?
            .inner_text(parser);

        self.pages_re
            .captures(&pages_txt)
            .map(|captures| {
                Ok(captures["pages"]
                    .parse()
                    .context("failed to parse number")?)
            })
            .context("failed to get pages count")?
    }

    fn get_title(&self, html: &TagWithParser<'_, '_>) -> Result<String> {
        let title = html.query_selector_mutliple(self.title_selector.iter())?;
        Ok(title.tag.inner_text(title.parser).to_string())
    }

    fn get_first_image_url(&self, dom: &VDom<'_>) -> Result<Uri> {
        let parser = dom.parser();

        let gallery = dom
            .query_selector(self.gallery_selector)
            .and_then(|mut q| q.next())
            .and_then(|node| node.get(parser))
            .map(|node| node.outer_html(parser))
            .with_context(|| format!("selector not found: {}", self.gallery_selector))?;

        let gallery_dom = tl::parse(&gallery, Default::default())?;
        let parser = gallery_dom.parser();

        let url = gallery_dom
            .query_selector("a")
            .and_then(|mut q| q.next())
            .and_then(|node| node.get(parser))
            .and_then(|node| node.as_tag())
            .map(|tag| tag.attributes())
            .map(|attrs| -> Result<_> {
                Ok(attrs
                    .get("href")
                    .context("no 'href' attribute")?
                    .context("attribute is empty")?
                    .as_utf8_str())
            })
            .context("anchor link in gallery not found")??;

        Ok(Uri::try_from(url.to_string()).context("failed to create uri")?)
    }

    async fn get_image_pattern_from_first_image_page(&self, first_image_url: &Uri) -> Result<Uri> {
        let page = request(first_image_url).await?.collect_response().await?;

        let page = String::from_utf8_lossy(page.as_slice());
        let dom = tl::parse(&page, Default::default()).context("failed to parse html")?;
        let parser = dom.parser();

        let selector = self.full_image_selector;
        let img = dom
            .query_selector(selector)
            .and_then(|mut q| {
                q.find(|node| {
                    node.get(parser)
                        .and_then(|node| node.as_tag())
                        .map(|tag| tag.attributes())
                        .map(|attrs| attrs.get("id"))
                        .flatten()
                        .flatten()
                        .map(|id| id == "gimg")
                        .unwrap_or(false)
                })
            })
            .and_then(|node| node.get(parser))
            .and_then(|node| node.as_tag())
            .map(|tag| tag.attributes())
            .map(|attrs| -> Result<_> {
                Ok(attrs
                    .get(self.full_image_url_attr)
                    .with_context(|| format!("no '{}' attribute", self.full_image_url_attr))?
                    .context("attribute is empty")?
                    .as_utf8_str())
            })
            .with_context(|| format!("{first_image_url:?}: selctor not found: {selector}"))??;

        Ok(Uri::try_from(img.to_string()).context("failed to create uri")?)
    }
}

impl Downloader for Imhentai {
    fn is_gallery_match(&self, gallery: &Uri) -> bool {
        utils::is_supported_scheme(gallery)
            && utils::is_proper_authority(gallery, self.authority)
            && self.is_gallery_path_match(gallery)
    }

    fn name(&self) -> &'static str {
        self.name
    }
}
