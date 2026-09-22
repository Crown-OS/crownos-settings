//! The monitors, as the compositor has them.
//!
//! The house shape (see [`crate::net`]): a command enum, an event enum, and a
//! [`OutputState`] in the [`Store`](crate::state::Store). Like the other two
//! backends it is one long-lived future on xilem's tokio runtime — see
//! [`protocol`] for why the Wayland socket is polled rather than blocked on.
//!
//! ## Why a second Wayland connection
//!
//! winit already holds one for the window. This is a separate conversation
//! about every monitor in the session rather than anything to do with this
//! app's surface, and `zwlr_output_management_v1` is not something winit
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
//! ## Monitors are packed, not placed
//!
//! The page has no arrangement editor, so a draft carries whatever position
//! the compositor reported. That position stops being valid the moment the
//! monitor's *size* changes — a coarser scale, a bigger mode, a rotation —
//! and a compositor is right to refuse an arrangement whose monitors overlap.
//! So [`OutputState::edit`] repacks the drafts left to right whenever the
//! edit has made them collide, which is also what makes switching a monitor
//! back on work: a disabled head is reported at the origin, on top of
//! everything else.

mod protocol;

use std::time::Duration;

use xilem::{
    ViewCtx,
    core::{MessageProxy, NoElement, View},
    tokio::{
        select,
        sync::mpsc::{UnboundedReceiver, UnboundedSender},
        time::{Instant, sleep_until},
    },
    view::worker_raw,
};

/// How long a newly applied arrangement has to be confirmed before it is
/// rolled back.
///
/// Long enough to find the mouse on a monitor that just moved, short enough
/// that a black screen is not a reason to reboot.
pub const REVERT_AFTER: Duration = Duration::from_secs(15);

/// How a monitor is oriented.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Transform {
    #[default]
    Normal,
    Rotate90,
    Rotate180,
    Rotate270,
    Flipped,
    Flipped90,
    Flipped180,
    Flipped270,
}

impl Transform {
    /// The four the page offers. The flipped variants exist in the protocol
    /// but describe mirrored panels, which is not something a user picks.
    pub const CHOICES: [Self; 4] = [
        Self::Normal,
        Self::Rotate90,
        Self::Rotate180,
        Self::Rotate270,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Normal => "Landscape",
            Self::Rotate90 => "Portrait",
            Self::Rotate180 => "Landscape (flipped)",
            Self::Rotate270 => "Portrait (flipped)",
            Self::Flipped => "Mirrored",
            Self::Flipped90 => "Mirrored portrait",
            Self::Flipped180 => "Mirrored landscape",
            Self::Flipped270 => "Mirrored portrait (flipped)",
        }
    }

    /// Whether this orientation swaps width and height.
    pub fn is_upright(self) -> bool {
        matches!(
            self,
            Self::Rotate90 | Self::Rotate270 | Self::Flipped90 | Self::Flipped270
        )
    }
}

/// One mode a monitor can run at.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Mode {
    pub width: i32,
    pub height: i32,
    /// Millihertz, as the protocol counts it.
    pub refresh: i32,
    pub preferred: bool,
}

impl Mode {
    pub fn label(&self) -> String {
        format!(
            "{}×{} @ {:.2} Hz",
            self.width,
            self.height,
            f64::from(self.refresh) / 1000.0
        )
    }
}

/// One monitor, as the page shows it.
#[derive(Debug, Clone, PartialEq)]
pub struct Monitor {
    /// The connector, e.g. `"DP-1"`. Identifies it in a configuration.
    pub name: String,
    pub description: String,
    pub make: Option<String>,
    pub model: Option<String>,
    pub serial: Option<String>,
    pub enabled: bool,
    pub modes: Vec<Mode>,
    pub current_mode: Option<usize>,
    pub position: (i32, i32),
    pub scale: f64,
    pub transform: Transform,
    pub adaptive_sync: bool,
    /// Whether the compositor said anything about adaptive sync at all, which
    /// is what decides if the toggle is offered.
    pub vrr_capable: bool,
}

