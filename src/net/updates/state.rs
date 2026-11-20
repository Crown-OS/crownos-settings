//! What the System Updates page reads, and the pure folding of daemon events
//! into it.

use std::collections::VecDeque;

use tokio::sync::mpsc::UnboundedSender;

use super::{Failure, Phase, Snapshot, UpdatesCommand, UpdatesEvent, Verb};

/// The page shows only the end of the program output.
const LOG_LINES: usize = 8;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Link {
    /// The first attempt has not come back yet.
    #[default]
    Connecting,
    Online,
    Offline,
}

/// How far the running step is, as its last output line said.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Progress {
    pub phase: Phase,
    pub fraction: Option<f32>,
}

#[derive(Debug, Default)]
pub struct UpdatesState {
    sender: Option<UnboundedSender<UpdatesCommand>>,
    pub link: Link,
    pub snapshot: Option<Snapshot>,
    /// Cleared when the daemon moves to another phase.
    pub progress: Option<Progress>,
    /// The last output lines of the running step, oldest first.
    pub log: VecDeque<String>,
    /// The last failed verb, until the user dismisses it.
    pub error: Option<String>,
    dismissed_failure: Option<Failure>,
}

impl UpdatesState {
    pub(super) fn attach(&mut self, sender: UnboundedSender<UpdatesCommand>) {
        self.sender = Some(sender);
    }

    pub fn request(&mut self, command: UpdatesCommand) {
        self.error = None;
        let Some(sender) = self.sender.as_ref() else {
            return;
        };
        if sender.send(command).is_err() {
            self.sender = None;
        }
    }

    /// Whether the button for `verb` must stay disabled.
    pub fn refuses(&self, verb: Verb) -> bool {
        self.link != Link::Online
            || self
                .snapshot
                .as_ref()
                .is_none_or(|snapshot| snapshot.refusal(verb).is_some())
    }

    /// An interrupted update waits for `apply` to continue it.
    pub fn continues_update(&self) -> bool {
        self.snapshot
            .as_ref()
            .is_some_and(|snapshot| snapshot.in_progress.is_some())
    }

    /// The daemon's last failure, unless the user dismissed it.
    pub fn failure(&self) -> Option<&Failure> {
        self.snapshot
            .as_ref()?
            .failure
            .as_ref()
            .filter(|failure| self.dismissed_failure.as_ref() != Some(*failure))
    }

    pub fn dismiss_failure(&mut self) {
        self.dismissed_failure = self.snapshot.as_ref().and_then(|s| s.failure.clone());
        self.error = None;
    }

    pub fn dismiss_error(&mut self) {
        self.error = None;
    }

    /// The phase of the step that runs now, if it has a progress card. A
    /// check shows only its spinner in the status row.
    pub fn running_phase(&self) -> Option<Phase> {
        let phase = self
            .progress
            .map(|progress| progress.phase)
            .or_else(|| self.snapshot.as_ref().map(|snapshot| snapshot.phase))?;
        (!matches!(phase, Phase::Idle | Phase::Checking)).then_some(phase)
    }

    pub(super) fn apply(&mut self, event: UpdatesEvent) {
        match event {
            UpdatesEvent::Online => {
                self.link = Link::Online;
                self.error = None;
            }
            UpdatesEvent::Offline => {
                self.link = Link::Offline;
                self.snapshot = None;
                self.progress = None;
                self.log.clear();
            }
            UpdatesEvent::Snapshot(snapshot) => {
                if self
                    .progress
                    .is_some_and(|progress| progress.phase != snapshot.phase)
                {
                    self.progress = None;
                }
                if snapshot.failure.is_none() {
                    self.dismissed_failure = None;
                }
                self.log = last_lines(&snapshot.log_tail);
                self.snapshot = Some(*snapshot);
            }
            UpdatesEvent::Progress {
                phase,
                fraction,
                line,
            } => {
                self.progress = Some(Progress { phase, fraction });
                if !line.is_empty() {
                    if self.log.len() == LOG_LINES {
                        self.log.pop_front();
                    }
                    self.log.push_back(line);
                }
            }
            UpdatesEvent::Error(message) => self.error = Some(message),
            UpdatesEvent::SessionRestartFailed(message) => {
                self.error = Some(format!(
                    "The desktop did not restart: {message}. Log out and log in again to finish the update."
                ));
            }
        }
    }
}

