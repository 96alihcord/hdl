use std::{borrow::Cow, sync::{Arc, OnceLock}};

use anyhow::Result;
use hyper::Uri;

mod imhentai;
use imhentai::Imhentai;

mod ehentai;
use ehentai::Ehentai;

mod nhentai;
use nhentai::Nhentai;

pub mod utils;

#[async_trait::async_trait]
pub trait GetImageUrls: Sync + Send {
    async fn get_image_urls(&self, gallery: &Uri) -> Result<Vec<Uri>>;
}

#[async_trait::async_trait]
pub trait Downloader: Sync + Send + GetImageUrls {
    fn name(&self) -> &'static str;
    fn is_gallery_match(&self, gallery: &Uri) -> bool;

    async fn resolve_image_url<'a>(&self, url: &'a Uri) -> Result<Cow<'a, Uri>> {
        Ok(Cow::Borrowed(url))
    }
}

static DOWNLOADERS: OnceLock<Box<[Arc<dyn Downloader>]>> = OnceLock::new();
#[inline]
pub fn downloaders() -> &'static Box<[Arc<dyn Downloader>]> {
    DOWNLOADERS.get_or_init(|| {
        Box::new([
            Arc::new(Imhentai::new()),
            Arc::new(Ehentai::new()),
            Arc::new(Nhentai::new()),
        ])
    })
}
