//! The monitors, as the compositor has them.
//!
//! The house shape (see [`crate::net`]): a command enum, an event enum, and a
//! [`OutputState`] in the [`Store`](crate::state::Store). Like the other two
//! backends it is one long-lived future on CrownUI's tokio runtime — see
//! [`protocol`] for why the Wayland socket is polled rather than blocked on.
//!
//! ## Why a second Wayland connection
//!
//! CrownUI already holds one for the window. This is a separate conversation
//! about every monitor in the session rather than anything to do with this
//! app's surface, and `zwlr_output_management_v1` is not something CrownUI
//! exposes — so it gets its own connection, which is what every other tool
//! that speaks this protocol does too.
//!
//! ## Applying is a two-step
//!
//! A bad arrangement can leave a user unable to see the window that made it,
//! so applying starts a countdown: the change goes in, and unless it is
//! confirmed within [`REVERT_AFTER`] the previous arrangement is put back.
//! That is only possible because the protocol applies atomically — the same
//! reason the page drives the compositor over it rather than by writing the
//! config file.
//!
//! ## Placed by hand, packed when an edit makes them collide
//!
//! The arrangement canvas places monitors with [`OutputState::place`], which
//! never repacks. A position stops being valid the moment a monitor's *size*
//! changes — a coarser scale, a bigger mode, a rotation — and a compositor is
//! right to refuse an arrangement whose monitors overlap. So
//! [`OutputState::edit`] repacks the drafts left to right whenever such an
//! edit has made them collide. A monitor switched back on goes right of the
//! others first: a disabled head is reported at the origin, on top of
//! everything else.

mod brightness;
mod info;
mod monitor;
mod protocol;
mod rollback;
mod wl_outputs;
mod worker;

use std::time::Duration;

use crownui::prelude::{Cx, Signal};
use tokio::sync::mpsc::UnboundedSender;

use crate::util::worker::spawn_worker;
pub use monitor::{
    Brightness, BrightnessControl, InfoState, Mode, Monitor, MonitorDraft, Rect, Transform, beside,
    bounds, intersects, logical_size,
};

/// How long a newly applied arrangement has to be confirmed before it is
/// rolled back.
///
/// Long enough to find the mouse on a monitor that just moved, short enough
/// that a black screen is not a reason to reboot.
pub const REVERT_AFTER: Duration = Duration::from_secs(15);

/// How a configuration ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Succeeded,
    Failed(String),
    /// The monitors changed while the configuration was in flight.
    Cancelled,
}

/// What a configuration was sent for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Purpose {
    /// Only checked; nothing changed.
    Test,
    /// The user's drafts; kept only once confirmed.
    Apply,
    /// The arrangement from before the last apply, put back.
    Revert,
}

/// Where an apply is in its two steps.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Confirmation {
    /// Sent; the compositor has not answered yet.
    Awaiting,
    /// Applied, and put back unless kept in time.
    Countdown { seconds_left: u64 },
}

/// Everything the page can ask of this backend.
#[derive(Debug, Clone, PartialEq)]
pub enum OutputCommand {
    /// Check whether an arrangement would work, changing nothing.
    Test(Vec<MonitorDraft>),
    /// Apply it, starting the confirmation countdown.
    Apply {
        drafts: Vec<MonitorDraft>,
        /// What to put back if nobody confirms.
        previous: Vec<MonitorDraft>,
    },
    /// Keep the arrangement; stop the countdown.
    Confirm,
    /// Put the previous arrangement back now.
    Revert,
    /// `level` is perceptual, from 0.0 to 1.0.
    SetBrightness { name: String, level: f32 },
}

