//! The one loop: the compositor's events, the page's commands, and the
//! countdown that undoes an arrangement nobody confirmed.
//!
//! The countdown lives here rather than in the page for the reason it exists
//! at all — a bad arrangement can leave the user unable to click anything, so
//! the rollback must not depend on the interface still working.
//!
//! Commands are taken first, because a `select!` that preferred the socket
//! would leave an apply waiting behind an unrelated hotplug.

use tokio::select;
use tokio::sync::mpsc::UnboundedReceiver;
use tokio::time::{Instant, sleep_until};

use super::protocol::Session;
use super::rollback::Rollback;
use super::{MonitorDraft, Outcome, OutputCommand, OutputEvent, Purpose};
use crate::util::worker::MessageProxy;

/// What woke the loop up.
enum Woken {
    Command(OutputCommand),
    /// A second of the countdown passed.
    Tick,
    /// The compositor said something, which `dispatch` has dealt with.
    Compositor,
}

/// One configuration to send, and what it is for.
enum Job {
    Test(Vec<MonitorDraft>),
    Apply {
        drafts: Vec<MonitorDraft>,
        previous: Vec<MonitorDraft>,
    },
    Revert {
        previous: Vec<MonitorDraft>,
        expired: bool,
    },
}

impl Job {
    fn revert(rollback: Rollback, expired: bool) -> Self {
        Self::Revert {
            previous: rollback.into_previous(),
            expired,
        }
    }

    fn drafts(&self) -> &[MonitorDraft] {
        match self {
            Self::Test(drafts) | Self::Apply { drafts, .. } => drafts,
            Self::Revert { previous, .. } => previous,
        }
    }

    fn purpose(&self) -> Purpose {
        match self {
            Self::Test(_) => Purpose::Test,
            Self::Apply { .. } => Purpose::Apply,
            Self::Revert { .. } => Purpose::Revert,
        }
    }
}

/// The page's last word when the conversation ends, whatever ended it.
fn gone(err: &impl std::fmt::Display) -> String {
    format!("the compositor stopped talking to us: {err}")
}

pub async fn run(proxy: MessageProxy<OutputEvent>, mut commands: UnboundedReceiver<OutputCommand>) {
    let report = move |event| {
        // The window has gone; the loop ends on its next turn either way.
        let _ = proxy.message(event);
    };

    let mut session = match Session::connect() {
        Ok(session) => session,
        Err(reason) => {
            report(OutputEvent::Unavailable(reason));
            return;
        }
    };

    let mut rollback: Option<Rollback> = None;

    loop {
        let tick = rollback
            .as_ref()
            .map(|rollback| rollback.next_tick(Instant::now()));

        let woken = select! {
            biased;
            command = commands.recv() => match command {
                Some(command) => Woken::Command(command),
                None => return,
            },
            () = sleep_until(tick.unwrap_or_else(Instant::now)), if tick.is_some() => Woken::Tick,
            result = session.dispatch(&report) => {
                if let Err(err) = result {
                    report(OutputEvent::Unavailable(gone(&err)));
                    return;
                }
                Woken::Compositor
            }
        };

        let job = match woken {
            Woken::Compositor => None,
            Woken::Tick => match rollback
                .as_ref()
                .map(|rollback| rollback.seconds_left(Instant::now()))
            {
                Some(0) => rollback.take().map(|rollback| Job::revert(rollback, true)),
                Some(seconds_left) => {
                    report(OutputEvent::Countdown { seconds_left });
                    None
                }
                None => None,
            },
            Woken::Command(OutputCommand::Test(drafts)) => Some(Job::Test(drafts)),
            Woken::Command(OutputCommand::Apply { drafts, previous }) => {
                Some(Job::Apply { drafts, previous })
            }
            Woken::Command(OutputCommand::Confirm) => {
                rollback = None;
                None
            }
            Woken::Command(OutputCommand::Revert) => {
                rollback.take().map(|rollback| Job::revert(rollback, false))
            }
            Woken::Command(OutputCommand::SetBrightness { name, level }) => {
                session.set_brightness(&name, level);
                None
            }
        };

        let Some(job) = job else {
            continue;
        };
        let purpose = job.purpose();
        let outcome = match session
            .configure(job.drafts(), purpose == Purpose::Test, &report)
            .await
        {
            Ok(outcome) => outcome,
            Err(err) => {
                report(OutputEvent::Unavailable(gone(&err)));
                return;
            }
        };
        report(OutputEvent::Result {
            outcome: outcome.clone(),
            purpose,
        });

        match job {
            Job::Apply { previous, .. } => {
                if let Some(applied) = Rollback::after_apply(previous, &outcome, Instant::now()) {
                    rollback = Some(applied);
                }
            }
            Job::Revert { expired: true, .. } if outcome == Outcome::Succeeded => {
                report(OutputEvent::Reverted);
            }
            Job::Test(_) | Job::Revert { .. } => {}
        }
    }
}
