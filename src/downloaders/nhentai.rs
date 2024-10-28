use anyhow::{Context, Result};
use hyper::Uri;
use regex::Regex;

use crate::request::request;

use super::utils::common_url_pattern_donwloader::GalleryInfo;
use super::utils::{self, common_url_pattern_donwloader::CommonUrlPatternDownloader};
use super::utils::{CollectResponse, TagWithParser};
use super::Downloader;

pub struct Nhentai {
    name: &'static str,
    authority: &'static str,

    path_re: Regex,

    info_selector: &'static str,
    title_selector: &'static str,

    info_field_selector: &'static str,
    info_value_selector: &'static str,

    first_image_selector: &'static [&'static str],

    img_section_selector: &'static str,
    img_section_img_selector: &'static str,
}

impl Nhentai {
    pub fn new() -> Self {
        Self {
            name: "Nhentai",
            authority: "nhentai.net",

            path_re: Regex::new(r"^/g/(?P<gallery_id>\d+)/?$").unwrap(),

            info_selector: "div#info",
            title_selector: "h1.title",

            first_image_selector: &["div.thumbs", "a.gallerythumb"],

            info_field_selector: "div.field-name",
            info_value_selector: "span.name",

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
    fn get_info(&self, html: &TagWithParser<'_, '_>) -> Result<GalleryInfo> {
        let info = html
            .query_selector(self.info_selector)
            .and_then(|mut q| q.next())
            .and_then(|node| node.get(html.parser))
            .and_then(|node| node.as_tag())
            .with_context(|| format!("selector not found: {}", self.info_selector))?;

        let title = info
            .query_selector(html.parser, self.title_selector)
            .and_then(|mut q| q.next())
            .and_then(|node| node.get(html.parser))
            .and_then(|node| node.as_tag())
            .with_context(|| format!("selector not found: {}", self.title_selector))?
            .inner_text(html.parser)
            .to_string();

        let pages_count_tag = info
            .query_selector(html.parser, self.info_field_selector)
            .and_then(|mut q| {
                q.find(|node| {
                    node.get(html.parser)
                        .map(|node| node.inner_text(html.parser).trim().starts_with("Pages:"))
                        .unwrap_or(false)
                })
            })
            .and_then(|node| node.get(html.parser))
            .and_then(|node| node.as_tag())
            .with_context(|| format!("selctor not found: {}", self.info_selector))?;

        let pages_count = pages_count_tag
            .query_selector(html.parser, self.info_value_selector)
            .and_then(|mut q| q.next())
            .and_then(|node| node.get(html.parser))
            .and_then(|node| node.as_tag())
            .with_context(|| format!("selctor not found: {}", self.info_value_selector))?
            .inner_text(html.parser)
            .parse()
            .context("failed to parse pages count (usize)")?;

        Ok(GalleryInfo { pages_count, title })
    }

    fn get_first_image_url(&self, html: &TagWithParser<'_, '_>) -> Result<Uri> {
        let anchor = html.query_selector_mutliple(self.first_image_selector.iter())?;

        let img_url = anchor
            .tag
            .attributes()
            .get("href")
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
}