/// Everything the backend can tell the page.
#[derive(Debug, Clone, PartialEq)]
pub enum OutputEvent {
    /// The monitors, as they are now.
    Snapshot(Vec<Monitor>),
    /// A configuration finished. A refusal never undoes the drafts: the
    /// arrangement on screen did not change, so the user can fix the edit.
    Result { outcome: Outcome, purpose: Purpose },
    /// The countdown expired and the previous arrangement was put back.
    Reverted,
    /// There will be no monitors: no compositor, or no support for this.
    /// Already human-readable; shown as-is.
    Unavailable(String),
}

/// The monitors half of the app's [`Store`](crate::state::Store).
#[derive(Debug, Default)]
pub struct OutputState {
    sender: Option<UnboundedSender<OutputCommand>>,
    /// What the compositor last said. Empty until the first snapshot, which is
    /// what the page draws its "looking for monitors" state from.
    pub monitors: Vec<Monitor>,
    /// Whether a snapshot has ever arrived, separating "still looking" from
    /// "looked, and there is nothing".
    pub loaded: bool,
    /// The edits the user has made but not applied, by monitor name.
    pub drafts: Vec<MonitorDraft>,
    /// Which monitor's settings are on screen.
    pub selected: Option<String>,
    /// Set from Apply until the arrangement is kept, refused or put back.
    pub confirmation: Option<Confirmation>,
    /// Why there are no monitors, when that is the situation.
    pub unavailable: Option<String>,
    /// The last failure, until the user does something else.
    pub error: Option<String>,
}

impl OutputState {
    fn attach(&mut self, sender: UnboundedSender<OutputCommand>) {
        self.sender = Some(sender);
    }

    /// Ask the backend to do something.
    ///
    /// Silent when there is no backend, for the same reason the other two are:
    /// it is not a condition the interface can repair, and a settings app must
    /// not panic because a compositor lacks a protocol.
    pub fn send(&mut self, command: OutputCommand) {
        let Some(sender) = self.sender.as_ref() else {
            return;
        };
        if sender.send(command).is_err() {
            self.sender = None;
        }
    }

    fn apply_event(&mut self, event: OutputEvent) {
        match event {
            OutputEvent::Snapshot(monitors) => {
                // A draft the user has edited survives; one that only ever
                // said "leave it alone" follows the monitor. Keeping the
                // second kind would strand the page on "unapplied changes"
                // forever the moment anything else — `wlr-randr`, a config
                // reload, this app's own apply — moved a monitor.
                self.drafts = monitors
                    .iter()
                    .map(|monitor| self.rebased(monitor))
                    .collect();

                if self
                    .selected
                    .as_ref()
                    .is_none_or(|name| !monitors.iter().any(|monitor| monitor.name == *name))
                {
                    self.selected = monitors.first().map(|monitor| monitor.name.clone());
                }

                self.monitors = monitors;
                self.loaded = true;
                self.unavailable = None;
            }
            OutputEvent::Result { outcome, purpose } => self.finish(outcome, purpose),
            OutputEvent::Reverted => {
                self.confirmation = None;
                self.reset_drafts();
                self.error =
                    Some("The display settings were not confirmed, so they were reverted.".into());
            }
            OutputEvent::Unavailable(reason) => {
                self.unavailable = Some(reason);
                self.loaded = true;
            }
        }
    }

    fn finish(&mut self, outcome: Outcome, purpose: Purpose) {
        let succeeded = outcome == Outcome::Succeeded;
        self.error = match outcome {
            Outcome::Succeeded => None,
            Outcome::Failed(reason) => Some(reason),
            Outcome::Cancelled => Some("The monitors changed; try again.".into()),
        };
        match purpose {
            Purpose::Test => {}
            Purpose::Apply if succeeded => {
                self.confirmation = Some(Confirmation::Countdown {
                    seconds_left: REVERT_AFTER.as_secs(),
                });
            }
            Purpose::Apply | Purpose::Revert => self.confirmation = None,
        }
    }

    /// Throws away unapplied edits, putting the drafts back to what is real.
    pub fn reset_drafts(&mut self) {
        self.drafts = self.monitors.iter().map(Monitor::to_draft).collect();
    }

