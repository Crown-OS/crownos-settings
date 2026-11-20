//! The system update backend: `crownos-updated`, the root daemon that checks,
//! downloads and installs the next CrownOS release.
//!
//! The house shape (see [`crate::net`]): [`UpdatesCommand`] for the four
//! verbs, [`UpdatesEvent`] for what the daemon and the compositor say, and
//! [`UpdatesState`] in the [`Store`](crate::state::Store). The daemon's
//! snapshot types are re-exported, not mirrored.

mod daemon;
mod state;
mod worker;

pub use state::{Link, UpdatesState};
pub use update_client::proto::{
    ApplyStep, DownloadStatus, FailedStep, Failure, PendingRestart, Phase, RestartKind, Snapshot,
    Verb,
};
#[cfg(test)]
pub use update_client::proto::{InProgress, NextRelease, ReleaseVersion, Rollback, SnapperNumber};

use crownui::prelude::{Cx, Signal};

use crate::util::worker::spawn_worker;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdatesCommand {
    Check,
    Download,
    Apply,
    /// Carries the kind the restart card shows: a session restart is the
    /// compositor's job after the daemon accepts it.
    Restart(RestartKind),
}

impl UpdatesCommand {
    pub fn verb(self) -> Verb {
        match self {
            Self::Check => Verb::Check,
            Self::Download => Verb::Download,
            Self::Apply => Verb::Apply,
            Self::Restart(_) => Verb::Restart,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum UpdatesEvent {
    /// Connected and subscribed; the snapshot follows.
    Online,
    /// The daemon cannot be reached. Retries are already scheduled.
    Offline,
    Snapshot(Box<Snapshot>),
    Progress {
        phase: Phase,
        fraction: Option<f32>,
        line: String,
    },
    /// A verb failed. Already human-readable.
    Error(String),
    /// The daemon finished the update, but the compositor did not restart
    /// the session.
    SessionRestartFailed(String),
}

/// Starts the update backend for as long as the calling component lives.
pub fn updates_worker(cx: &mut Cx, updates: Signal<UpdatesState>) {
    let sender = spawn_worker(cx, worker::run, move |cx, event| {
        updates.update(cx, |state| state.apply(event));
    });
    updates.update(cx, |state| state.attach(sender));
}
