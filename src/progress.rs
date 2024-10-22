use std::{collections::HashMap, ffi::OsString};

use tokio::sync::mpsc;

use anyhow::{Context, Result};
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};

#[derive(Clone)]
#[derive(Debug)]
pub enum Status {
    Starting(OsString),
    ResolvingUrl,
    Downloading,
    Done,
}

#[derive(Debug)]
#[derive(Clone)]
pub struct Update {
    pub id: usize,
    pub status: Status,
}

#[derive(Debug)]
pub enum Msg {
    Update(Update),
    Quit,
}

pub async fn progress_bar(mut rx: mpsc::Receiver<Msg>, len: u64) -> Result<()> {
    let progress = MultiProgress::new();
    let main_progress = progress.add(ProgressBar::new(len));
    main_progress.tick();

    let mut bars = HashMap::<usize, ProgressBar>::new();

    while let Some(msg) = rx.recv().await {
        match msg {
            Msg::Update(Update { id, status }) => match status {
                Status::ResolvingUrl => {
                    let style =
                        ProgressStyle::with_template("{spinner} {elapsed} {msg:19} {prefix}")?;
                    let bar = ProgressBar::new_spinner()
                        .with_style(style)
                        .with_message("Resolving Image Url");

                    bars.insert(id, progress.add(bar));

                }
                Status::Starting(name) => {
                    let bar = bars
                        .get(&id)
                        .with_context(|| format!("failed to get bar with id={id}"))?;
                    bar.set_prefix(format!("{name:?}"));
                    bar.set_message("Starting");

                }
                Status::Downloading => {
                    let bar = bars
                        .get(&id)
                        .with_context(|| format!("failed to get bar with id={id}"))?;
                    bar.set_message("Downloading");
                }
                Status::Done => {
                    let bar = bars
                        .get(&id)
                        .with_context(|| format!("failed to get bar with id={id}"))?;
                    bar.finish_with_message("Done");
                    main_progress.inc(1);
                    progress.remove(bar);
                }
            },
            Msg::Quit => {
                progress.clear()?;
                break;
            },
        }
    }

    Ok(())
}
