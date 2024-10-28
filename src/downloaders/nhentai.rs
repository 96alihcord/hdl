use anyhow::{Context, Result};
use hyper::Uri;
use regex::Regex;
use tl::VDom;

use crate::request::request;

use super::utils::{self, common_url_pattern_donwloader::CommonUrlPatternDownloader};
use super::utils::{CollectResponse, TagWithParser};
use super::Downloader;

pub struct Nhentai {
    name: &'static str,
    authority: &'static str,

    path_re: Regex,

    title_selector: &'static [&'static str],

    info_selector: &'static str,
    info_field_selector: &'static str,
    info_pages_selector: &'static str,

    gallery_selector: &'static str,
    gallery_img_selector: &'static str,

    img_section_selector: &'static str,
    img_section_img_selector: &'static str,
}

impl Nhentai {
    pub fn new() -> Self {
        Self {
            name: "Nhentai",
            authority: "nhentai.net",

            path_re: Regex::new(r"^/g/(?P<gallery_id>\d+)/?$").unwrap(),

            title_selector: &["div#info", "h1.title"],

            info_selector: "section#tags",
            info_field_selector: "div.field-name",
            info_pages_selector: "span.name",

            gallery_selector: "div.thumbs",
            gallery_img_selector: "a.gallerythumb",

            img_section_selector: "section#image-container",
            img_section_img_selector: "img[src]",
        }
    }

    #[inline]
    fn is_gallery_path_match(&self, uri: &Uri) -> bool {
        self.path_re.is_match(uri.path())
    }
}

impl Downloader for Nhentai {
    fn is_gallery_match(&self, gallery: &Uri) -> bool {
        utils::is_supported_scheme(gallery)
            && utils::is_proper_authority(gallery, self.authority)
            && self.is_gallery_path_match(gallery)
    }

    fn name(&self) -> &'static str {
        self.name
    }
}

#[async_trait::async_trait]
impl CommonUrlPatternDownloader for Nhentai {
    fn get_pages_count(&self, dom: &VDom<'_>) -> Result<usize> {
        let parser = dom.parser();
        let info = dom
            .query_selector(self.info_selector)
            .and_then(|mut q| q.next())
            .and_then(|node| node.get(parser))
            .map(|node| node.inner_html(parser))
            .with_context(|| format!("selctor not found: {}", self.info_selector))?;

        let info_dom = tl::parse(&info, Default::default())?;
        let parser = info_dom.parser();

        let pages_info = info_dom
            .query_selector(self.info_field_selector)
            .and_then(|mut q| {
                q.find(|node| {
                    node.get(parser)
                        .map(|node| node.inner_text(parser).trim().starts_with("Pages:"))
                        .unwrap_or(false)
                })
            })
            .and_then(|node| node.get(parser))
            .map(|node| node.inner_html(parser))
            .with_context(|| format!("selctor not found: {}", self.info_selector))?;

        let pages_info_dom = tl::parse(&pages_info, Default::default())?;
        let parser = pages_info_dom.parser();

        let pages = pages_info_dom
            .query_selector(self.info_pages_selector)
            .and_then(|mut q| q.next())
            .and_then(|node| node.get(parser))
            .map(|node| node.inner_text(parser))
            .with_context(|| format!("selctor not found: {}", self.info_pages_selector))?;

        pages.parse().context("failed to parse pages count (usize)")
    }

    fn get_first_image_url(&self, dom: &VDom<'_>) -> Result<Uri> {
        let parser = dom.parser();
        let gallery = dom
            .query_selector(self.gallery_selector)
            .and_then(|mut q| q.next())
            .and_then(|node| node.get(parser))
            .map(|node| node.inner_html(parser))
            .with_context(|| format!("selctor not found: {}", self.gallery_selector))?;

        let gallery_dom = tl::parse(&gallery, Default::default())?;
        let parser = gallery_dom.parser();

        let img_url = gallery_dom
            .query_selector(self.gallery_img_selector)
            .and_then(|mut q| q.next())
            .and_then(|node| node.get(parser))
            .and_then(|node| node.as_tag())
            .map(|tag| tag.attributes())
            .and_then(|attrs| attrs.get("href"))
            .context("no 'href' attribute")?
            .context("empty 'href' attribute")?;

        Ok(Uri::try_from(img_url.as_bytes())?)
    }

    async fn get_image_pattern_from_first_image_page(&self, first_image_url: &Uri) -> Result<Uri> {
        let page = request(first_image_url).await?.collect_response().await?;
        let page = String::from_utf8_lossy(&page);

        let dom = tl::parse(&page, Default::default())?;
        let parser = dom.parser();

        let section = dom
            .query_selector(self.img_section_selector)
            .and_then(|mut q| q.next())
            .and_then(|node| node.get(parser))
            .map(|node| node.inner_html(parser))
            .with_context(|| format!("selctor not found: {}", self.img_section_selector))?;

        let section_dom = tl::parse(&section, Default::default())?;
        let parser = section_dom.parser();

        let url = section_dom
            .query_selector(self.img_section_img_selector)
            .and_then(|mut q| q.next())
            .and_then(|node| node.get(parser))
            .and_then(|node| node.as_tag())
            .map(|tag| tag.attributes())
            .and_then(|attrs| attrs.get("src"))
            .context("no 'src' attribute in 'img' tag")?
            .context("empty 'src' attribute")?;

        Ok(Uri::try_from(url.as_bytes())?)
    }

    fn get_title(&self, html: &TagWithParser<'_, '_>) -> Result<String> {
        let title = html.query_selector_mutliple(self.title_selector.iter())?;
        Ok(title.tag.inner_text(title.parser).to_string())
    }
}
