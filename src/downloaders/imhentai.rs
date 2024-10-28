use anyhow::{Context, Result};
use regex::Regex;

use hyper::Uri;

use crate::{downloaders::Downloader, request::request};

use crate::downloaders::utils;

use super::utils::common_url_pattern_donwloader::{CommonUrlPatternDownloader, GalleryInfo};
use super::utils::{CollectResponse, GetHtmlTag, TagWithParser};

pub struct Imhentai {
    name: &'static str,
    authority: &'static str,
    path_re: Regex,

    info_selector: &'static str,
    title_selector: &'static str,
    pages_selector: &'static str,
    pages_re: Regex,

    img_url_attr: &'static str,
    first_image_selector: &'static [&'static str],
    full_image_selector: &'static [&'static str],
}

impl Imhentai {
    pub fn new() -> Self {
        Self {
            name: "Imhentai",
            authority: "imhentai.xxx",
            path_re: Regex::new(r"^/gallery/(?P<gallery_id>\d+)/?$").unwrap(),

            info_selector: "div.right_details",
            title_selector: "h1",
            pages_selector: "li.pages",
            pages_re: Regex::new(r"Pages:\s+(?P<pages>\d+)").unwrap(),

            img_url_attr: "data-src",
            first_image_selector: &["div#append_thumbs", "div.gthumb", "a"],
            full_image_selector: &["div.gview", "img#gimg"],
        }
    }

    #[inline]
    fn is_gallery_path_match(&self, uri: &Uri) -> bool {
        self.path_re.is_match(uri.path())
    }
}

#[async_trait::async_trait]
impl CommonUrlPatternDownloader for Imhentai {
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

        let pages_count = info
            .query_selector(html.parser, self.pages_selector)
            .and_then(|mut q| q.next())
            .and_then(|node| node.get(html.parser))
            .and_then(|node| node.as_tag())
            .with_context(|| format!("selctor not found: {}", self.pages_selector))?
            .inner_text(html.parser);

        let pages_count = self
            .pages_re
            .captures(&pages_count)
            .map(|captures| captures["pages"].parse().context("failed to parse number"))
            .context("failed to get pages count")??;

        Ok(GalleryInfo { pages_count, title })
    }

    fn get_first_image_url(&self, html: &TagWithParser<'_, '_>) -> Result<Uri> {
        let anchor = html.query_selector_mutliple(self.first_image_selector.iter())?;

        let attr = "href";
        let url = anchor
            .tag
            .attributes()
            .get(attr)
            .with_context(|| format!("no {attr:?} attribute"))?
            .with_context(|| format!("empty {attr:?} attribute"))?;

        Ok(Uri::try_from(url.as_bytes()).context("failed to create uri")?)
    }

    async fn get_image_pattern_from_first_image_page(&self, first_image_url: &Uri) -> Result<Uri> {
        let page = request(first_image_url).await?.collect_response().await?;

        let page = String::from_utf8_lossy(page.as_slice());
        let dom = tl::parse(&page, Default::default()).context("failed to parse html")?;
        let html = &dom.get_html_tag()?;

        let img_tag = html
            .query_selector_mutliple(self.full_image_selector.iter())?;


        let attr = self.img_url_attr;
        let img = img_tag
            .tag
            .attributes()
            .get(attr)
            .with_context(|| format!("no {attr:?} attribute"))?
            .with_context(|| format!("empty {attr:?} attribute"))?;

        Ok(Uri::try_from(img.as_bytes()).context("failed to create uri")?)
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