fn last_lines(tail: &[String]) -> VecDeque<String> {
    tail[tail.len().saturating_sub(LOG_LINES)..]
        .iter()
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use tokio::sync::mpsc::unbounded_channel;

    use super::super::{
        ApplyStep, FailedStep, ReleaseVersion, RestartKind, Rollback, SnapperNumber,
    };
    use super::*;

    fn snapshot(phase: Phase, log_tail: &[&str]) -> Box<Snapshot> {
        Box::new(Snapshot {
            phase,
            log_tail: log_tail.iter().map(|line| (*line).to_owned()).collect(),
            ..Snapshot::default()
        })
    }

    fn progress(phase: Phase, line: &str) -> UpdatesEvent {
        UpdatesEvent::Progress {
            phase,
            fraction: Some(0.5),
            line: line.to_owned(),
        }
    }

    #[test]
    fn progress_keeps_only_the_last_lines() {
        let mut state = UpdatesState::default();
        for number in 0..20 {
            state.apply(progress(Phase::Downloading, &format!("line {number}")));
        }
        state.apply(progress(Phase::Downloading, ""));
        assert_eq!(state.log.len(), LOG_LINES);
        assert_eq!(state.log.front().map(String::as_str), Some("line 12"));
        assert_eq!(state.log.back().map(String::as_str), Some("line 19"));
        assert_eq!(state.running_phase(), Some(Phase::Downloading));

        state.apply(progress(Phase::Checking, "reading the manifest"));
        assert_eq!(state.running_phase(), None, "a check has no progress card");
    }

    #[test]
    fn a_new_phase_drops_the_old_progress_and_takes_the_daemons_log() {
        let mut state = UpdatesState::default();
        state.apply(progress(Phase::Downloading, "downloading crowndock 100%"));
        let installing = Phase::Applying(ApplyStep::Install);
        state.apply(UpdatesEvent::Snapshot(snapshot(
            installing,
            &["(1/2) upgrading"],
        )));
        assert_eq!(state.progress, None);
        assert_eq!(state.log, ["(1/2) upgrading"]);
        assert_eq!(state.running_phase(), Some(installing));

        state.apply(progress(installing, "(2/2) installing"));
        state.apply(UpdatesEvent::Snapshot(snapshot(installing, &[])));
        assert!(state.progress.is_some(), "the same phase keeps its bar");

        state.apply(UpdatesEvent::Snapshot(snapshot(Phase::Idle, &[])));
        assert_eq!(state.running_phase(), None);
    }

    #[test]
    fn going_offline_forgets_the_daemons_state() {
        let mut state = UpdatesState::default();
        state.apply(UpdatesEvent::Error("refused".to_owned()));
        state.apply(UpdatesEvent::Online);
        assert_eq!(state.error, None, "a new connection starts clean");
        state.apply(UpdatesEvent::Snapshot(snapshot(Phase::Idle, &["old"])));
        assert!(!state.refuses(Verb::Check));

        state.apply(UpdatesEvent::Offline);
        assert_eq!(state.link, Link::Offline);
        assert_eq!(state.snapshot, None);
        assert!(state.log.is_empty());
        assert!(state.refuses(Verb::Check));
    }

    #[test]
    fn a_dismissed_failure_stays_hidden_until_another_one() -> Result<(), Box<dyn std::error::Error>>
    {
        let from: ReleaseVersion = "0.4.0".parse()?;
        let failure = |message: &str| Failure {
            step: FailedStep::Check,
            message: message.to_owned(),
            rollback: Some(Rollback {
                snapshot: SnapperNumber(7),
                from: from.clone(),
            }),
        };
        let failed = |message: &str| {
            UpdatesEvent::Snapshot(Box::new(Snapshot {
                failure: Some(failure(message)),
                ..Snapshot::default()
            }))
        };
        let mut state = UpdatesState::default();
        state.apply(failed("no network"));
        assert!(state.failure().is_some());

        state.dismiss_failure();
        assert_eq!(state.failure(), None);
        state.apply(failed("no network"));
        assert_eq!(
            state.failure(),
            None,
            "the same snapshot again stays hidden"
        );

        state.apply(UpdatesEvent::Snapshot(snapshot(Phase::Checking, &[])));
        state.apply(failed("no network"));
        assert!(
            state.failure().is_some(),
            "a retry that fails the same way is a new failure"
        );
        state.dismiss_failure();

        state.apply(failed("bad signature"));
        assert_eq!(
            state.failure().map(|failure| failure.message.as_str()),
            Some("bad signature")
        );
        Ok(())
    }

    #[test]
    fn a_command_clears_the_last_error_and_reaches_the_worker() {
        let (sender, mut receiver) = unbounded_channel();
        let mut state = UpdatesState::default();
        state.attach(sender);
        state.apply(UpdatesEvent::Error(
            "Only an administrator can update this computer.".to_owned(),
        ));
        state.request(UpdatesCommand::Restart(RestartKind::Session));
        assert_eq!(state.error, None);
        assert_eq!(
            receiver.try_recv().ok(),
            Some(UpdatesCommand::Restart(RestartKind::Session))
        );
    }

    #[test]
    fn a_failed_session_restart_is_reported() {
        let mut state = UpdatesState::default();
        state.apply(UpdatesEvent::SessionRestartFailed(
            "the session is locked".to_owned(),
        ));
        assert_eq!(
            state.error.as_deref(),
            Some(
                "The desktop did not restart: the session is locked. Log out and log in again to finish the update."
            )
        );
    }
}
