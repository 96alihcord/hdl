use std::time::Instant;
use std::{collections::HashMap, ffi::OsString};

use std::sync::mpsc;

use anyhow::{Context, Result};
use indicatif::{MultiProgress, ProgressBar, ProgressDrawTarget, ProgressStyle};

use crate::config;

#[derive(Clone, Debug)]
pub enum Status {
    Starting(OsString),
    ResolvingUrl,
    Downloading,
    Done,
}

#[derive(Debug, Clone)]
pub struct Update {
    pub id: usize,
    pub status: Status,
}

#[derive(Debug)]
pub enum Msg {
    Update(Update),
    IncLen(u64),
    Quit,
}

#[inline]
fn is_tick_needed(now: Instant, last_tick: &Option<Instant>) -> Result<bool> {
    Ok(match last_tick {
        Some(last_tick) => {
            let threshold = now
                .checked_sub(config::PROGRESS_BAR_TICK_TIME)
                .context("failed to substitute time")?;

            &threshold > last_tick
        }
        None => true,
    })
}

pub fn progress_bar(rx: mpsc::Receiver<Msg>, name: &'static str) -> Result<()> {
    let progress = MultiProgress::new();
    let main_progress = progress.add({
        let style = ProgressStyle::with_template("{msg}: {wide_bar} {pos}/{len}")?;
        ProgressBar::with_draw_target(None, ProgressDrawTarget::stderr())
            .with_style(style)
            .with_message(name)
    });

    let mut bars = HashMap::<usize, (ProgressBar, Option<Instant>)>::new();

    loop {
        let rx = rx.recv_timeout(config::PROGRESS_BAR_TICK_TIME);

        for (_, (bar, last_tick)) in bars.iter_mut() {
            let now = Instant::now();
            if is_tick_needed(now, last_tick)? {
                *last_tick = Some(now);
                bar.tick();
            }
        }
        if let Ok(msg) = rx {
            match msg {
                Msg::IncLen(len) => {
                    if main_progress.length().is_none() {
                        main_progress.set_length(0);
                    }
                    main_progress.inc_length(len);
                }
                Msg::Update(Update { id, status }) => match status {
                    Status::ResolvingUrl => {
                        let style =
                            ProgressStyle::with_template("{spinner} {elapsed} {msg:19} {prefix}")?;
                        let bar = ProgressBar::new_spinner()
                            .with_style(style)
                            .with_message("Resolving Image Url");

                        bars.insert(id, (progress.add(bar), None));
                    }
                    Status::Starting(name) => {
                        let (bar, _) = bars
                            .get(&id)
                            .with_context(|| format!("failed to get bar with id={id}"))?;
                        bar.set_prefix(format!("{name:?}"));
                        bar.set_message("Starting");
                    }
                    Status::Downloading => {
                        let (bar, _) = bars
                            .get(&id)
                            .with_context(|| format!("failed to get bar with id={id}"))?;
                        bar.set_message("Downloading");
                    }
                    Status::Done => {
                        let (bar, _) = bars
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
                }
            }
        }
    }

    Ok(())
}