    /// The draft `monitor` should have after a fresh snapshot: the user's
    /// edit if they made one, and otherwise the monitor as it now is.
    fn rebased(&self, monitor: &Monitor) -> MonitorDraft {
        let edited = self.draft(&monitor.name).filter(|draft| {
            self.monitor(&monitor.name)
                .is_some_and(|was| **draft != was.to_draft())
        });

        edited.cloned().unwrap_or_else(|| monitor.to_draft())
    }

    /// Whether two enabled drafts would cover the same pixel, which is an
    /// arrangement no compositor should accept.
    fn overlaps(&self) -> bool {
        let boxes: Vec<Rect> = self
            .drafts
            .iter()
            .filter(|draft| draft.enabled)
            .filter_map(|draft| Some(draft.geometry(self.monitor(&draft.name)?)))
            .collect();

        boxes.iter().enumerate().any(|(index, left)| {
            boxes[index + 1..]
                .iter()
                .any(|right| intersects(*left, *right))
        })
    }

    /// Lays the enabled monitors out in one row, left to right, keeping the
    /// order they are already in. A remote screen stays where it is.
    fn repack(&mut self) {
        let mut order: Vec<usize> = (0..self.drafts.len())
            .filter(|index| {
                let draft = &self.drafts[*index];
                draft.enabled
                    && self
                        .monitor(&draft.name)
                        .is_some_and(|monitor| !monitor.is_virtual())
            })
            .collect();
        order.sort_by_key(|index| self.drafts[*index].position.0);

        let mut x = 0;
        for index in order {
            let draft = &self.drafts[index];
            let width = self
                .monitor(&draft.name)
                .map_or(0, |monitor| draft.geometry(monitor).1.0);

            self.drafts[index].position = (x, 0);
            x += width;
        }
    }

    /// Where each enabled monitor other than `except` is, by its draft.
    pub fn enabled_rects(&self, except: &str) -> Vec<Rect> {
        self.drafts
            .iter()
            .filter(|draft| draft.enabled && draft.name != except)
            .filter_map(|draft| Some(draft.geometry(self.monitor(&draft.name)?)))
            .collect()
    }

