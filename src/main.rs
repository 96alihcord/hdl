mod args;
mod config;
mod downloaders;
mod progress;
mod request;

use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::thread;

use anyhow::anyhow;
use clap::Parser;
use http_body_util::BodyExt;
use tokio::fs;
use tokio::io::AsyncWriteExt;
use tokio::sync::{mpsc, Semaphore};
use tokio::task::JoinSet;

use anyhow::{bail, Context, Result};

use args::Args;
use downloaders::{downloaders, Downloader};
use progress::progress_bar;
use request::request;

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

    let (progress_tx, progress_rx) = std::sync::mpsc::channel::<progress::Msg>();

    let mut set = JoinSet::<Result<()>>::new();

    let name = downloader.name();

    let progress: thread::JoinHandle<Result<_>> = thread::spawn(|| {
        progress_bar(progress_rx, name)?;
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
                    .take(config::MAX_FILE_NAME_LEN)
                    .collect::<String>();
                let dir = args.out_dir.as_ref().join(title);
                fs::create_dir_all(&dir).await?;
                manga_dir = Some(Arc::from(dir));
            }
            Msg::Images(urls) => {
                let len = urls.len().try_into()?;
                progress_tx.send(progress::Msg::IncLen(len))?;

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
                        let mut done = false;
                        // TODO: do this properly
                        for _ in 0..config::DOWNLOAD_RETRIES {
                            let tx = tx.clone();
                            let out_dir = Arc::clone(&out_dir);
                            let downloader = Arc::clone(&downloader);

                            let timeout =
                                tokio::time::timeout(config::REQUEST_READ_TIMEOUT, async move {
                                    download_image(downloader, out_dir, tx, id, img)
                                        .await
                                        .with_context(|| {
                                            format!("failed to download {img:?} (task-id={id})")
                                        })
                                })
                                .await;
                            if let Ok(res) = timeout {
                                res?;
                                done = true;
                                break;
                            }
                        }
                        drop(permit);
                        if done {
                            Ok(())
                        } else {
                            bail!("download failed after {} retries", config::DOWNLOAD_RETRIES)
                        }
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
    progress_tx.send(progress::Msg::Quit)?;
    progress
        .join()
        .map_err(|e| anyhow!("progress-bar thread panicked: {:?}", e))
        .context("failed to join the spawned progress-bar thread")?
        .context("progress-bar thread returned an error")?;

    Ok(())
}

async fn download_image(
    downloader: Arc<dyn Downloader>,
    out_dir: Arc<Path>,
    tx: std::sync::mpsc::Sender<progress::Msg>,
    id: usize,
    url: &hyper::Uri,
) -> Result<()> {
    use progress::{Msg, Status, Update};

    tx.send(Msg::Update(Update {
        id,
        status: Status::ResolvingUrl,
    }))?;
    let url = &downloader.resolve_image_url(url).await?;

    let out_dir = out_dir.as_ref();
    let path = PathBuf::from(url.path());

    let file_name = path.file_name().context("missing file name")?;
    // TODO: use image number as file name
    let file_path = out_dir.join(file_name);

    tx.send(Msg::Update(Update {
        id,
        status: Status::Starting(file_name.to_owned()),
    }))?;

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
                }))?;
            }
            file.write_all(chunck).await?;
        }
    }

    tx.send(Msg::Update(Update {
        id,
        status: Status::Done,
    }))?;

    Ok(())
}
