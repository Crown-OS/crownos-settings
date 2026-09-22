//! The Wi-Fi backend: NetworkManager on one side, a plain struct on the other.
//!
//! [`nmrs`] is an async D-Bus client. A xilem page is a synchronous function
//! that turns `&State` into a view tree and must never block. This module is the
//! seam between those two worlds, and it is built so that *all* of the async,
//! fallible, D-Bus-shaped half lives below [`wifi_worker`] and none of it
//! escapes upwards.
//!
//! Four types make up the whole contract:
//!
//! * [`WifiCommand`] — everything the page can ask for. Fire-and-forget: a
//!   command is pushed onto a channel and the page keeps rendering.
//! * [`WifiEvent`] — everything the worker can say back. Delivered on the main
//!   thread through xilem's message path, so handling one is an ordinary
//!   `&mut State` edit followed by a free rebuild.
//! * [`WifiSnapshot`] — the whole visible world in one value: radio, current
//!   connection, known networks, other networks. Already sorted, already
//!   de-duplicated, already carrying the booleans a row needs. Derived once per
//!   refresh so that a page never computes anything but layout.
//! * [`WifiState`] — the snapshot plus the transient bits a page needs to keep
//!   between rebuilds (which SSID is being joined, the last error, an open
//!   password prompt) plus the sending half of the command channel.
//!
//! No `nmrs` type appears in any of them. That is not tidiness for its own
//! sake: it is what lets [`crate::pages::wifi`] be built — and tested — on a
//! machine with no NetworkManager running at all, from
//! `WifiState::default()`.
//!
//! ## Why a snapshot and not a stream of deltas
//!
//! `nmrs`'s event stream is explicitly lossy: every variant means "something
//! changed, go and look again". So the worker never tries to patch its model
//! incrementally. It calls [`NetworkManager::snapshot`] — one batched read —
//! derives a fresh [`WifiSnapshot`], and sends the whole thing. That is a few
//! hundred bytes at human-scale frequencies, and in exchange the UI can never
//! drift out of step with the system.
//!
//! Because `snapshot()` is the expensive call, refreshes are debounced: a burst
//! of signal-strength changes (which NetworkManager emits constantly) collapses
//! into one re-read. The debounce is a *leading* window — the first signal in a
//! burst sets the deadline and later ones don't push it back — so a busy radio
//! delays a refresh by at most [`REFRESH_DEBOUNCE`], rather than starving it
//! forever.
//!
//! ## Nothing in the loop is allowed to take a while
//!
//! The select loop is the only thing that reads NetworkManager's signals and
//! the only thing that drains the command channel, so anything it awaits
//! inline is time the whole subsystem is deaf. `nmrs`'s connect timeout alone
//! is thirty seconds. So commands are *planned* on the loop — which is where
//! the last snapshot lives — and *run* on a task of their own, and the loop
//! learns they finished the same way it learns anything else: as one more arm
//! of the same `select!`. See [`plan`] and [`run_command`].
//!
//! ## Failure
//!
//! Nothing here unwraps. Every `nmrs` error becomes a
//! [`WifiEvent::Error`] string that the page shows and the user can dismiss. If
//! NetworkManager cannot be reached at all the worker emits
//! [`WifiEvent::Unavailable`] and then *keeps draining commands*, answering each
//! one the same way, so the UI stays responsive and consistent instead of
//! silently swallowing clicks.
//!
//! The one failure that must never be answered with a shrug is a failure of the
//! loop's own plumbing: a change stream that could not be opened has to be
//! retried on a timer the loop is already waiting on, because a loop with no
//! stream and no deadline has nothing left that can wake it. That is why
//! [`run`] carries two deadlines rather than one.

use std::cmp::Ordering;
use std::collections::HashMap;
use std::time::Duration;

use futures_util::StreamExt;
use nmrs::models::{
    ActiveConnection, ActiveConnectionState, NetworkEvent, NetworkEventStream, NetworkSnapshot,
    SavedConnection, SecurityFeatures, SettingsPatch, WifiNetworkGroup, WifiSecurity,
};
use nmrs::{ConnectionError, NetworkManager};
use xilem::ViewCtx;
use xilem::core::{MessageProxy, NoElement, View};
use xilem::tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use xilem::tokio::task::JoinSet;
use xilem::tokio::time::{Instant, sleep_until};
use xilem::view::worker_raw;

/// How long a burst of NetworkManager signals is allowed to run before the
/// worker re-reads the world.
///
/// Signal strength alone changes several times a second on a busy band, and
/// [`NetworkManager::snapshot`] is a batched D-Bus round trip — worth
/// collapsing. Long enough to swallow a burst, short enough that clicking
/// "Wi-Fi off" still feels immediate.
const REFRESH_DEBOUNCE: Duration = Duration::from_millis(350);

/// How long to wait before re-subscribing to a change stream that went away.
///
/// A stream ends when NetworkManager does, and a service that is mid-restart
/// will hand back another stream that ends immediately. Without this pause that
/// would be a hot loop with a D-Bus round trip in it; with it, the worker
/// simply waits for the daemon to finish coming back.
const RESUBSCRIBE_DELAY: Duration = Duration::from_millis(500);

/// How long a command that lost a race waits before its one retry.
///
/// Long enough for whichever link was mid-negotiation to settle — an ethernet
/// cable being plugged in is the usual culprit — and short enough that the row
/// is still spinning when the second attempt goes out, so the user sees one
/// slow join rather than a failure and a click.
const RETRY_DELAY: Duration = Duration::from_secs(2);

/// What to say when NetworkManager refuses because it is already busy.
///
/// Phrased as a moment rather than a fault: [`ConnectionError::ConnectionInProgress`]
/// is contention, not a rejection, and the only useful instruction is to wait.
const BUSY_MESSAGE: &str = "Busy — another connection is being set up. Try again in a moment.";

/// The valid length of a WPA pre-shared key, in bytes.
///
/// NetworkManager rejects anything outside this range with an error that reads
/// like an addressing problem (`InvalidAddress`), so the page validates before
/// sending and this is the range it validates against.
///
/// Bytes and not characters, because bytes are what `nmrs` counts: a passphrase
/// with an accent in it is longer than it looks, and a page validating in
/// `char`s would arm its Join button for a key NetworkManager will refuse.
#[allow(dead_code)] // Part of this module's contract; the page reads it.
pub const PSK_LENGTH: std::ops::RangeInclusive<usize> = 8..=63;

// --- MARK: Commands ---

