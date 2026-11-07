//! Handing a web address to the user's browser.

use std::process::{Command, Stdio};

/// Opens `url` with `xdg-open`, without waiting for the browser. The launcher
/// is reaped on a thread of its own, so it never lingers as a zombie.
pub fn open_in_browser(url: &str) -> std::io::Result<()> {
    let mut launcher = Command::new("xdg-open")
        .arg(url)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    std::thread::spawn(move || launcher.wait());
    Ok(())
}