    /// Moves monitors to positions the user chose, and asks whether the
    /// result would work. Never repacks.
    pub fn place<'a>(&mut self, positions: impl IntoIterator<Item = (&'a str, (i32, i32))>) {
        for (name, position) in positions {
            if let Some(draft) = self.drafts.iter_mut().find(|draft| draft.name == name) {
                draft.position = position;
            }
        }
        self.test();
    }

    /// The primary monitor: the one `configured` names, else the one the
    /// compositor reports, else the top-left enabled monitor with a display.
    pub fn primary(&self, configured: Option<&str>) -> Option<&Monitor> {
        let configured = configured.and_then(|name| {
            self.monitors
                .iter()
                .find(|monitor| monitor.enabled && monitor.is_named(name))
        });
        let reported = || {
            self.monitors
                .iter()
                .find(|monitor| monitor.described().is_some_and(|info| info.primary))
        };
        let top_left = || {
            self.monitors
                .iter()
                .filter(|monitor| monitor.enabled && !monitor.is_virtual())
                .min_by_key(|monitor| (monitor.position.1, monitor.position.0))
        };
        configured.or_else(reported).or_else(top_left)
    }

    pub fn set_brightness(&mut self, name: &str, level: f32) {
        if let Some(brightness) = self
            .monitors
            .iter_mut()
            .find(|monitor| monitor.name == name)
            .and_then(|monitor| monitor.brightness.as_mut())
        {
            brightness.level = level;
        }
        self.send(OutputCommand::SetBrightness {
            name: name.to_owned(),
            level,
        });
    }

    pub fn monitor(&self, name: &str) -> Option<&Monitor> {
        self.monitors.iter().find(|monitor| monitor.name == name)
    }

    pub fn draft(&self, name: &str) -> Option<&MonitorDraft> {
        self.drafts.iter().find(|draft| draft.name == name)
    }

    /// Edits one monitor's draft in place, and asks whether the result would
    /// work.
    ///
    /// The test is what turns "Apply did nothing and you don't know why" into
    /// a message as the change is made — it costs the compositor nothing,
    /// since testing is pure validation with no hardware involved.
    pub fn edit(&mut self, name: &str, edit: impl FnOnce(&mut MonitorDraft)) {
        let Some(draft) = self.drafts.iter_mut().find(|draft| draft.name == name) else {
            return;
        };
        let was_enabled = draft.enabled;
        edit(draft);
        if !was_enabled && draft.enabled {
            let position = beside(&self.enabled_rects(name));
            if let Some(draft) = self.drafts.iter_mut().find(|draft| draft.name == name) {
                draft.position = position;
            }
        }

        // A coarser scale, a bigger mode or a rotation can leave two of them
        // on the same pixels. Repacking only once that has actually happened
        // leaves an arrangement made elsewhere — stacked monitors, a
        // deliberate gap — alone for as long as it still works.
        if self.overlaps() {
            self.repack();
        }
        self.test();
    }

    /// Whether anything has been changed but not applied.
    pub fn is_dirty(&self) -> bool {
        self.monitors
            .iter()
            .any(|monitor| self.draft(&monitor.name) != Some(&monitor.to_draft()))
    }

    /// Checks the drafts without applying them.
    pub fn test(&mut self) {
        let drafts = self.drafts.clone();
        self.send(OutputCommand::Test(drafts));
    }

    /// Applies the drafts, telling the backend what to go back to.
    pub fn apply(&mut self) {
        let previous = self.monitors.iter().map(Monitor::to_draft).collect();
        self.confirmation = Some(Confirmation::Awaiting);
        let drafts = self.drafts.clone();
        self.send(OutputCommand::Apply { drafts, previous });
    }

    /// Keeps the arrangement that was applied.
    pub fn confirm(&mut self) {
        self.confirmation = None;
        self.send(OutputCommand::Confirm);
    }

    /// Puts back what was there before the last apply. The snapshot that
    /// follows brings the drafts back with it.
    pub fn revert(&mut self) {
        if self.confirmation.take().is_some() {
            self.send(OutputCommand::Revert);
        }
    }
}

/// Starts the monitor backend for as long as the calling component lives.
///
/// Nothing is mirrored into a RON file: the compositor persists the
/// arrangement itself, and a copy here would be a second source of truth.
pub fn output_worker(cx: &mut Cx, outputs: Signal<OutputState>) {
    let sender = spawn_worker(cx, worker::run, move |cx, event| {
        outputs.update(cx, |state| state.apply_event(event));
    });
    outputs.update(cx, |state| state.attach(sender));
}