/// Everything the Wi-Fi page can ask the system to do.
///
/// Deliberately imperative and deliberately small. Each variant maps to exactly
/// one `nmrs` call, and none of them return anything: the result of a command is
/// observed the same way an external change is — as the next
/// [`WifiEvent::Snapshot`].
///
/// The variants are the module's published vocabulary rather than a list of
/// call sites, so `dead_code` is silenced here: which of them
/// [`crate::pages::wifi`] happens to reach today is a fact about the page, and
/// removing a verb the backend supports because no button calls it yet would be
/// the wrong repair.
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WifiCommand {
    /// Turn the wireless radio on or off (NM's global software switch).
    SetRadio(bool),
    /// Ask every Wi-Fi device to re-scan. Results arrive as a later snapshot.
    Scan,
    /// Join a network.
    ///
    /// `password` is `None` when the caller has no new secret to offer, which
    /// covers both interesting cases: an open network, and a saved network
    /// whose key NetworkManager already holds. The worker tells them apart from
    /// its own last snapshot, so the page never has to.
    Connect {
        /// The network to join.
        ssid: String,
        /// A freshly typed key, or `None` to use what NetworkManager has.
        password: Option<String>,
    },
    /// Drop the current Wi-Fi connection without forgetting the profile.
    Disconnect,
    /// Flip a saved profile's "connect to this automatically" flag.
    SetAutoJoin {
        /// The saved profile's `connection.uuid`.
        uuid: String,
        /// The new value of `connection.autoconnect`.
        enabled: bool,
    },
    /// Delete a saved profile, disconnecting first if it is the current one.
    ///
    /// Both identifiers are carried because the two cases need different calls:
    /// forgetting the *joined* network has to tear the connection down first,
    /// which `nmrs` only does when asked by SSID.
    Forget {
        /// The network's name, used when it is the one currently joined.
        ssid: String,
        /// The saved profile's `connection.uuid`, used otherwise.
        uuid: String,
    },
}

// --- MARK: Events ---

/// Everything the worker can tell the page.
///
/// This is the `M` of [`worker_raw`], so it must be [`Debug`] — and it is
/// handled on the main thread, so handling it is a plain state edit.
#[derive(Debug, Clone)]
pub enum WifiEvent {
    /// The world, as of a moment ago. Replaces whatever came before.
    Snapshot(WifiSnapshot),
    /// A join has been handed to NetworkManager, named by SSID.
    ///
    /// Sent optimistically the instant the command is picked up, because the
    /// call itself can take seconds and the row needs its spinner now.
    Connecting(String),
    /// An operation failed. Already human-readable; shown as-is.
    Error(String),
    /// NetworkManager could not be reached, or there is no Wi-Fi to manage.
    ///
    /// Distinct from [`Error`](Self::Error) because it is not a failed action —
    /// it is a statement about the whole subsystem, and the page draws a quiet
    /// empty state for it rather than a dismissible warning.
    Unavailable(String),
}

// --- MARK: Snapshot ---

/// Where a link is in its lifecycle, reduced to the two states a row can draw.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkState {
    /// Associating, authenticating, or waiting for an address.
    Connecting,
    /// Up and carrying traffic.
    Connected,
}

/// The Wi-Fi world in one value, shaped for the page that draws it.
///
/// Every field is a decision already made. `known` and `others` are sorted
/// strongest-first and partitioned so that no SSID appears twice; the joined
/// network is in [`connection`](Self::connection) and never in `others`;
/// `weak`/`secured` are booleans rather than flag sets. A page reads fields.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct WifiSnapshot {
    /// NetworkManager's software radio switch.
    pub radio_enabled: bool,
    /// The rfkill switch is off, so `radio_enabled` cannot take effect.
    pub hardware_blocked: bool,
    /// The host actually has a Wi-Fi radio. When `false`, everything else here
    /// is meaningless and the page says "no adapter".
    pub present: bool,
    /// The network currently joined or being joined, if any.
    pub connection: Option<ActiveWifi>,
    /// Saved networks that are in range, strongest first.
    pub known: Vec<KnownNetwork>,
    /// Visible networks with no saved profile, strongest first.
    pub others: Vec<OtherNetwork>,
}

/// The network this machine is on (or is in the middle of joining).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActiveWifi {
    /// The network's name.
    pub ssid: String,
    /// Connected, or still negotiating.
    pub state: LinkState,
    /// Signal strength as a percentage, when NetworkManager reports one.
    pub strength: Option<u8>,
    /// The address NetworkManager assigned, in CIDR form, once there is one.
    pub ip4_address: Option<String>,
    /// This link's security is one of the discouraged ones — see
    /// [`is_weak_security`].
    pub weak_security: bool,
}

/// A saved network that is currently in range.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KnownNetwork {
    /// The network's name.
    pub ssid: String,
    /// The saved profile's `connection.uuid` — the handle for auto-join and
    /// forget, neither of which should go by name.
    pub uuid: String,
    /// Signal strength of the strongest access point, as a percentage.
    pub strength: u8,
    /// Joining needs a key (or already had one).
    pub secured: bool,
    /// The security on offer is one of the discouraged ones.
    pub weak: bool,
    /// This is the network currently joined.
    pub joined: bool,
    /// NetworkManager may re-join this profile on its own.
    pub auto_join: bool,
}

/// A visible network this machine has never saved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OtherNetwork {
    /// The network's name.
    pub ssid: String,
    /// Signal strength of the strongest access point, as a percentage.
    pub strength: u8,
    /// Joining will need a key, so the page prompts for one.
    pub secured: bool,
    /// The security on offer is one of the discouraged ones.
    pub weak: bool,
}

impl WifiSnapshot {
    /// Whether joining `ssid` needs a key, as far as this snapshot knows.
    ///
    /// `None` means the SSID isn't in this snapshot at all — a hidden network,
    /// or one that went out of range between the click and the command being
    /// picked up. Callers treat that as "assume secured", which degrades to a
    /// clear "no password was provided" error instead of a mystifying
    /// authentication failure.
    #[must_use]
    pub fn secured(&self, ssid: &str) -> Option<bool> {
        if let Some(known) = self.known.iter().find(|network| network.ssid == ssid) {
            return Some(known.secured);
        }
        if let Some(other) = self.others.iter().find(|network| network.ssid == ssid) {
            return Some(other.secured);
        }
        None
    }

    /// Whether `ssid` is the network currently joined or being joined.
    #[must_use]
    pub fn is_current(&self, ssid: &str) -> bool {
        self.connection
            .as_ref()
            .is_some_and(|current| current.ssid == ssid)
    }
}

// --- MARK: State ---

/// An unsubmitted password, for a network the user has clicked but not joined.
///
/// Pure UI state — it never reaches the worker until the user presses Join, at
/// which point it becomes a [`WifiCommand::Connect`]. It lives in [`WifiState`]
/// rather than in the view because xilem views are rebuilt from scratch on
/// every message and cannot hold anything of their own.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PasswordPrompt {
    /// The network the prompt is attached to.
    pub ssid: String,
    /// What has been typed so far.
    pub password: String,
}

/// The Wi-Fi half of the app's [`Store`](crate::state::Store).
///
/// Runtime only: nothing here is ever persisted, and a fresh
/// [`default`](Default::default) is a perfectly valid "we have not heard from
/// NetworkManager yet" — which is exactly what the page-building test runs
/// against.
///
/// The command channel's sending half is private because it has an invariant:
/// once the receiving end is gone the sender is worthless, and
/// [`send`](Self::send) is the only place that notices and drops it. Everything
/// else is plain data the page reads and writes freely.
#[derive(Debug, Default)]
pub struct WifiState {
    /// Set once, when [`wifi_worker`] is first built.
    sender: Option<UnboundedSender<WifiCommand>>,
    /// The last snapshot, or `None` before the first one lands — which the page
    /// draws as "looking for networks…".
    pub snapshot: Option<WifiSnapshot>,
    /// The SSID of a join in flight, so its row can show a spinner.
    pub connecting: Option<String>,
    /// The last failed operation, until the user dismisses it.
    pub error: Option<String>,
    /// NetworkManager itself is missing or has no Wi-Fi to offer.
    ///
    /// Separate from [`error`](Self::error): an error is about one action and
    /// goes away, this is about the whole page and doesn't.
    pub unavailable: Option<String>,
    /// The open password prompt, if the user is part-way through joining a
    /// secured network.
    pub prompt: Option<PasswordPrompt>,
    /// Whether the current network's card is showing its address, signal and
    /// security rather than just its name.
    ///
    /// Pure UI state, for the same reason [`prompt`](Self::prompt) is: a xilem
    /// view is rebuilt from scratch on every message and can remember nothing
    /// of its own, so "the user pressed Details…" has to live somewhere the
    /// rebuild can see. It is deliberately not per-SSID — there is only ever
    /// one current network, and a disclosure triangle that remembers a network
    /// you left is a disclosure triangle pointing at nothing.
    pub details_open: bool,
}