impl Monitor {
    /// What to call this monitor in the interface.
    ///
    /// The make and model where the panel provided them, because "Dell U2720Q"
    /// means something to a user and "DP-1" does not. The connector is kept
    /// alongside, since it is what distinguishes two identical monitors.
    pub fn title(&self) -> String {
        match (&self.make, &self.model) {
            (Some(make), Some(model)) => format!("{make} {model} ({})", self.name),
            (Some(name), None) | (None, Some(name)) => format!("{name} ({})", self.name),
            (None, None) => self.name.clone(),
        }
    }

    /// This monitor as a draft, i.e. "leave it exactly as it is".
    pub fn to_draft(&self) -> MonitorDraft {
        MonitorDraft {
            name: self.name.clone(),
            enabled: self.enabled,
            mode: self.current_mode,
            position: self.position,
            scale: self.scale,
            transform: self.transform,
            adaptive_sync: self.adaptive_sync,
        }
    }
}

/// A logical rectangle: where a monitor starts, and how big it is.
type Rect = ((i32, i32), (i32, i32));

/// What a mode occupies once scale and orientation are applied.
fn logical_size(mode: Option<&Mode>, scale: f64, transform: Transform) -> (i32, i32) {
    let Some(mode) = mode else {
        return (0, 0);
    };
    let scale = if scale > 0.0 { scale } else { 1.0 };
    let (width, height) = (
        (f64::from(mode.width) / scale).round() as i32,
        (f64::from(mode.height) / scale).round() as i32,
    );

    if transform.is_upright() {
        (height, width)
    } else {
        (width, height)
    }
}

fn intersects(left: Rect, right: Rect) -> bool {
    let (((lx, ly), (lw, lh)), ((rx, ry), (rw, rh))) = (left, right);
    lx < rx + rw && rx < lx + lw && ly < ry + rh && ry < ly + lh
}

/// What the page wants one monitor to become.
#[derive(Debug, Clone, PartialEq)]
pub struct MonitorDraft {
    pub name: String,
    pub enabled: bool,
    pub mode: Option<usize>,
    pub position: (i32, i32),
    pub scale: f64,
    pub transform: Transform,
    pub adaptive_sync: bool,
}

impl MonitorDraft {
    /// Where this draft would put the monitor, and how much of the desktop it
    /// would cover. The mode list is the monitor's, so it has to be given.
    fn geometry(&self, monitor: &Monitor) -> Rect {
        let mode = self.mode.and_then(|index| monitor.modes.get(index));
        (self.position, logical_size(mode, self.scale, self.transform))
    }
}

/// How a configuration ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Succeeded,
    Failed(String),
    /// The monitors changed while the configuration was in flight.
    Cancelled,
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
}

