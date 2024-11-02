use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;

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

const MAX_FILE_NAME_LEN: usize = 255;

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    let url = args.url.as_ref();

    for extractor in downloaders().iter() {
        if !extractor.is_gallery_match(url) {
            continue;
        }

        return start_download(Arc::clone(extractor), &args).await;
    }

    bail!(format!("downloader not found for: {:?}", url))
}

async fn start_download(downloader: Arc<dyn Downloader>, args: &Args) -> Result<()> {
    const BUFF_SZ: usize = 1024;
    let semaphore = Arc::new(Semaphore::new(args.jobs));

    let (parser_tx, mut parser_rx) = mpsc::channel::<downloaders::Msg>(BUFF_SZ);

    let (progress_tx, progress_rx) = mpsc::channel::<progress::Msg>(BUFF_SZ);

    let mut set = JoinSet::<Result<()>>::new();

    let name = downloader.name();

    // TODO: use real thread
    let progress: JoinHandle<Result<()>> = tokio::spawn(async move {
        progress_bar(progress_rx, name).await?;
        Ok(())
    });

    let parser_task = {
        let downloader = Arc::clone(&downloader);
        let url = args.url.inner();

        tokio::spawn(async move {
            downloader.start_parser_task(parser_tx, url).await;
        })
    };

    let mut id: usize = 0;
    let mut manga_dir = None;

    while let Some(msg) = parser_rx.recv().await {
        use downloaders::Msg;
        match msg {
            Msg::Title(title) => {
                let title = title
                    .replace('/', "_")
                    .chars()
                    .take(MAX_FILE_NAME_LEN)
                    .collect::<String>();
                let dir = args.out_dir.as_ref().join(title);
                fs::create_dir_all(&dir).await?;
                manga_dir = Some(Arc::from(dir));
            }
            Msg::Images(urls) => {
                let len = urls.len().try_into()?;
                progress_tx.send(progress::Msg::IncLen(len)).await?;

                let out_dir = manga_dir
                    .as_ref()
                    .expect("name message should be already received");

                for img in urls {
                    let permit = semaphore.clone().acquire_owned().await?;

                    let tx = progress_tx.clone();
                    let downloader = Arc::clone(&downloader);
                    let out_dir = Arc::clone(out_dir);

                    set.spawn(async move {
                        let img = &img;
                        download_image(downloader, out_dir, tx, id, img)
                            .await
                            .with_context(|| {
                                format!("failed to download {img:?} (task-id={id})")
                            })?;
                        drop(permit);
                        Ok(())
                    });

                    id += 1;
                }
            }
            Msg::Error(e) => {
                Err(e).context("error happend in downloader task")?;
            }
        }
    }

    while let Some(res) = set.join_next().await {
        if let Err(e) = res.context("failed to join async task")? {
            eprintln!("async task failed: {e}");
        }
    }

    parser_task.await?;
    progress_tx.send(progress::Msg::Quit).await?;
    progress.await??;

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
    let url = &downloader.resolve_image_url(url).await?;

    let out_dir = out_dir.as_ref();
    let path = PathBuf::from(url.path());

    let file_name = path.file_name().context("missing file name")?;
    // TODO: use image number as file name
    let file_path = out_dir.join(file_name);

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