/// Same reasoning as [`WifiCommand`]: this is the page's whole interface to the
/// backend, published as a unit rather than grown one call site at a time.
#[allow(dead_code)]
impl WifiState {
    /// Ask the worker to do something.
    ///
    /// Silent when there is no worker (the page-building test) and when the
    /// worker is gone (the window is closing). Neither is a condition the UI can
    /// do anything about, and neither should be able to panic a settings app —
    /// so a dead channel is simply dropped, and later sends are free.
    pub fn send(&mut self, command: WifiCommand) {
        let Some(sender) = self.sender.as_ref() else {
            return;
        };
        if sender.send(command).is_err() {
            self.sender = None;
        }
    }

    /// Whether a worker is attached — i.e. whether commands go anywhere.
    #[must_use]
    pub fn is_live(&self) -> bool {
        self.sender.is_some()
    }

    /// Clear the last error, for the page's dismiss affordance.
    pub fn dismiss_error(&mut self) {
        self.error = None;
    }

    /// Adopt the sending half of the command channel. Called once, at build.
    fn attach(&mut self, sender: UnboundedSender<WifiCommand>) {
        self.sender = Some(sender);
    }

    /// Fold one event into the state, on the main thread.
    ///
    /// The only subtle part is [`connecting`](Self::connecting), which is
    /// optimistic state and therefore needs an explicit way to end. It is
    /// cleared when the snapshot shows that network actually joined, and when
    /// anything fails — so the two ways a join can finish are the two ways the
    /// spinner stops.
    fn apply(&mut self, event: WifiEvent) {
        match event {
            WifiEvent::Snapshot(snapshot) => {
                if let Some(ssid) = self.connecting.as_deref()
                    && snapshot.connection.as_ref().is_some_and(|current| {
                        current.ssid == ssid && current.state == LinkState::Connected
                    })
                {
                    self.connecting = None;
                }
                self.unavailable = None;
                self.snapshot = Some(snapshot);
            }
            WifiEvent::Connecting(ssid) => {
                // The prompt did its job the moment the join was submitted;
                // leaving it open would put a filled password field under a
                // row that is already spinning.
                if self.prompt.as_ref().is_some_and(|open| open.ssid == ssid) {
                    self.prompt = None;
                }
                self.error = None;
                self.connecting = Some(ssid);
            }
            WifiEvent::Error(message) => {
                self.connecting = None;
                self.error = Some(message);
            }
            WifiEvent::Unavailable(message) => {
                self.connecting = None;
                // A failed action is a statement about a world that no longer
                // exists, and the offline card has no dismiss affordance to
                // clear it with — so an error that outlived its subsystem would
                // outlive it for good.
                self.error = None;
                self.snapshot = None;
                self.unavailable = Some(message);
            }
        }
    }
}

// --- MARK: The worker view ---

/// The Wi-Fi backend, as an element-less view.
///
/// Draws nothing, so — like [`crate::config::ConfigStore::watchers`] — it rides
/// in the alongside slot of [`fork`](xilem::core::fork). Its future is spawned
/// once when the tree is built and aborted when the tree goes away, which is
/// what makes the D-Bus connection's lifetime exactly the window's lifetime.
///
/// Two projections rather than one, because the two halves of handling an event
/// belong to different layers. `wifi` reaches the runtime state this module
/// owns; `on_event` is the caller's chance to do something with the event that
/// this module has no business knowing about — in this app, mirroring the radio
/// and the SSID back into `wifi.ron` so other CrownOS apps see them.
pub fn wifi_worker<State, F>(
    wifi: fn(&mut State) -> &mut WifiState,
    on_event: F,
) -> impl View<State, (), ViewCtx, Element = NoElement> + Send + Sync
where
    State: 'static,
    F: Fn(&mut State, &WifiEvent) + Send + Sync + 'static,
{
    worker_raw(
        |proxy: MessageProxy<WifiEvent>, commands: UnboundedReceiver<WifiCommand>| {
            run(proxy, commands)
        },
        move |state: &mut State, sender| wifi(state).attach(sender),
        move |state: &mut State, event: WifiEvent| {
            on_event(state, &event);
            wifi(state).apply(event);
        },
    )
}

// --- MARK: The worker future ---

