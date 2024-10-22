//#![deny(warnings)]
#![warn(rust_2018_idioms)]

use std::path::PathBuf;
use std::sync::Arc;
use std::path::Path;

use clap::Parser;
use http_body_util::BodyExt;
use tokio::fs;
use tokio::io::AsyncWriteExt;
use tokio::sync::{mpsc, Semaphore};
use tokio::task::{JoinHandle, JoinSet};

use anyhow::{bail, Context, Result};

mod downloaders;
use downloaders::{downloaders, Downloader};
mod request;
use request::request;

mod progress;
use progress::progress_bar;

mod args;
use args::Args;

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    let url = &args.url;

    let out_dir = args.out_dir.clone().into();
    fs::create_dir_all(&out_dir).await?;

    for extractor in downloaders().iter() {
        let extractor = Arc::clone(extractor);
        if !extractor.is_gallery_match(&url) {
            continue;
        }

        return start_download(extractor, out_dir, &args).await;
    }

    bail!(format!("downloader not found for: {:?}", url))
}

async fn start_download(
    downloader: Arc<dyn Downloader>,
    out_dir: Arc<Path>,
    args: &Args,
) -> Result<()> {
    let semaphore = Arc::new(Semaphore::new(args.jobs));

    let urls = downloader.get_image_urls(&args.url).await?;
    let tasks_count = urls.len();

    let (tx, rx) = mpsc::channel::<progress::Msg>(args.jobs * 2);

    let mut set = JoinSet::<Result<()>>::new();

    let progress: JoinHandle<Result<()>> = tokio::spawn(async move {
        progress_bar(rx, tasks_count.try_into()?).await?;
        Ok(())
    });

    for (id, img) in urls.into_iter().enumerate() {
        let permit = semaphore.clone().acquire_owned().await?;

        let tx = tx.clone();
        let downloader = Arc::clone(&downloader);
        let out_dir = Arc::clone(&out_dir);

        set.spawn(async move {
            download_image(downloader, out_dir, tx, id, &img).await?;
            drop(permit);
            Ok(())
        });
    }

    while let Some(res) = set.join_next().await {
        res.context("failed to join async task")?
            .context("async task failed")?;
    }

    tx.send(progress::Msg::Quit).await?;
    progress.await??;
    println!();

    Ok(())
}

async fn download_image(
    downloader: Arc<dyn Downloader>,
    out_dir: Arc<Path>,
    tx: mpsc::Sender<progress::Msg>,
    id: usize,
    url: &hyper::Uri,
) -> Result<()> {
    use progress::{Msg, Status, Update};

    tx.send(Msg::Update(Update {
        id,
        status: Status::ResolvingUrl,
    }))
    .await?;
    let url = &downloader.resolve_image_url(&url).await?;

    // TODO: use gallery name as subdir
    let out_dir = out_dir.as_ref();
    let path = PathBuf::from(url.path());

    let file_name = path.file_name().context("missing file name")?;
    // TODO: use image number as file name
    let file_path = out_dir.join(&file_name);


    tx.send(Msg::Update(Update {
        id,
        status: Status::Starting(file_name.to_owned()),
    }))
    .await?;

    let mut response = request(url).await?;

    let mut file = fs::File::create(&file_path)
        .await
        .with_context(|| format!("failed to create file: {file_path:?}"))?;

    let mut downloading_sent = false;
    while let Some(next) = response.frame().await {
        if let Some(chunck) = next?.data_ref() {
            if !downloading_sent {
                downloading_sent = true;
                tx.send(Msg::Update(Update {
                    id,
                    status: Status::Downloading,
                }))
                .await?;
            }
            file.write_all(chunck).await?;
        }
    }

    tx.send(Msg::Update(Update {
        id,
        status: Status::Done,
    }))
    .await?;

    Ok(())
}
