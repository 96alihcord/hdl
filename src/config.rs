use std::time::Duration;

pub(crate) const PROGRESS_BAR_TICK_TIME: Duration = Duration::from_millis(100);

pub(crate) const MAX_FILE_NAME_LEN: usize = 255;

pub(crate) const REQUEST_READ_TIMEOUT: Duration = Duration::from_secs(10);

pub(crate) const DOWNLOAD_RETRIES: usize = 3;
