//! Online accounts kept away from the real ones: `crownos-accounts` reads
//! `$XDG_CONFIG_HOME/crownos/accounts`, pointed here at an empty sample home
//! in this crate's `target` folder.

use std::path::PathBuf;

use anyhow::{Context, Result};

fn sample_config_home() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/home/alex/.config")
}

/// Points the accounts store at an empty sample home.
pub fn isolate() -> Result<()> {
    let home = sample_config_home();
    if home.exists() {
        std::fs::remove_dir_all(&home).context("clearing the sample accounts")?;
    }
    std::fs::create_dir_all(&home).context("creating the sample accounts folder")?;
    // SAFETY: nothing else runs yet; the capture is single-threaded up to here.
    unsafe { std::env::set_var("XDG_CONFIG_HOME", &home) };
    Ok(())
}

/// Removes the sample home again.
pub fn clean_up() {
    let _ = std::fs::remove_dir_all(sample_config_home());
}