/// The whole async half of this module: connect, publish, then loop.
///
/// Returns only when the command channel closes, which happens when the view
/// tree — and therefore the window — is gone.
///
/// Two deadlines, not one, because the loop waits on two unrelated things and
/// conflating them would make each the other's hostage. `refresh_at` is when
/// the world is next worth re-reading; `resubscribe_at` is the earliest moment
/// it is worth asking NetworkManager for a change stream again. The `select!`
/// sleeps until whichever comes first, and both are *floors* rather than
/// promises: [`schedule`] only ever moves a deadline earlier, so a pending
/// refresh can be brought forward but never pushed back.
///
/// The invariant that makes the loop live is that `events` being `None` always
/// comes with a `resubscribe_at`. That is what the pre-fix code got wrong: a
/// failed subscription left nothing armed, and a `select!` whose only live arm
/// is `commands.recv()` is a worker that has stopped watching the network.
async fn run(proxy: MessageProxy<WifiEvent>, mut commands: UnboundedReceiver<WifiCommand>) {
    let manager = match NetworkManager::new().await {
        Ok(manager) => manager,
        Err(error) => {
            let reason = format!("NetworkManager is not available: {error}");
            let _ = proxy.message(WifiEvent::Unavailable(reason.clone()));
            park(&proxy, &mut commands, reason).await;
            return;
        }
    };

    // Best-effort nudge so the first snapshot isn't yesterday's scan results.
    // A failure here is not worth reporting: the snapshot below still works,
    // it is just staler.
    let _ = manager.scan_networks(None).await;

    let mut latest = publish(&manager, &proxy).await;
    let mut events: Option<NetworkEventStream> = None;
    let mut refresh_at: Option<Instant> = None;
    // The first attempt is due immediately; every later one is a backoff.
    let mut resubscribe_at: Option<Instant> = Some(Instant::now());
    // Commands in flight. Reaped by the loop, aborted with the window.
    let mut running: JoinSet<()> = JoinSet::new();

    loop {
        // Opened here rather than in a branch handler because every path that
        // loses the stream — a failed subscribe, a restart, an ended stream —
        // wants the same retry, and the loop top is the one place all three
        // pass through.
        if events.is_none() && resubscribe_at.is_some_and(|due| due <= Instant::now()) {
            resubscribe_at = None;
            match open_events(&manager, &proxy).await {
                Some(stream) => {
                    events = Some(stream);
                    // Whatever happened while we were deaf is invisible to us
                    // now, so the world is re-read at once rather than waiting
                    // for a signal that already went past.
                    schedule(&mut refresh_at, Instant::now());
                }
                // NetworkManager is most likely mid-start or mid-restart. Back
                // off on the timer instead of sleeping here, so a command that
                // arrives during the pause is still answered.
                None => resubscribe_at = Some(Instant::now() + RESUBSCRIBE_DELAY),
            }
        }

        // Copied out so the timer future borrows nothing the handlers below
        // want to assign to.
        let deadline = earliest(refresh_at, resubscribe_at);

        xilem::tokio::select! {
            () = wait_until(deadline) => {
                // The timer serves both deadlines, so it fires for whichever
                // was sooner and the other is simply not due yet. A resubscribe
                // needs no handler at all: it is consumed at the top of the
                // loop, which is where this arm goes next.
                if refresh_at.is_some_and(|due| due <= Instant::now()) {
                    refresh_at = None;
                    latest = publish(&manager, &proxy).await;
                }
            }
            signal = next_signal(&mut events) => {
                match signal {
                    // NetworkManager went away and came back: every object path
                    // we were watching is stale, so the stream is rebuilt from
                    // scratch — after a pause, because a daemon that is still
                    // coming back hands out streams that end immediately.
                    Some(Ok(NetworkEvent::NetworkManagerRestarted)) | None => {
                        events = None;
                        resubscribe_at = Some(Instant::now() + RESUBSCRIBE_DELAY);
                    }
                    Some(Ok(_)) => {}
                    Some(Err(error)) => {
                        let _ = proxy.message(WifiEvent::Error(error.to_string()));
                    }
                }
                // Leading-edge debounce: the first signal of a burst fixes the
                // deadline, later ones ride along, so a chatty radio can delay
                // a refresh but never postpone it indefinitely.
                schedule(&mut refresh_at, Instant::now() + REFRESH_DEBOUNCE);
            }
            command = commands.recv() => {
                let Some(command) = command else {
                    return;
                };
                if let WifiCommand::Connect { ssid, .. } = &command {
                    // Optimistic, and sent before anything is spawned: the join
                    // can sit for half a minute and the row needs its spinner
                    // for all of it.
                    let _ = proxy.message(WifiEvent::Connecting(ssid.clone()));
                }
                // Planned here, where the last snapshot is, and run elsewhere.
                let job = plan(command, latest.as_ref());
                running.spawn(run_command(manager.clone(), job, proxy.clone()));
            }
            () = next_finished(&mut running) => {
                // Something changed, or tried to. Either way the world is worth
                // a look — and a command that *failed* gets one too, which is
                // what stops a snapshot from being owed to an event that will
                // never arrive.
                schedule(&mut refresh_at, Instant::now() + REFRESH_DEBOUNCE);
            }
        }
    }
}

/// Answer every command with the same bad news, until the window closes.
///
/// The alternative — returning and letting the channel fill up — would leave
/// the UI accepting clicks that go nowhere and never explaining why.
async fn park(
    proxy: &MessageProxy<WifiEvent>,
    commands: &mut UnboundedReceiver<WifiCommand>,
    reason: String,
) {
    while commands.recv().await.is_some() {
        if proxy
            .message(WifiEvent::Unavailable(reason.clone()))
            .is_err()
        {
            break;
        }
    }
}

/// Wait for the next refresh signal, or forever when there is no stream.
///
/// The `pending` arm is what lets the [`select!`](xilem::tokio::select) above
/// treat "no event stream" as "this branch simply never fires" instead of
/// needing a second loop shape.
async fn next_signal(
    events: &mut Option<NetworkEventStream>,
) -> Option<nmrs::Result<NetworkEvent>> {
    match events {
        Some(stream) => stream.next().await,
        None => std::future::pending().await,
    }
}

/// Wait for the next command to finish, or forever when none are running.
///
/// Same shape and same reason as [`next_signal`]: an empty [`JoinSet`] yields
/// `None` the instant it is polled, which inside a `select!` is a spin rather
/// than a wait. The completion value is dropped, panic and all — a command's
/// own reporting is done by then, and the loop's only interest is that
/// *something* finished.
async fn next_finished(running: &mut JoinSet<()>) {
    if running.is_empty() {
        std::future::pending::<()>().await;
    }
    let _ = running.join_next().await;
}

/// Sleep until `deadline`, or forever when there is nothing pending.
async fn wait_until(deadline: Option<Instant>) {
    match deadline {
        Some(deadline) => sleep_until(deadline).await,
        None => std::future::pending().await,
    }
}

/// Bring a deadline forward to `due`, or set it if there wasn't one.
///
/// Deadlines here are answers to "how long may this wait at most", so the
/// earliest one wins and a later request never delays an earlier one. That is
/// also what makes the debounce a *leading* window: the second signal of a
/// burst asks for a deadline further out than the first and is ignored.
fn schedule(deadline: &mut Option<Instant>, due: Instant) {
    *deadline = Some(deadline.map_or(due, |pending| pending.min(due)));
}

/// The sooner of two optional deadlines.
fn earliest(left: Option<Instant>, right: Option<Instant>) -> Option<Instant> {
    match (left, right) {
        (Some(left), Some(right)) => Some(left.min(right)),
        (Some(only), None) | (None, Some(only)) => Some(only),
        (None, None) => None,
    }
}

/// Subscribe to NetworkManager's change signals, reporting a failure once.
async fn open_events(
    manager: &NetworkManager,
    proxy: &MessageProxy<WifiEvent>,
) -> Option<NetworkEventStream> {
    match manager.network_events().await {
        Ok(stream) => Some(stream),
        Err(error) => {
            let _ = proxy.message(WifiEvent::Error(format!(
                "Lost track of network changes: {error}"
            )));
            None
        }
    }
}

/// Read the world, derive a snapshot, send it, and keep a copy.
///
/// The copy is not a cache for the UI — the UI has its own. It is what
/// [`execute`] consults to answer questions the command itself does not carry,
/// such as whether an SSID is open and whether it is the one currently joined.
async fn publish(
    manager: &NetworkManager,
    proxy: &MessageProxy<WifiEvent>,
) -> Option<WifiSnapshot> {
    match manager.snapshot().await {
        Ok(raw) => {
            let snapshot = derive_snapshot(&raw);
            let _ = proxy.message(WifiEvent::Snapshot(snapshot.clone()));
            Some(snapshot)
        }
        Err(error) => {
            let _ = proxy.message(WifiEvent::Error(format!(
                "Could not read the network state: {error}"
            )));
            None
        }
    }
}

// --- MARK: Snapshot derivation ---

/// One SSID's row-to-be, still undecided about which list it belongs in.
///
/// The intermediate exists because [`NetworkSnapshot::wifi_groups`] groups by
/// `(interface, SSID)` and this module's lists are keyed by SSID alone, so
/// there is a merge to do that neither [`KnownNetwork`] nor [`OtherNetwork`]
/// can express — the merge has to happen while "known" is still a field and not
/// yet a choice of vector.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Candidate {
    /// The network's name.
    ssid: String,
    /// The strongest access point one adapter can see.
    strength: u8,
    /// Joining needs a key.
    secured: bool,
    /// The security on offer is one of the discouraged ones.
    weak: bool,
    /// Some saved profile matches this network.
    known: bool,
    /// This adapter is on this network.
    active: bool,
    /// The saved profile's uuid, empty when the group named none.
    uuid: String,
    /// That profile's `connection.autoconnect`; `true` when there is no
    /// profile to read it from, matching NetworkManager's own default.
    auto_join: bool,
}