#[cfg(test)]
mod tests {
    use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};

    use super::*;

    fn monitor(name: &str, position: (i32, i32)) -> Monitor {
        Monitor {
            name: name.into(),
            description: format!("{name} description"),
            make: Some("Dell".into()),
            model: Some("U2720Q".into()),
            serial: None,
            enabled: true,
            modes: vec![Mode {
                width: 3840,
                height: 2160,
                refresh: 60_000,
                preferred: true,
            }],
            current_mode: Some(0),
            position,
            scale: 2.0,
            transform: Transform::Normal,
            vrr_capable: true,
            info: InfoState::Unserved,
            brightness: None,
        }
    }

    fn loaded(monitors: Vec<Monitor>) -> OutputState {
        let mut state = OutputState::default();
        state.apply_event(OutputEvent::Snapshot(monitors));
        state
    }

    #[test]
    fn a_snapshot_seeds_a_draft_for_every_monitor() {
        let state = loaded(vec![monitor("DP-1", (0, 0)), monitor("DP-2", (1920, 0))]);

        assert_eq!(state.drafts.len(), 2);
        assert!(!state.is_dirty(), "an untouched snapshot is not dirty");
        assert_eq!(state.selected.as_deref(), Some("DP-1"));
    }

    #[test]
    fn editing_a_draft_makes_the_page_dirty() {
        let mut state = loaded(vec![monitor("DP-1", (0, 0))]);
        state.edit("DP-1", |draft| draft.scale = 1.5);

        assert!(state.is_dirty());
        state.reset_drafts();
        assert!(!state.is_dirty());
    }

    #[test]
    fn a_hotplug_keeps_edits_to_monitors_that_are_still_there() {
        let mut state = loaded(vec![monitor("DP-1", (0, 0)), monitor("DP-2", (1920, 0))]);
        state.edit("DP-1", |draft| draft.scale = 1.25);

        // DP-2 was unplugged; DP-3 appeared.
        state.apply_event(OutputEvent::Snapshot(vec![
            monitor("DP-1", (0, 0)),
            monitor("DP-3", (1920, 0)),
        ]));

        assert_eq!(
            state.draft("DP-1").map(|draft| draft.scale),
            Some(1.25),
            "an edit in progress must survive an unrelated hotplug"
        );
        assert!(state.draft("DP-2").is_none(), "a gone monitor leaves");
        assert!(state.draft("DP-3").is_some(), "a new one arrives");
    }

    #[test]
    fn selection_moves_when_the_selected_monitor_is_unplugged() {
        let mut state = loaded(vec![monitor("DP-1", (0, 0)), monitor("DP-2", (1920, 0))]);
        state.selected = Some("DP-2".into());

        state.apply_event(OutputEvent::Snapshot(vec![monitor("DP-1", (0, 0))]));
        assert_eq!(state.selected.as_deref(), Some("DP-1"));
    }

    fn attached(state: &mut OutputState) -> UnboundedReceiver<OutputCommand> {
        let (sender, receiver) = unbounded_channel();
        state.attach(sender);
        receiver
    }

    fn sent(commands: &mut UnboundedReceiver<OutputCommand>) -> Vec<OutputCommand> {
        std::iter::from_fn(|| commands.try_recv().ok()).collect()
    }

    fn applied(outcome: Outcome) -> OutputEvent {
        OutputEvent::Result {
            outcome,
            purpose: Purpose::Apply,
        }
    }

    #[test]
    fn an_applied_arrangement_asks_to_be_kept() {
        let mut state = loaded(vec![monitor("DP-1", (0, 0))]);
        state.edit("DP-1", |draft| draft.scale = 1.0);
        state.apply();
        assert_eq!(state.confirmation, Some(Confirmation::Awaiting));

        state.apply_event(applied(Outcome::Succeeded));
        assert_eq!(
            state.confirmation,
            Some(Confirmation::Countdown {
                seconds_left: REVERT_AFTER.as_secs()
            })
        );
        assert_eq!(state.error, None);
    }

    #[test]
    fn a_refused_apply_shows_the_refusal_keeps_the_drafts_and_reverts_nothing() {
        for refusal in [
            Outcome::Failed("head winit-1 has no such mode".into()),
            Outcome::Cancelled,
        ] {
            let mut state = loaded(vec![monitor("DP-1", (0, 0))]);
            let mut commands = attached(&mut state);
            state.edit("DP-1", |draft| draft.scale = 1.0);
            state.apply();
            state.apply_event(applied(refusal));

            assert!(state.error.is_some(), "the refusal is shown");
            assert_ne!(
                state.error.as_deref(),
                Some("The display settings were not confirmed, so they were reverted.")
            );
            assert!(state.is_dirty(), "the drafts stay for the user to fix");
            assert_eq!(state.confirmation, None, "there is nothing to keep");

            state.revert();
            assert!(
                !sent(&mut commands).contains(&OutputCommand::Revert),
                "nothing was applied, so nothing goes back"
            );
        }
    }

    #[test]
    fn a_refused_test_reports_the_problem_without_undoing_the_edit() {
        let mut state = loaded(vec![monitor("DP-1", (0, 0))]);
        state.edit("DP-1", |draft| draft.scale = 1.0);
        state.apply_event(OutputEvent::Result {
            outcome: Outcome::Failed("they would overlap".into()),
            purpose: Purpose::Test,
        });

        assert_eq!(state.error.as_deref(), Some("they would overlap"));
        assert!(
            state.is_dirty(),
            "the user is mid-edit; their work must survive a failed check"
        );
    }

    #[test]
    fn a_successful_revert_does_not_ask_to_be_kept() {
        let mut state = loaded(vec![monitor("DP-1", (0, 0))]);
        state.apply();
        state.apply_event(applied(Outcome::Succeeded));
        state.revert();
        state.apply_event(OutputEvent::Result {
            outcome: Outcome::Succeeded,
            purpose: Purpose::Revert,
        });
        assert_eq!(state.confirmation, None);
    }

    #[test]
    fn an_expired_countdown_puts_everything_back_and_says_so() {
        let mut state = loaded(vec![monitor("DP-1", (0, 0))]);
        state.edit("DP-1", |draft| draft.scale = 1.0);
        state.apply();
        state.apply_event(applied(Outcome::Succeeded));

        state.apply_event(OutputEvent::Reverted);
        assert!(!state.is_dirty(), "the drafts go back to what is real");
        assert_eq!(state.confirmation, None);
        assert!(
            state.error.is_some(),
            "the user is told why it changed back"
        );
    }

    #[test]
    fn reverting_asks_the_backend_to_put_the_old_arrangement_back() {
        let mut state = loaded(vec![monitor("DP-1", (0, 0))]);
        let mut commands = attached(&mut state);
        state.edit("DP-1", |draft| draft.scale = 1.0);
        state.apply();
        state.apply_event(applied(Outcome::Succeeded));

        state.revert();
        assert_eq!(sent(&mut commands).last(), Some(&OutputCommand::Revert));
        assert_eq!(state.confirmation, None);
    }

    #[test]
    fn confirming_drops_the_rollback() {
        let mut state = loaded(vec![monitor("DP-1", (0, 0))]);
        state.apply();
        state.apply_event(applied(Outcome::Succeeded));
        state.confirm();
        assert_eq!(state.confirmation, None);
    }

    #[test]
    fn a_draft_covers_what_its_scale_and_rotation_make_of_its_mode() {
        let monitor = monitor("DP-1", (0, 0));
        let mut draft = monitor.to_draft();
        assert_eq!(draft.geometry(&monitor).1, (1920, 1080));

        draft.transform = Transform::Rotate90;
        assert_eq!(draft.geometry(&monitor).1, (1080, 1920));

        draft.scale = 1.0;
        assert_eq!(draft.geometry(&monitor).1, (2160, 3840));
    }

    #[test]
    fn a_draft_with_no_mode_covers_nothing() {
        let monitor = monitor("DP-1", (0, 0));
        let mut draft = monitor.to_draft();
        draft.mode = None;

        assert_eq!(draft.geometry(&monitor).1, (0, 0));
        assert!(!loaded(vec![monitor]).overlaps());
    }

    #[test]
    fn a_monitor_is_titled_by_its_make_and_model_over_its_connector() {
        assert_eq!(monitor("DP-1", (0, 0)).title(), "Dell U2720Q (DP-1)");

        let mut nameless = monitor("DP-1", (0, 0));
        nameless.make = None;
        nameless.model = None;
        assert_eq!(
            nameless.title(),
            "DP-1",
            "with nothing else to say, the connector is not worth saying twice"
        );
    }

    #[test]
    fn two_identical_panels_are_told_apart() {
        let left = monitor("DP-1", (0, 0));
        let right = monitor("DP-2", (1920, 0));
        assert_ne!(left.title(), right.title());
    }

    #[test]
    fn a_snapshot_rebases_a_draft_nobody_edited() {
        let mut state = loaded(vec![monitor("DP-1", (0, 0))]);

        // Something else moved it: `wlr-randr`, a config reload, this app's
        // own apply landing.
        let mut moved = monitor("DP-1", (0, 0));
        moved.scale = 1.0;
        state.apply_event(OutputEvent::Snapshot(vec![moved]));

        assert_eq!(state.draft("DP-1").map(|draft| draft.scale), Some(1.0));
        assert!(
            !state.is_dirty(),
            "a draft that said nothing must not strand the page on unapplied changes"
        );
    }

    #[test]
    fn a_snapshot_leaves_an_edit_in_progress_alone() {
        let mut state = loaded(vec![monitor("DP-1", (0, 0)), monitor("DP-2", (1920, 0))]);
        state.edit("DP-1", |draft| draft.scale = 1.25);

        state.apply_event(OutputEvent::Snapshot(vec![
            monitor("DP-1", (0, 0)),
            monitor("DP-2", (1920, 0)),
        ]));

        assert_eq!(state.draft("DP-1").map(|draft| draft.scale), Some(1.25));
        assert!(state.is_dirty());
    }

    #[test]
    fn growing_a_monitor_repacks_the_row_instead_of_overlapping() {
        let mut state = loaded(vec![monitor("DP-1", (0, 0)), monitor("DP-2", (1920, 0))]);

        // Halving the scale doubles the logical width to 3840, which would
        // swallow DP-2 where it stands.
        state.edit("DP-1", |draft| draft.scale = 1.0);

        assert_eq!(
            state.draft("DP-1").map(|draft| draft.position),
            Some((0, 0))
        );
        assert_eq!(
            state.draft("DP-2").map(|draft| draft.position),
            Some((3840, 0)),
            "the monitor to its right moves out of the way"
        );
        assert!(!state.overlaps());
    }

    #[test]
    fn switching_a_monitor_back_on_finds_it_somewhere_to_go() {
        // A disabled head is reported at the origin, on top of everything.
        let mut off = monitor("DP-2", (0, 0));
        off.enabled = false;
        let mut state = loaded(vec![monitor("DP-1", (0, 0)), off]);

        state.edit("DP-2", |draft| draft.enabled = true);

        assert!(
            !state.overlaps(),
            "an overlapping arrangement is always refused"
        );
        assert_eq!(
            state.draft("DP-2").map(|draft| draft.position),
            Some((1920, 0))
        );
    }

    #[test]
    fn a_monitor_switched_back_on_goes_right_of_a_stacked_arrangement() {
        let mut off = monitor("DP-3", (0, 0));
        off.enabled = false;
        let mut state = loaded(vec![
            monitor("DP-1", (0, 0)),
            monitor("DP-2", (0, 1080)),
            off,
        ]);

        state.edit("DP-3", |draft| draft.enabled = true);

        assert_eq!(
            state.draft("DP-2").map(|draft| draft.position),
            Some((0, 1080)),
            "the stack stays a stack"
        );
        assert_eq!(
            state.draft("DP-3").map(|draft| draft.position),
            Some((1920, 0))
        );
    }

    #[test]
    fn placing_a_monitor_never_repacks() {
        let mut state = loaded(vec![monitor("DP-1", (0, 0)), monitor("DP-2", (1920, 0))]);
        state.place([("DP-2", (0, 1080)), ("Gone-1", (5, 5))]);

        assert_eq!(
            state.draft("DP-2").map(|draft| draft.position),
            Some((0, 1080))
        );
        assert!(state.is_dirty());
    }

    #[test]
    fn repacking_leaves_a_remote_screen_where_it_is() {
        let mut state = loaded(vec![
            monitor("DP-1", (0, 0)),
            monitor("DP-2", (1920, 0)),
            monitor("VIRTUAL-1", (0, 2000)),
        ]);
        state.edit("DP-1", |draft| draft.scale = 1.0);

        assert_eq!(
            state.draft("VIRTUAL-1").map(|draft| draft.position),
            Some((0, 2000))
        );
        assert_eq!(
            state.draft("DP-2").map(|draft| draft.position),
            Some((3840, 0))
        );
    }

    #[test]
    fn an_arrangement_that_still_works_is_left_alone() {
        let mut state = loaded(vec![monitor("DP-1", (0, 0)), monitor("DP-2", (4000, 500))]);

        state.edit("DP-1", |draft| draft.transform = Transform::Rotate180);

        assert_eq!(
            state.draft("DP-2").map(|draft| draft.position),
            Some((4000, 500)),
            "a deliberate gap, or a stacked monitor, is nobody else's business"
        );
    }

    #[test]
    fn an_unavailable_compositor_is_reported_rather_than_looking_empty() {
        let mut state = OutputState::default();
        state.apply_event(OutputEvent::Unavailable("no support".into()));

        assert!(
            state.loaded,
            "the page must stop saying it is still looking"
        );
        assert_eq!(state.unavailable.as_deref(), Some("no support"));
    }

    fn described(name: &str, identity: &str, primary: bool) -> Monitor {
        Monitor {
            info: InfoState::Described(monitor::OutputInfo {
                identity: identity.into(),
                primary,
                scanout_bits: 8,
                ..monitor::OutputInfo::default()
            }),
            ..monitor(name, (0, 0))
        }
    }

    #[test]
    fn the_identity_is_the_compositors_or_the_connector_without_the_protocol() {
        assert_eq!(
            described("DP-1", "Dell U2720Q 7XK2", false).identity(),
            Some("Dell U2720Q 7XK2")
        );
        assert_eq!(monitor("DP-1", (0, 0)).identity(), Some("DP-1"));

        let waiting = Monitor {
            info: InfoState::Waiting,
            ..monitor("DP-1", (0, 0))
        };
        assert_eq!(waiting.identity(), None);
    }

    #[test]
    fn a_virtual_head_is_known_by_its_prefix_without_the_protocol() {
        assert!(monitor("VIRTUAL-1", (0, 0)).is_virtual());
        assert!(!monitor("DP-1", (0, 0)).is_virtual());
        assert!(!described("VIRTUAL-1", "VIRTUAL-1", false).is_virtual());
    }

    #[test]
    fn the_configured_primary_wins_then_the_reported_then_the_top_left() {
        let state = loaded(vec![
            described("DP-1", "Left 1", false),
            described("DP-2", "Right 2", true),
        ]);
        let primary = |configured| {
            state
                .primary(configured)
                .map(|monitor| monitor.name.as_str())
        };

        assert_eq!(primary(Some("Left 1")), Some("DP-1"));
        assert_eq!(primary(None), Some("DP-2"));
        assert_eq!(primary(Some("Gone 3")), Some("DP-2"));

        let unserved = loaded(vec![monitor("DP-1", (1920, 0)), monitor("DP-2", (0, 0))]);
        assert_eq!(
            unserved.primary(None).map(|monitor| monitor.name.as_str()),
            Some("DP-2")
        );
    }

    #[test]
    fn a_brightness_change_shows_at_once() {
        let mut state = loaded(vec![Monitor {
            brightness: Some(monitor::Brightness {
                control: BrightnessControl::Software,
                level: 1.0,
            }),
            ..monitor("DP-1", (0, 0))
        }]);
        state.set_brightness("DP-1", 0.4);
        assert_eq!(
            state.monitor("DP-1").and_then(|monitor| monitor.brightness),
            Some(monitor::Brightness {
                control: BrightnessControl::Software,
                level: 0.4,
            })
        );
    }
}
