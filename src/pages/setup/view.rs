//! What the Web Browser and Developer Tools pages share: the setup status,
//! read when the page opens and again while a task runs, the requests that
//! change it, and the notices about it.
//!
//! The status is `None` until the first read arrives. Until then the rows say
//! "Checking…", the controls stay locked, and no notice shows.

use std::future::Future;
use std::time::Duration;

use anyhow::Result;
use crownos_config::system::SetupConfig;
use crownui::kit::{CalloutOptions, Tone, callout};
use crownui::prelude::{Cx, Runtime, Signal, View};

use crate::layout::{dismissible_failure, optional};
use crate::net::setup::{SetupStatus, setup_notice, tasks_unchanged};
use crate::util::pkexec::AdminAction;

/// How often a task in progress is looked at again while the page is open.
const RECHECK_INTERVAL: Duration = Duration::from_secs(5);
const CHECKING: &str = "Checking…";

#[derive(Clone, Copy)]
pub struct SetupView {
    status: Signal<Option<SetupStatus>>,
    applying: Signal<bool>,
    /// Set when an applied request has not shown in the status yet, so the
    /// recheck runs until it does.
    awaiting_change: Signal<bool>,
    failure: Signal<Option<String>>,
}

impl SetupView {
    pub fn open(cx: &mut Cx) -> Self {
        let view = Self {
            status: cx.signal(None),
            applying: cx.signal(false),
            awaiting_change: cx.signal(false),
            failure: cx.signal(None),
        };
        cx.spawn(SetupStatus::read(), move |cx, now| {
            view.status.set(cx, Some(now));
        });
        view.recheck_while_busy(cx);
        view
    }

    /// Sends `request`, then reads the status again. The status is set even
    /// when it did not change, so a control the user moved snaps back after a
    /// declined password prompt.
    pub fn apply(
        self,
        cx: &mut Cx,
        request: impl Future<Output = Result<AdminAction>> + Send + 'static,
    ) {
        self.applying.set(cx, true);
        self.failure.set(cx, None);
        let applied = async move {
            let outcome = request.await;
            (SetupStatus::read().await, outcome)
        };
        cx.spawn(applied, move |cx, (now, outcome)| {
            let tasks_still_the_same = self.status.with_untracked(cx, |before| {
                before
                    .as_ref()
                    .is_some_and(|before| tasks_unchanged(before, &now))
            });
            let applied = matches!(outcome, Ok(AdminAction::Applied));
            self.applying.set(cx, false);
            self.awaiting_change
                .set(cx, applied && tasks_still_the_same);
            self.failure
                .set(cx, outcome.err().map(|error| format!("{error:#}")));
            self.status.set(cx, Some(now));
        });
    }

    pub fn read<T>(self, runtime: &mut Runtime, read: impl FnOnce(&SetupStatus) -> T) -> Option<T> {
        self.status
            .with(runtime, |status| status.as_ref().map(read))
    }

    pub fn line(
        self,
        runtime: &mut Runtime,
        describe: impl FnOnce(&SetupStatus) -> String,
    ) -> String {
        self.read(runtime, describe)
            .unwrap_or_else(|| CHECKING.to_owned())
    }

    pub fn config(self, runtime: &mut Runtime) -> SetupConfig {
        self.read(runtime, |status| status.config)
            .unwrap_or_default()
    }

    /// True while nothing can be changed: before the first read, while a
    /// request runs, when the runner is not installed, and when the choices
    /// shown are defaults because the real ones could not be read.
    pub fn is_locked(self, runtime: &mut Runtime) -> bool {
        let accepts_changes = self
            .read(runtime, |status| {
                status.runner_available && status.config_error.is_none()
            })
            .unwrap_or(false);
        self.applying.get(runtime) || !accepts_changes
    }

    pub fn notice(self) -> impl View {
        optional(
            move |runtime| self.read(runtime, setup_notice).flatten(),
            |_, notice| callout(Tone::Warning, notice.title).body(notice.detail.into_owned()),
        )
    }

    pub fn failure(self) -> impl View {
        dismissible_failure(self.failure)
    }

    fn recheck_while_busy(self, cx: &mut Cx) {
        let pause = async { tokio::time::sleep(RECHECK_INTERVAL).await };
        cx.spawn(pause, move |cx, ()| {
            let busy = self.status.with_untracked(cx, |status| {
                status.as_ref().is_some_and(SetupStatus::is_busy)
            });
            if busy
                || self
                    .awaiting_change
                    .with_untracked(cx, |awaiting| *awaiting)
            {
                cx.spawn(SetupStatus::read(), move |cx, now| {
                    let changed = self
                        .status
                        .with_untracked(cx, |status| status.as_ref() != Some(&now));
                    if changed {
                        self.awaiting_change.set(cx, false);
                        self.status.set(cx, Some(now));
                    }
                });
            }
            self.recheck_while_busy(cx);
        });
    }
}
