use clap::Parser;
use std::convert::Infallible;
use std::sync::Arc;
use std::path::{Path, PathBuf};
use std::str::FromStr;


pub struct ArcPath(Arc<Path>);

impl Clone for ArcPath {
    fn clone(&self) -> Self {
        ArcPath(Arc::clone(&self.0))
    }
}

impl Into<Arc<Path>> for ArcPath {
    fn into(self) -> Arc<Path> {
        self.0
    }
}

impl FromStr for ArcPath {
    type Err = Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(ArcPath(Arc::from(PathBuf::from(s))))
    }
}

#[derive(Parser)]
pub(crate) struct Args {
    /// parallel jobs count
    #[arg(short, long, default_value_t = 1)]
    pub(crate) jobs: usize,

    #[arg(short, long, default_value = "./out/")]
    pub(crate) out_dir: ArcPath,

    pub(crate) url: hyper::Uri,
}