/// Turn one `nmrs` read into the value the page draws.
///
/// Pure: no I/O, no awaiting, no `self`. [`NetworkSnapshot::wifi_groups`] has
/// already collapsed access points into one row per `(interface, SSID)` and
/// dropped hidden networks — *per interface*, which is why [`merge_by_ssid`]
/// comes first here — and the rest of the work is classification (known versus
/// not, secured versus not, weak versus not) and ordering.
fn derive_snapshot(raw: &NetworkSnapshot) -> WifiSnapshot {
    let groups = raw.wifi_groups();
    let connection = active_wifi(raw, &groups);

    let mut known = Vec::new();
    let mut others = Vec::new();

    for candidate in merge_by_ssid(groups.iter().map(|group| candidate(raw, group))) {
        if candidate.known {
            known.push(KnownNetwork {
                ssid: candidate.ssid,
                uuid: candidate.uuid,
                strength: candidate.strength,
                secured: candidate.secured,
                weak: candidate.weak,
                joined: candidate.active,
                auto_join: candidate.auto_join,
            });
        } else if !candidate.active {
            // The joined network is drawn by the current-network card, so it
            // must not also appear in the list of things to join.
            others.push(OtherNetwork {
                ssid: candidate.ssid,
                strength: candidate.strength,
                secured: candidate.secured,
                weak: candidate.weak,
            });
        }
    }

    known.sort_by(|a, b| by_signal((a.strength, &a.ssid), (b.strength, &b.ssid)));
    others.sort_by(|a, b| by_signal((a.strength, &a.ssid), (b.strength, &b.ssid)));

    WifiSnapshot {
        radio_enabled: raw.wifi.enabled,
        hardware_blocked: !raw.wifi.hardware_enabled,
        present: raw.wifi.present,
        connection,
        known,
        others,
    }
}

/// Everything one adapter's view of one network contributes to its row.
fn candidate(raw: &NetworkSnapshot, group: &WifiNetworkGroup) -> Candidate {
    let security = &group.strongest.security;
    Candidate {
        ssid: group.ssid.clone(),
        strength: group.strongest.strength,
        secured: !security.is_open(),
        weak: is_weak_security(security),
        known: group.known,
        active: group.active,
        uuid: group
            .saved_profiles
            .first()
            .map(|saved| saved.uuid.clone())
            .unwrap_or_default(),
        auto_join: saved_profile(raw, group).is_none_or(|saved| saved.autoconnect),
    }
}

/// Collapse one group per adapter into one row per network.
///
/// Two Wi-Fi adapters see the same air, so a machine with two of them gets two
/// groups for every network in range — and the duplicate row is the least of
/// it. Only one adapter is joined, so the network you are on arrives once as
/// `known`/`active` and once as neither, which puts the same SSID in the
/// current-network card *and* in the list of networks to join. Merging here is
/// what makes [`WifiSnapshot`]'s "no SSID appears twice" true rather than
/// aspirational.
///
/// The merge keeps the best-informed answer to each question:
///
/// * strength, and the security that goes with it, come from whichever adapter
///   hears the network best — that is the access point a row is describing;
/// * `known` and `active` are unions, because a network is saved if either
///   adapter has a profile for it and joined if either adapter is on it;
/// * the first non-empty uuid wins, along with the auto-join flag belonging to
///   it, since one saved profile is what both adapters would use anyway.
///
/// First-seen order is preserved. The caller sorts by signal afterwards, so the
/// order here is only about being deterministic on the way in.
fn merge_by_ssid(candidates: impl IntoIterator<Item = Candidate>) -> Vec<Candidate> {
    let mut merged: Vec<Candidate> = Vec::new();
    let mut seen: HashMap<String, usize> = HashMap::new();

    for candidate in candidates {
        let Some(&index) = seen.get(&candidate.ssid) else {
            seen.insert(candidate.ssid.clone(), merged.len());
            merged.push(candidate);
            continue;
        };

        let row = &mut merged[index];
        if candidate.strength > row.strength {
            row.strength = candidate.strength;
            row.secured = candidate.secured;
            row.weak = candidate.weak;
        }
        row.known |= candidate.known;
        row.active |= candidate.active;
        if row.uuid.is_empty() {
            row.uuid = candidate.uuid;
            row.auto_join = candidate.auto_join;
        }
    }

    merged
}

/// Strongest first, ties broken alphabetically.
///
/// The tiebreak is not cosmetic: without it, two access points reporting the
/// same strength would swap places every time a snapshot arrives, and the list
/// would visibly churn while nothing is happening.
fn by_signal(left: (u8, &str), right: (u8, &str)) -> Ordering {
    right.0.cmp(&left.0).then_with(|| left.1.cmp(right.1))
}

/// The one active Wi-Fi connection, if NetworkManager reports one.
///
/// Only `Activating` and `Activated` count. A connection that is on its way
/// down is, for a settings page, already gone — showing it would put a
/// "Connected" dot next to a network the user just left.
fn active_wifi(raw: &NetworkSnapshot, groups: &[WifiNetworkGroup]) -> Option<ActiveWifi> {
    raw.active_connections.iter().find_map(|connection| {
        let ActiveConnection::Wifi(active) = connection else {
            return None;
        };
        let state = match active.state {
            ActiveConnectionState::Activating => LinkState::Connecting,
            ActiveConnectionState::Activated => LinkState::Connected,
            _ => return None,
        };
        // The active connection carries no security flags of its own, so the
        // "weak security" warning comes from the access point it is on.
        let weak_security = groups
            .iter()
            .find(|group| group.ssid == active.ssid)
            .is_some_and(|group| is_weak_security(&group.strongest.security));

        Some(ActiveWifi {
            ssid: active.ssid.clone(),
            state,
            strength: active.strength,
            ip4_address: active.ip4_address.clone(),
            weak_security,
        })
    })
}

/// The full saved profile behind a known group, for the fields the brief
/// listing inside a group doesn't carry (namely `autoconnect`).
fn saved_profile<'a>(
    raw: &'a NetworkSnapshot,
    group: &WifiNetworkGroup,
) -> Option<&'a SavedConnection> {
    let uuid = &group.saved_profiles.first()?.uuid;
    raw.saved_wifi_profiles
        .iter()
        .find(|profile| &profile.uuid == uuid)
}

/// Whether an access point's security is one of the discouraged ones: open,
/// WEP, or WPA1-only.
///
/// A caveat worth stating, because it shapes the last clause: `nmrs` merges
/// NetworkManager's `WpaFlags` (WPA1) and `RsnFlags` (WPA2/3) into one
/// [`SecurityFeatures`], so "WPA1 only" cannot be read off directly. TKIP
/// without CCMP is the stand-in — a WPA2 access point always advertises CCMP,
/// so an AP offering only the older cipher is the population we mean.
///
/// WPA3 short-circuits the whole test: SAE, OWE and Suite-B are never weak,
/// even when the same AP still advertises a legacy cipher for compatibility.
#[must_use]
pub fn is_weak_security(security: &SecurityFeatures) -> bool {
    if security.sae || security.owe || security.owe_transition_mode || security.eap_suite_b_192 {
        return false;
    }
    security.is_open() || security.wep40 || security.wep104 || (security.tkip && !security.ccmp)
}