/// Everything the backend can tell the page.
#[derive(Debug, Clone, PartialEq)]
pub enum OutputEvent {
    /// The monitors, as they are now.
    Snapshot(Vec<Monitor>),
    /// A configuration finished. `test` says which kind it was, because the
    /// two failures mean different things: a refused *apply* leaves the
    /// drafts stale and they go back, while a refused *test* is the user
    /// halfway through an edit and must not have their work undone.
    Result { outcome: Outcome, test: bool },
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
    /// The arrangement to put back if the current one is not confirmed.
    pub pending_revert: Option<Vec<MonitorDraft>>,
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
                self.drafts = monitors.iter().map(|monitor| self.rebased(monitor)).collect();

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
            OutputEvent::Result {
                outcome: Outcome::Succeeded,
                ..
            } => self.error = None,
            OutputEvent::Result {
                outcome: Outcome::Failed(reason),
                test: true,
            } => self.error = Some(reason),
            OutputEvent::Reverted => {
                self.pending_revert = None;
                self.reset_drafts();
                self.error =
                    Some("The display settings were not confirmed, so they were reverted.".into());
            }
            OutputEvent::Result {
                outcome: Outcome::Failed(reason),
                test: false,
            } => {
                self.error = Some(reason);
                self.pending_revert = None;
                self.reset_drafts();
            }
            OutputEvent::Result {
                outcome: Outcome::Cancelled,
                ..
            } => {
                self.error = Some("The monitors changed; try again.".into());
                self.pending_revert = None;
                self.reset_drafts();
            }
            OutputEvent::Unavailable(reason) => {
                self.unavailable = Some(reason);
                self.loaded = true;
            }
        }
    }

    /// Throws away unapplied edits, putting the drafts back to what is real.
    pub fn reset_drafts(&mut self) {
        self.drafts = self.monitors.iter().map(Monitor::to_draft).collect();
    }

    /// The draft `monitor` should have after a fresh snapshot: the user's
    /// edit if they made one, and otherwise the monitor as it now is.
    fn rebased(&self, monitor: &Monitor) -> MonitorDraft {
        let edited = self
            .draft(&monitor.name)
            .filter(|draft| self.monitor(&monitor.name).is_some_and(|was| **draft != was.to_draft()));

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

        boxes
            .iter()
            .enumerate()
            .any(|(index, left)| boxes[index + 1..].iter().any(|right| intersects(*left, *right)))
    }

    /// Lays the enabled monitors out in one row, left to right, keeping the
    /// order they are already in.
    ///
    /// The page offers no way to place a monitor, so this is the only
    /// arrangement it can express — and it is the one the compositor would
    /// have picked for an output it had to place itself.
    fn repack(&mut self) {
        let mut order: Vec<usize> = (0..self.drafts.len())
            .filter(|index| self.drafts[*index].enabled)
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
        edit(draft);

        // A coarser scale, a bigger mode, a rotation or a monitor coming back
        // on can all leave two of them on the same pixels. Repacking only
        // once that has actually happened leaves an arrangement made
        // elsewhere — stacked monitors, a deliberate gap — alone for as long
        // as it still works.
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

    /// Applies the drafts, remembering what to go back to.
    pub fn apply(&mut self) {
        let previous: Vec<MonitorDraft> = self.monitors.iter().map(Monitor::to_draft).collect();
        self.pending_revert = Some(previous.clone());
        let drafts = self.drafts.clone();
        self.send(OutputCommand::Apply { drafts, previous });
    }

    /// Keeps the arrangement that was applied.
    pub fn confirm(&mut self) {
        self.pending_revert = None;
        self.send(OutputCommand::Confirm);
    }

    /// Puts back what was there before the last apply.
    pub fn revert(&mut self) {
        let Some(previous) = self.pending_revert.take() else {
            return;
        };
        self.drafts = previous;
        self.send(OutputCommand::Revert);
    }
}

/// The backend, as a view that draws nothing.
pub fn output_worker<State, F>(
    outputs: fn(&mut State) -> &mut OutputState,
    on_event: F,
) -> impl View<State, (), ViewCtx, Element = NoElement> + Send + Sync
where
    State: 'static,
    F: Fn(&mut State, &OutputEvent) + Send + Sync + 'static,
{
    worker_raw(
        run,
        move |state: &mut State, sender| outputs(state).attach(sender),
        move |state: &mut State, event: OutputEvent| {
            on_event(state, &event);
            outputs(state).apply_event(event);
        },
    )
}

/// The page's last word when the conversation ends, whatever ended it.
fn gone(err: &impl std::fmt::Display) -> String {
    format!("the compositor stopped talking to us: {err}")
}

/// The one loop: the compositor's events, the page's commands, and the
/// countdown that undoes an arrangement nobody confirmed.
///
/// The countdown lives here rather than in the page for the reason it exists
/// at all — a bad arrangement can leave the user unable to click anything, so
/// the rollback must not depend on the interface still working.
///
/// Commands are taken first, because a `select!` that preferred the socket
/// would leave an apply waiting behind an unrelated hotplug.
async fn run(proxy: MessageProxy<OutputEvent>, mut commands: UnboundedReceiver<OutputCommand>) {
    let report = move |event| {
        // The window has gone; the loop ends on its next turn either way.
        let _ = proxy.message(event);
    };

    let mut session = match protocol::Session::connect() {
        Ok(session) => session,
        Err(reason) => {
            report(OutputEvent::Unavailable(reason));
            return;
        }
    };

    /// What woke the loop up.
    enum Woken {
        Command(OutputCommand),
        /// Nobody confirmed the last arrangement.
        Expired,
        /// The compositor said something, which `dispatch` has dealt with.
        Compositor,
    }

    // What to put back, and when to give up waiting for a confirmation.
    let mut rollback: Option<(Vec<MonitorDraft>, Instant)> = None;

    loop {
        let deadline = rollback.as_ref().map(|(_, at)| *at);

        let woken = select! {
            biased;
            command = commands.recv() => match command {
                Some(command) => Woken::Command(command),
                None => return,
            },
            () = sleep_until(deadline.unwrap_or_else(Instant::now)), if deadline.is_some() => {
                Woken::Expired
            }
            result = session.dispatch(&report) => {
                if let Err(err) = result {
                    report(OutputEvent::Unavailable(gone(&err)));
                    return;
                }
                Woken::Compositor
            }
        };

        let expired = matches!(woken, Woken::Expired);
        let configuration = match woken {
            Woken::Compositor => None,
            Woken::Expired => rollback.take().map(|(previous, _)| (previous, false)),
            Woken::Command(OutputCommand::Test(drafts)) => Some((drafts, true)),
            Woken::Command(OutputCommand::Apply { drafts, previous }) => {
                rollback = Some((previous, Instant::now() + REVERT_AFTER));
                Some((drafts, false))
            }
            Woken::Command(OutputCommand::Confirm) => {
                rollback = None;
                None
            }
            Woken::Command(OutputCommand::Revert) => {
                rollback.take().map(|(previous, _)| (previous, false))
            }
        };

        let Some((drafts, test_only)) = configuration else {
            continue;
        };
        if let Err(err) = session.configure(&drafts, test_only, &report).await {
            report(OutputEvent::Unavailable(gone(&err)));
            return;
        }
        if expired {
            report(OutputEvent::Reverted);
        }
    }
}

