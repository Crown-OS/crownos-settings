//! Handing a web address to the user's browser.

use std::process::{Command, Stdio};
use std::thread;

const OPENER: &str = "xdg-open";

/// Opens `url` without waiting for the browser; the opener is reaped off the
/// UI thread.
pub fn open_url(url: &str) {
    let spawned = Command::new(OPENER)
        .arg(url)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
    if let Ok(mut opener) = spawned {
        thread::spawn(move || opener.wait());
    }
}
