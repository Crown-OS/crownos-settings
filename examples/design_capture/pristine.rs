//! The settings every window starts from. A window's store persists what a
//! capture sets up, so each window first puts the run's config folder back
//! the way the run found it.

use std::path::PathBuf;
use std::sync::OnceLock;

use anyhow::{Context, Result};
use crownos_config::CONFIG_DIR_ENV;

use crate::util::persist;

struct Snapshot {
    dir: PathBuf,
    files: Vec<(PathBuf, Vec<u8>)>,
}

/// Puts the config folder back as it was when the run started.
pub fn restore_config() -> Result<()> {
    persist::flush();
    let snapshot = snapshot()?;
    for entry in std::fs::read_dir(&snapshot.dir).context("listing the config folder")? {
        let path = entry.context("reading the config folder")?.path();
        let original = snapshot.files.iter().any(|(kept, _)| *kept == path);
        if path.is_file() && !original {
            std::fs::remove_file(&path).context("removing a setting the capture wrote")?;
        }
    }
    for (path, contents) in &snapshot.files {
        std::fs::write(path, contents).context("restoring a setting")?;
    }
    Ok(())
}

fn snapshot() -> Result<&'static Snapshot> {
    static SNAPSHOT: OnceLock<Snapshot> = OnceLock::new();
    if let Some(snapshot) = SNAPSHOT.get() {
        return Ok(snapshot);
    }
    let dir = std::env::var_os(CONFIG_DIR_ENV)
        .filter(|dir| !dir.is_empty())
        .map(PathBuf::from)
        .context("CROWN_CONFIG_DIR names the run's own copy of the settings")?;
    let files = std::fs::read_dir(&dir)
        .context("listing the config folder")?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_file())
        .map(|path| std::fs::read(&path).map(|contents| (path, contents)))
        .collect::<std::io::Result<_>>()
        .context("reading the config folder")?;
    Ok(SNAPSHOT.get_or_init(|| Snapshot { dir, files }))
}
