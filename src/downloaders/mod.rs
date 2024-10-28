use std::{
    borrow::Cow,
    sync::{Arc, OnceLock},
};

use anyhow::Result;
use hyper::Uri;
use tokio::sync::mpsc::Sender;

mod imhentai;
use imhentai::Imhentai;

mod ehentai;
use ehentai::Ehentai;

mod nhentai;
use nhentai::Nhentai;

pub mod utils;

pub enum Msg {
    Title(String),
    Images(Vec<Uri>),
    Error(anyhow::Error),
}

#[async_trait::async_trait]
pub trait GetImageUrls: Sync + Send {
    // TODO: do it somehow properly
    async fn start_parser_task_result(
        self: Arc<Self>,
        tx: Sender<Msg>,
        gallery: Arc<Uri>,
    ) -> Result<()>;
    async fn start_parser_task(self: Arc<Self>, tx: Sender<Msg>, gallery: Arc<Uri>) {
        match self.start_parser_task_result(tx.clone(), gallery).await {
            Err(e) => tx.send(Msg::Error(e)).await.expect("failed to send"),
            Ok(()) => return,
        }
    }
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