// --- MARK: Command execution ---

/// A command with every question the last snapshot could answer already
/// answered.
///
/// The split between this and [`WifiCommand`] is the split between the two
/// tasks. A command is planned on the loop, which is the only place the last
/// snapshot lives; a job is run on a task of its own, which must therefore own
/// everything it needs and consult nothing. What survives the crossing is
/// exactly the two decisions the command itself could not carry: which
/// credentials to offer, and which of the two ways to forget.
// `WifiSecurity` is much the biggest thing here, because its EAP arm carries a
// whole certificate configuration. Boxing it would shrink a value that is built
// once per click and moved once into a task, at the cost of an allocation and a
// deref on every arm that touches it.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq, Eq)]
enum Job {
    /// Flip NM's global software radio switch.
    SetRadio(bool),
    /// Ask every Wi-Fi device to re-scan.
    Scan,
    /// Join `ssid`, offering `credentials`.
    Connect {
        /// The network to join.
        ssid: String,
        /// What [`credentials_for`] decided to offer.
        credentials: WifiSecurity,
    },
    /// Drop the current Wi-Fi connection.
    Disconnect,
    /// Set a saved profile's `connection.autoconnect`.
    SetAutoJoin {
        /// The saved profile's `connection.uuid`.
        uuid: String,
        /// The new value.
        enabled: bool,
    },
    /// Delete a saved profile.
    Forget {
        /// The network's name, for the by-SSID call.
        ssid: String,
        /// The saved profile's uuid, for the by-uuid call.
        uuid: String,
        /// Whether this is the network currently joined, which decides which
        /// of the two calls is the right one.
        joined: bool,
    },
}

/// Fold what the last snapshot knows into a command, so the job that comes out
/// can be run by a task that knows nothing.
///
/// Pure, and deliberately so: this is where the interesting decisions are, and
/// keeping them out of the async half is what makes them testable without a
/// NetworkManager.
fn plan(command: WifiCommand, latest: Option<&WifiSnapshot>) -> Job {
    match command {
        WifiCommand::SetRadio(enabled) => Job::SetRadio(enabled),
        WifiCommand::Scan => Job::Scan,
        WifiCommand::Connect { ssid, password } => {
            let credentials = credentials_for(latest, &ssid, password);
            Job::Connect { ssid, credentials }
        }
        WifiCommand::Disconnect => Job::Disconnect,
        WifiCommand::SetAutoJoin { uuid, enabled } => Job::SetAutoJoin { uuid, enabled },
        WifiCommand::Forget { ssid, uuid } => {
            let joined = latest.is_some_and(|snapshot| snapshot.is_current(&ssid));
            Job::Forget { ssid, uuid, joined }
        }
    }
}

/// Run one job to completion, off the loop, and report what happened.
///
/// This is the whole body of a spawned command task. It owns its clones —
/// [`NetworkManager`] is a handle over a shared D-Bus connection and
/// [`MessageProxy`] is a channel sender, so both are cheap and neither
/// duplicates any state — which is what lets the loop hand the job over and go
/// straight back to answering signals.
///
/// Only failure is reported. Success is observed the way every other change is:
/// as the next [`WifiEvent::Snapshot`], which the loop asks for as soon as this
/// task finishes. That is also how a join's spinner stops — on success by the
/// snapshot showing it joined, on failure by the [`WifiEvent::Error`] below.
async fn run_command(manager: NetworkManager, job: Job, proxy: MessageProxy<WifiEvent>) {
    if let Err(message) = attempt(&manager, job).await {
        let _ = proxy.message(WifiEvent::Error(message));
    }
}

/// Run a job, and give one that merely lost a race a second chance.
///
/// `nmrs`'s `try_*` calls refuse with [`ConnectionError::ConnectionInProgress`]
/// whenever *any* device on the machine is mid-transition — an ethernet cable
/// being plugged in is enough — so that error is far more often a moment's
/// contention than a real failure. Surfacing it as-is turned an ordinary click
/// into a red card. One retry after [`RETRY_DELAY`] turns the common case into
/// a join that took a little longer, and leaves only genuinely sustained
/// contention to be reported — in the transient wording of [`BUSY_MESSAGE`],
/// which says the useful thing rather than naming a D-Bus condition.
///
/// The retry is here rather than in the loop because it is two seconds of
/// sleeping, and the loop's whole point is that it never does that.
async fn attempt(manager: &NetworkManager, job: Job) -> Result<(), String> {
    if let Err(error) = run_job(manager, &job).await {
        if !matches!(error, ConnectionError::ConnectionInProgress) {
            return Err(describe(&job, &error));
        }
        xilem::tokio::time::sleep(RETRY_DELAY).await;
        return run_job(manager, &job)
            .await
            .map_err(|error| describe(&job, &error));
    }
    Ok(())
}

/// One job, one `nmrs` call, and the error exactly as `nmrs` gave it.
///
/// Left un-worded so that [`attempt`] can still tell the failures apart;
/// turning one into a sentence is [`describe`]'s job.
async fn run_job(manager: &NetworkManager, job: &Job) -> nmrs::Result<()> {
    match job {
        Job::SetRadio(enabled) => manager.set_wireless_enabled(*enabled).await,

        Job::Scan => manager.scan_networks(None).await,

        Job::Connect { ssid, credentials } => {
            manager.try_connect(ssid, None, credentials.clone()).await
        }

        Job::Disconnect => manager.disconnect(None).await,

        Job::SetAutoJoin { uuid, enabled } => {
            // `SettingsPatch` is `#[non_exhaustive]`, so it is filled in field
            // by field rather than with a struct literal.
            let mut patch = SettingsPatch::default();
            patch.autoconnect = Some(*enabled);
            manager.update_saved_connection(uuid, patch).await
        }

        Job::Forget { ssid, uuid, joined } => {
            if *joined {
                // Only the by-SSID call tears the live connection down first.
                manager.forget(ssid).await
            } else {
                manager.delete_saved_connection(uuid).await
            }
        }
    }
}

/// The sentence to show when `job` fails with `error`.
///
/// Contention is not about the job at all, so it gets the same wording whatever
/// was being attempted; everything else is named by the thing the user asked
/// for, because that is what they will be looking at when it appears.
fn describe(job: &Job, error: &ConnectionError) -> String {
    if matches!(error, ConnectionError::ConnectionInProgress) {
        return BUSY_MESSAGE.to_owned();
    }
    match job {
        Job::SetRadio(enabled) => format!("Could not turn Wi-Fi {}: {error}", on_off(*enabled)),
        Job::Scan => format!("Could not scan for networks: {error}"),
        Job::Connect { ssid, .. } => format!("Could not join {ssid}: {error}"),
        Job::Disconnect => format!("Could not disconnect: {error}"),
        Job::SetAutoJoin { .. } => format!("Could not change auto-join: {error}"),
        Job::Forget { ssid, .. } => format!("Could not forget {ssid}: {error}"),
    }
}