#[cfg(test)]
mod tests {
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
            adaptive_sync: false,
            vrr_capable: true,
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

    #[test]
    fn a_failed_configuration_puts_the_drafts_back() {
        let mut state = loaded(vec![monitor("DP-1", (0, 0))]);
        state.edit("DP-1", |draft| draft.scale = 4.0);
        state.apply_event(OutputEvent::Result {
            outcome: Outcome::Failed("nope".into()),
            test: false,
        });

        assert!(!state.is_dirty(), "a refused change must not stay pending");
        assert_eq!(state.error.as_deref(), Some("nope"));
        assert!(state.pending_revert.is_none());
    }

    #[test]
    fn a_refused_test_reports_the_problem_without_undoing_the_edit() {
        let mut state = loaded(vec![monitor("DP-1", (0, 0))]);
        state.edit("DP-1", |draft| draft.scale = 1.0);
        state.apply_event(OutputEvent::Result {
            outcome: Outcome::Failed("they would overlap".into()),
            test: true,
        });

        assert_eq!(state.error.as_deref(), Some("they would overlap"));
        assert!(
            state.is_dirty(),
            "the user is mid-edit; their work must survive a failed check"
        );
    }

    #[test]
    fn an_expired_countdown_puts_everything_back_and_says_so() {
        let mut state = loaded(vec![monitor("DP-1", (0, 0))]);
        state.edit("DP-1", |draft| draft.scale = 1.0);
        state.apply();

        state.apply_event(OutputEvent::Reverted);
        assert!(!state.is_dirty(), "the drafts go back to what is real");
        assert!(state.pending_revert.is_none());
        assert!(state.error.is_some(), "the user is told why it changed back");
    }

    #[test]
    fn reverting_restores_what_was_there_before() {
        let mut state = loaded(vec![monitor("DP-1", (0, 0))]);
        state.edit("DP-1", |draft| draft.scale = 1.0);
        state.apply();
        assert!(state.pending_revert.is_some());

        state.revert();
        assert_eq!(state.draft("DP-1").map(|draft| draft.scale), Some(2.0));
        assert!(state.pending_revert.is_none());
    }

    #[test]
    fn confirming_drops_the_rollback() {
        let mut state = loaded(vec![monitor("DP-1", (0, 0))]);
        state.apply();
        state.confirm();
        assert!(state.pending_revert.is_none());
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

        assert_eq!(state.draft("DP-1").map(|draft| draft.position), Some((0, 0)));
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

        assert!(!state.overlaps(), "an overlapping arrangement is always refused");
        assert_eq!(state.draft("DP-2").map(|draft| draft.position), Some((1920, 0)));
    }

    #[test]
    fn an_arrangement_that_still_works_is_left_alone() {
        let mut state = loaded(vec![monitor("DP-1", (0, 0)), monitor("DP-2", (4000, 500))]);

        state.edit("DP-1", |draft| draft.adaptive_sync = true);

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

        assert!(state.loaded, "the page must stop saying it is still looking");
        assert_eq!(state.unavailable.as_deref(), Some("no support"));
    }
}

