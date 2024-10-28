use std::convert::Infallible;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Arc;

use clap::Parser;
use hyper::Uri;

pub struct ArcWrap<T: ?Sized>(Arc<T>);

impl<T: ?Sized> Clone for ArcWrap<T> {
    fn clone(&self) -> Self {
        Self(Arc::clone(&self.0))
    }
}

impl<T: ?Sized> AsRef<T> for ArcWrap<T> {
    fn as_ref(&self) -> &T {
        self.0.as_ref()
    }
}

impl<T: ?Sized> ArcWrap<T> {
    pub fn inner(&self) -> Arc<T> {
        Arc::clone(&self.0)
    }
}

impl FromStr for ArcWrap<Path> {
    type Err = Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(Arc::from(PathBuf::from(s))))
    }
}

impl FromStr for ArcWrap<Uri> {
    type Err = hyper::http::uri::InvalidUri;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(Arc::from(Uri::try_from(s)?)))
    }
}

#[derive(Parser)]
pub(crate) struct Args {
    /// parallel jobs count
    #[arg(short, long, default_value_t = 3)]
    pub(crate) jobs: usize,

    #[arg(short, long, default_value = "./out/")]
    pub(crate) out_dir: ArcWrap<Path>,

    pub(crate) url: ArcWrap<Uri>,
}