/// Which credentials to hand NetworkManager for a join.
///
/// Three cases, and the caller only distinguishes one of them:
///
/// * a freshly typed key — use it, which also makes NetworkManager rebuild the
///   profile rather than reuse a stale saved secret;
/// * nothing typed, network open — [`WifiSecurity::Open`];
/// * nothing typed, network secured — an empty PSK, which is `nmrs`'s
///   documented "use the saved secret" sentinel, and which fails cleanly with
///   `MissingPassword` when there is no saved profile to draw on.
fn credentials_for(
    latest: Option<&WifiSnapshot>,
    ssid: &str,
    password: Option<String>,
) -> WifiSecurity {
    if let Some(psk) = password {
        return WifiSecurity::WpaPsk { psk };
    }
    // Unknown SSIDs are assumed secured — see `WifiSnapshot::secured`.
    let secured = latest
        .and_then(|snapshot| snapshot.secured(ssid))
        .unwrap_or(true);
    if secured {
        WifiSecurity::WpaPsk { psk: String::new() }
    } else {
        WifiSecurity::Open
    }
}

/// The word that reads best inside "Could not turn Wi-Fi …".
fn on_off(enabled: bool) -> &'static str {
    if enabled { "on" } else { "off" }
}

#[cfg(test)]
mod tests {
    use super::*;
    use xilem::tokio::sync::mpsc::unbounded_channel;

    /// `nmrs`'s own types are `#[non_exhaustive]` and mostly have no `Default`,
    /// so [`derive_snapshot`] itself cannot be driven from a test without a
    /// live NetworkManager. What *is* reachable is every decision it delegates:
    /// the ordering, the weak-security rule, and the lookups
    /// [`execute`] makes against the last snapshot. Those are tested here;
    /// `SecurityFeatures` is the one `nmrs` type that cooperates, because it
    /// derives `Default` and has public fields.
    fn security(edit: impl FnOnce(&mut SecurityFeatures)) -> SecurityFeatures {
        let mut features = SecurityFeatures::default();
        edit(&mut features);
        features
    }

    fn known(ssid: &str, strength: u8, secured: bool) -> KnownNetwork {
        KnownNetwork {
            ssid: ssid.to_owned(),
            uuid: format!("{ssid}-uuid"),
            strength,
            secured,
            weak: false,
            joined: false,
            auto_join: true,
        }
    }

    fn other(ssid: &str, strength: u8, secured: bool) -> OtherNetwork {
        OtherNetwork {
            ssid: ssid.to_owned(),
            strength,
            secured,
            weak: false,
        }
    }

    /// One adapter's view of one network, as [`candidate`] would have built it
    /// from a group `nmrs` cannot be made to hand us in a test.
    fn seen(ssid: &str, strength: u8, uuid: &str, active: bool) -> Candidate {
        Candidate {
            ssid: ssid.to_owned(),
            strength,
            secured: true,
            weak: false,
            known: !uuid.is_empty(),
            active,
            uuid: uuid.to_owned(),
            auto_join: true,
        }
    }

    // --- MARK: Ordering ---

    #[test]
    fn rows_sort_strongest_first_then_alphabetically() {
        let mut rows = vec![
            known("Workshop", 40, true),
            known("Attic", 90, true),
            known("Basement", 40, true),
            known("Garden", 90, true),
        ];
        rows.sort_by(|a, b| by_signal((a.strength, &a.ssid), (b.strength, &b.ssid)));

        let order: Vec<&str> = rows.iter().map(|row| row.ssid.as_str()).collect();
        assert_eq!(
            order,
            ["Attic", "Garden", "Basement", "Workshop"],
            "strength descending, and equal strengths in a stable, visible order"
        );
    }

    // --- MARK: Merging adapters ---

    #[test]
    fn two_adapters_seeing_one_network_collapse_to_one_row() {
        let merged = merge_by_ssid([
            // wlan0 is the one that is joined, and the one that knows the
            // profile; wlan1 hears the same access point better.
            seen("Workshop", 40, "workshop-uuid", true),
            seen("Workshop", 80, "", false),
            seen("Cafe", 30, "", false),
        ]);

        assert_eq!(merged.len(), 2, "one row per SSID, not one per adapter");

        let workshop = &merged[0];
        assert_eq!(workshop.strength, 80, "the better-placed adapter's reading");
        assert!(workshop.known, "one adapter's profile makes it known");
        assert!(workshop.active, "one adapter's link makes it joined");
        assert_eq!(
            workshop.uuid, "workshop-uuid",
            "the uuid survives the adapter that had no profile"
        );
        assert_eq!(merged[1].ssid, "Cafe", "first-seen order is kept");
    }

    #[test]
    fn a_merged_network_cannot_be_both_joined_and_joinable() {
        let raw = [
            seen("Workshop", 80, "", false),
            seen("Workshop", 40, "workshop-uuid", true),
        ];

        // Without the merge this is the bug: one group is known and active, the
        // other is neither, so the same SSID lands in both lists.
        let (known, others) = partition(raw.clone().into_iter());
        assert_eq!((known, others), (1, 1), "the shape of the defect");

        let (known, others) = partition(merge_by_ssid(raw).into_iter());
        assert_eq!(
            (known, others),
            (1, 0),
            "merged, it is the network we are on and nothing else"
        );
    }

    /// How many candidates [`derive_snapshot`] would put in each list.
    fn partition(candidates: impl Iterator<Item = Candidate>) -> (usize, usize) {
        let mut known = 0;
        let mut others = 0;
        for candidate in candidates {
            if candidate.known {
                known += 1;
            } else if !candidate.active {
                others += 1;
            }
        }
        (known, others)
    }

    // --- MARK: Weak security ---

    #[test]
    fn open_wep_and_wpa1_only_are_weak() {
        assert!(
            is_weak_security(&SecurityFeatures::default()),
            "an open network is weak"
        );
        assert!(is_weak_security(&security(|s| {
            s.privacy = true;
            s.wep40 = true;
        })));
        assert!(is_weak_security(&security(|s| {
            s.privacy = true;
            s.wep104 = true;
        })));
        assert!(
            is_weak_security(&security(|s| {
                s.privacy = true;
                s.psk = true;
                s.tkip = true;
            })),
            "TKIP with no CCMP is the WPA1-only population"
        );
    }

    #[test]
    fn wpa2_and_wpa3_are_not_weak() {
        assert!(!is_weak_security(&security(|s| {
            s.privacy = true;
            s.psk = true;
            s.ccmp = true;
        })));
        assert!(
            !is_weak_security(&security(|s| {
                s.privacy = true;
                s.psk = true;
                s.tkip = true;
                s.ccmp = true;
            })),
            "a mixed WPA1/WPA2 AP still offers CCMP"
        );
        assert!(
            !is_weak_security(&security(|s| {
                s.privacy = true;
                s.sae = true;
                s.tkip = true;
            })),
            "WPA3 outranks a legacy cipher advertised for compatibility"
        );
        assert!(
            !is_weak_security(&security(|s| s.owe = true)),
            "OWE looks open but is encrypted"
        );
    }

    // --- MARK: Snapshot lookups ---

    #[test]
    fn secured_looks_in_both_lists_and_admits_ignorance() {
        let snapshot = WifiSnapshot {
            known: vec![known("Workshop", 70, true)],
            others: vec![other("Cafe", 55, false)],
            ..WifiSnapshot::default()
        };

        assert_eq!(snapshot.secured("Workshop"), Some(true));
        assert_eq!(snapshot.secured("Cafe"), Some(false));
        assert_eq!(
            snapshot.secured("Nowhere"),
            None,
            "an SSID this snapshot never saw must not be reported as open"
        );
    }

