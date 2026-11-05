//! What the page reads off the disk rather than asking the daemon: the
//! system's Android setup, the setup service's progress and the space Android
//! takes. Read on the blocking pool, since walking the images is real IO.

use std::path::{Path, PathBuf};

use crownos_config::system::AndroidConfig;
use setup_state::{STATUS_PATH, SetupStatus};

use super::Storage;
use crate::util::tree_size::tree_size;

const IMAGES_DIR: &str = "/var/lib/waydroid/images";
/// Under `$HOME`, where Waydroid keeps every app's data.
const USER_DATA_DIR: &str = ".local/share/waydroid";

#[derive(Debug, Default)]
pub(super) struct Survey {
    pub(super) installed: Option<AndroidConfig>,
    pub(super) setup: Option<SetupStatus>,
    pub(super) storage: Storage,
}

pub(super) async fn survey() -> Survey {
    tokio::task::spawn_blocking(survey_now)
        .await
        .unwrap_or_default()
}

/// A file that cannot be read or parsed reads as absent: the page then shows
/// Android as not set up rather than an error the user cannot act on.
fn survey_now() -> Survey {
    Survey {
        installed: AndroidConfig::load().ok().flatten(),
        setup: setup_state::load(Path::new(STATUS_PATH)).ok().flatten(),
        storage: Storage {
            images_bytes: tree_size(Path::new(IMAGES_DIR)),
            data_bytes: user_data_dir().as_deref().and_then(tree_size),
        },
    }
}

fn user_data_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|home| Path::new(&home).join(USER_DATA_DIR))
}