    #[test]
    fn credentials_follow_what_is_known_about_the_network() {
        let snapshot = WifiSnapshot {
            known: vec![known("Workshop", 70, true)],
            others: vec![other("Cafe", 55, false)],
            ..WifiSnapshot::default()
        };

        assert_eq!(
            credentials_for(Some(&snapshot), "Workshop", Some("hunter22".to_owned())),
            WifiSecurity::WpaPsk {
                psk: "hunter22".to_owned()
            },
            "a typed key is used as typed"
        );
        assert_eq!(
            credentials_for(Some(&snapshot), "Workshop", None),
            WifiSecurity::WpaPsk { psk: String::new() },
            "a saved secured network joins on its stored secret"
        );
        assert_eq!(
            credentials_for(Some(&snapshot), "Cafe", None),
            WifiSecurity::Open,
            "an open network needs no credentials at all"
        );
        assert_eq!(
            credentials_for(None, "Cafe", None),
            WifiSecurity::WpaPsk { psk: String::new() },
            "with no snapshot to consult, assume secured"
        );
    }

    // --- MARK: Planning ---

    #[test]
    fn a_job_carries_everything_the_snapshot_knew() {
        let snapshot = WifiSnapshot {
            connection: Some(ActiveWifi {
                ssid: "Workshop".to_owned(),
                state: LinkState::Connected,
                strength: Some(72),
                ip4_address: None,
                weak_security: false,
            }),
            known: vec![known("Workshop", 70, true), known("Attic", 30, true)],
            ..WifiSnapshot::default()
        };

        // Forgetting the joined network has to tear the link down first, which
        // only the by-SSID call does — so the choice is made here, while the
        // snapshot is still in reach.
        assert_eq!(
            plan(
                WifiCommand::Forget {
                    ssid: "Workshop".to_owned(),
                    uuid: "workshop-uuid".to_owned(),
                },
                Some(&snapshot),
            ),
            Job::Forget {
                ssid: "Workshop".to_owned(),
                uuid: "workshop-uuid".to_owned(),
                joined: true,
            }
        );
        assert_eq!(
            plan(
                WifiCommand::Forget {
                    ssid: "Attic".to_owned(),
                    uuid: "attic-uuid".to_owned(),
                },
                Some(&snapshot),
            ),
            Job::Forget {
                ssid: "Attic".to_owned(),
                uuid: "attic-uuid".to_owned(),
                joined: false,
            },
            "a saved network we are not on is deleted by uuid"
        );
        assert_eq!(
            plan(
                WifiCommand::Connect {
                    ssid: "Workshop".to_owned(),
                    password: None,
                },
                Some(&snapshot),
            ),
            Job::Connect {
                ssid: "Workshop".to_owned(),
                credentials: WifiSecurity::WpaPsk { psk: String::new() },
            },
            "a saved secured network joins on its stored secret"
        );
    }

    #[test]
    fn contention_is_worded_as_a_moment_rather_than_a_failure() {
        let job = Job::Connect {
            ssid: "Workshop".to_owned(),
            credentials: WifiSecurity::Open,
        };

        assert_eq!(
            describe(&job, &ConnectionError::ConnectionInProgress),
            BUSY_MESSAGE,
            "another device merely being mid-transition is not a failed join"
        );
        assert!(
            describe(&job, &ConnectionError::AuthFailed).starts_with("Could not join Workshop"),
            "a real failure still names what was attempted"
        );
    }

    #[test]
    fn is_current_only_matches_the_active_connection() {
        let snapshot = WifiSnapshot {
            connection: Some(ActiveWifi {
                ssid: "Workshop".to_owned(),
                state: LinkState::Connected,
                strength: Some(72),
                ip4_address: Some("192.168.1.20/24".to_owned()),
                weak_security: false,
            }),
            known: vec![known("Attic", 30, true)],
            ..WifiSnapshot::default()
        };

        assert!(snapshot.is_current("Workshop"));
        assert!(!snapshot.is_current("Attic"));
        assert!(!WifiSnapshot::default().is_current("Workshop"));
    }

    // --- MARK: The command channel ---

    #[test]
    fn send_reaches_an_attached_worker() {
        let (sender, mut receiver) = unbounded_channel();
        let mut state = WifiState::default();
        state.attach(sender);

        assert!(state.is_live());
        state.send(WifiCommand::SetRadio(false));

        assert_eq!(receiver.try_recv().ok(), Some(WifiCommand::SetRadio(false)));
    }

    #[test]
    fn send_without_a_worker_is_a_no_op() {
        let mut state = WifiState::default();
        assert!(!state.is_live());
        state.send(WifiCommand::Scan);
        assert!(!state.is_live(), "nothing to attach, nothing to drop");
    }

    #[test]
    fn a_closed_channel_is_dropped_rather_than_retried() {
        let (sender, receiver) = unbounded_channel::<WifiCommand>();
        let mut state = WifiState::default();
        state.attach(sender);
        drop(receiver);

        state.send(WifiCommand::Scan);
        assert!(
            !state.is_live(),
            "a send to a gone worker must retire the channel"
        );
        // And a second send must still not panic.
        state.send(WifiCommand::Scan);
    }

    // --- MARK: Event folding ---

    #[test]
    fn a_join_spins_until_it_lands() {
        let mut state = WifiState::default();
        state.prompt = Some(PasswordPrompt {
            ssid: "Workshop".to_owned(),
            password: "hunter22".to_owned(),
        });

        state.apply(WifiEvent::Connecting("Workshop".to_owned()));
        assert_eq!(state.connecting.as_deref(), Some("Workshop"));
        assert!(state.prompt.is_none(), "a submitted prompt closes");

        // A snapshot that doesn't show the network joined yet leaves the
        // spinner alone.
        state.apply(WifiEvent::Snapshot(WifiSnapshot::default()));
        assert_eq!(state.connecting.as_deref(), Some("Workshop"));

        state.apply(WifiEvent::Snapshot(WifiSnapshot {
            connection: Some(ActiveWifi {
                ssid: "Workshop".to_owned(),
                state: LinkState::Connected,
                strength: Some(80),
                ip4_address: None,
                weak_security: false,
            }),
            ..WifiSnapshot::default()
        }));
        assert!(state.connecting.is_none(), "joined, so the spinner stops");
    }

    #[test]
    fn a_failure_stops_the_spinner_and_is_dismissible() {
        let mut state = WifiState::default();
        state.apply(WifiEvent::Connecting("Workshop".to_owned()));
        state.apply(WifiEvent::Error("authentication failed".to_owned()));

        assert!(state.connecting.is_none());
        assert_eq!(state.error.as_deref(), Some("authentication failed"));

        state.dismiss_error();
        assert!(state.error.is_none());
    }

    #[test]
    fn unavailable_clears_the_world_and_snapshots_restore_it() {
        let mut state = WifiState::default();
        state.apply(WifiEvent::Snapshot(WifiSnapshot::default()));
        state.apply(WifiEvent::Error("could not scan".to_owned()));
        assert!(state.snapshot.is_some());

        state.apply(WifiEvent::Unavailable("no NetworkManager".to_owned()));
        assert!(state.snapshot.is_none());
        assert_eq!(state.unavailable.as_deref(), Some("no NetworkManager"));
        assert!(
            state.error.is_none(),
            "the offline card has no dismiss button, so a stale error would be permanent"
        );

        state.apply(WifiEvent::Snapshot(WifiSnapshot::default()));
        assert!(state.unavailable.is_none(), "it came back");
    }
}
