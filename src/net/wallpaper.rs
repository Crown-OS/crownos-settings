//! The wallpaper backend: the crownpaper daemon on one side, thumbnails on the
//! other.
//!
//! This module has two jobs that happen to share a lifecycle. It owns the IPC
//! connection to crownpaper — the daemon that actually draws the desktop — and
//! it produces the thumbnail grid the Appearance page shows, decoded off the
//! main thread and downscaled to grid size so that thirty 4K wallpapers cost
//! megabytes rather than gigabytes.
//!
//! The shape is the house shape (see [`crate::net`]): a [`WallpaperCommand`]
//! enum for everything the page can ask, a [`WallpaperEvent`] enum for
//! everything the backend can say, and a [`WallpaperState`] living in the
//! [`Store`](crate::state::Store) that holds the last word on each subject plus
//! the command channel. No `crownos_ipc` or `image` type escapes upward; the
//! one foreign type that does — [`ImageBrush`] — is xilem's own bitmap
//! currency, cheap to clone (an `Arc` under the hood) and compared by identity
//! on rebuild, so holding them in the store is exactly what the `image` view
//! wants.
//!
//! ## The picker's lifecycle is explicit
//!
//! Thumbnails and the daemon's preloaded textures are both memory spent on a
//! grid the user may only rarely look at, so neither exists while the
//! Appearance page is off screen. [`WallpaperState::enter`] and
//! [`WallpaperState::leave`] are the two verbs — [`crate::app`] calls them when
//! the page truly arrives and truly departs (not per rebuild, and not per
//! slide-transition churn). Enter scans the wallpaper directories, asks the
//! daemon to warm its cache, and starts decoding; leave aborts the decoding,
//! tells the daemon to drop the cache, and frees every brush on the spot.
//!
//! ## The daemon may not be there
//!
//! crownpaper is a separate process with its own life. The worker connects
//! lazily, retries on a timer while disconnected, re-subscribes to the
//! [`Changed`](crownpaper::Changed) event and re-warms the cache after every
//! reconnect, and answers a `Set` clicked while offline with an error rather
//! than silence. Browsing keeps working throughout — the thumbnails are ours,
//! not the daemon's — and the chosen path is persisted to `appearance.ron`
//! either way, so a daemon that comes up later still knows what was picked.
//!
//! Like the Wi-Fi backend, results are observed rather than returned: a
//! successful `set` shows up as a `Changed` event, which [`crate::state`]
//! mirrors back into the config so every other CrownOS app sees it too.

use std::path::{Path, PathBuf};
use std::time::Duration;

use crownos_ipc::adapter::tokio::AsyncClient;
use crownos_ipc::Error as IpcError;
use xilem::core::{MessageProxy, NoElement, View};
use xilem::masonry::peniko::{ImageAlphaType, ImageData};
use xilem::tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use xilem::tokio::task::JoinHandle;
use xilem::tokio::time::{sleep_until, Instant};
use xilem::view::worker_raw;
use xilem::{Blob, ImageBrush, ImageFormat, ViewCtx};

/// How long to wait before knocking on the daemon's socket again.
///
/// Long enough not to spin while crownpaper is genuinely absent, short enough
/// that a daemon restarting mid-session is back in the picker's good graces
/// before the user has read the offline notice.
const RECONNECT_DELAY: Duration = Duration::from_secs(2);

/// The transition handed to `set`, in seconds — crownpaper's own CLI default,
/// so a wallpaper picked here blooms in exactly like one picked anywhere else.
const SET_DURATION: f32 = 1.6;

/// The longest edge of a decoded thumbnail, in pixels.
///
/// Sized for the bento grid's feature tile — the widest a thumbnail is ever
/// drawn — so tiles stay sharp on a hidpi display while a thumbnail costs
/// ~500 KB of RGBA instead of the tens of megabytes the full wallpaper would.
/// The cost is transient either way: the grid is emptied when the picker
/// leaves the screen.
const THUMBNAIL_EDGE: u32 = 480;

/// Where wallpapers live: the user's own folder, then the system's.
const USER_WALLPAPERS: &str = ".backgrounds";
const SYSTEM_WALLPAPERS: &str = "/usr/share/backgrounds";

/// The extensions worth offering — the formats crownpaper can decode.
const WALLPAPER_EXTENSIONS: [&str; 5] = ["jpg", "jpeg", "png", "webp", "bmp"];

/// What a click has to be told when there is no daemon to receive it.
const SET_OFFLINE: &str =
    "The wallpaper service is not running — your choice is saved and will apply when it returns.";

// --- MARK: Commands ---

/// Everything the Appearance page can ask of this backend.
///
/// `Enter` and `Leave` are not sent by the page itself but by
/// [`crate::app`]'s navigation, which is the only place that knows the page
/// truly came and went rather than merely rebuilt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WallpaperCommand {
    /// The picker is on screen: scan, warm the daemon's cache, decode.
    Enter,
    /// The picker is gone: stop decoding and release both caches.
    Leave,
    /// Transition the desktop to the wallpaper at `path`.
    Set {
        /// Absolute path of the image to show.
        path: String,
    },
}

// --- MARK: Events ---

/// Everything the worker can tell the page.
#[derive(Debug, Clone)]
pub enum WallpaperEvent {
    /// The scan finished; these are the wallpapers on disk, in display order.
    /// Thumbnails follow one at a time.
    Found(Vec<String>),
    /// One wallpaper's thumbnail is ready.
    Thumbnail {
        /// The wallpaper it belongs to.
        path: String,
        /// The decoded, downscaled pixels.
        brush: ImageBrush,
    },
    /// A scanned file turned out not to decode, so it must not be offered.
    Unreadable {
        /// The file to drop from the grid.
        path: String,
    },
    /// The daemon reports this is what the desktop now shows.
    Current(String),
    /// The daemon answered; `Set` clicks will reach a desktop.
    Online,
    /// The daemon cannot be reached. Retries are already scheduled.
    Offline,
    /// An operation failed. Already human-readable; shown as-is.
    Error(String),
}

// --- MARK: State ---

/// One wallpaper the picker offers: its path, and its thumbnail once decoded.
#[derive(Debug, Clone, PartialEq)]
pub struct WallpaperEntry {
    /// Absolute path of the image file.
    pub path: String,
    /// The grid-sized pixels, or `None` while the decoder hasn't reached it.
    pub thumbnail: Option<ImageBrush>,
}

/// The wallpaper half of the app's [`Store`](crate::state::Store).
///
/// Runtime only, like [`WifiState`](crate::net::wifi::WifiState), and a fresh
/// [`default`](Default::default) is a valid "the picker is closed and nothing
/// has been heard" — which is what the page-building test renders.
#[derive(Debug, Default)]
pub struct WallpaperState {
    /// Set once, when [`wallpaper_worker`] is first built.
    sender: Option<UnboundedSender<WallpaperCommand>>,
    /// Whether the picker is on screen — the gate that keeps a scan finishing
    /// after [`leave`](Self::leave) from resurrecting a grid nobody is
    /// looking at.
    open: bool,
    /// The wallpapers on offer, in display order. Emptied on leave, which is
    /// where the thumbnail memory actually goes away.
    pub entries: Vec<WallpaperEntry>,
    /// Whether a scan has completed since the picker opened — what separates
    /// "still looking" from "looked, and there are none".
    pub scanned: bool,
    /// The path the daemon last said it was showing, if it has said anything.
    pub current: Option<String>,
    /// Whether the daemon is reachable. `false` until the first connect, so
    /// the page starts honest on a machine with no crownpaper at all.
    pub online: bool,
    /// The last failed operation, until the user dismisses it.
    pub error: Option<String>,
}

impl WallpaperState {
    /// Ask the worker to do something.
    ///
    /// Silent when there is no worker and when the worker is gone, for the
    /// same reasons as [`WifiState::send`](crate::net::wifi::WifiState::send):
    /// neither is a condition the UI can repair, and neither may panic a
    /// settings app.
    pub fn send(&mut self, command: WallpaperCommand) {
        let Some(sender) = self.sender.as_ref() else {
            return;
        };
        if sender.send(command).is_err() {
            self.sender = None;
        }
    }

    /// The picker came on screen: start paying for thumbnails.
    ///
    /// Idempotent — the navigation in [`crate::app`] is edge-triggered, but a
    /// second `Enter` mid-decode would throw away half a grid for nothing, so
    /// the guard is kept here too.
    pub fn enter(&mut self) {
        if self.open {
            return;
        }
        self.open = true;
        self.scanned = false;
        self.send(WallpaperCommand::Enter);
    }

    /// The picker is gone: stop paying for thumbnails.
    ///
    /// The entries are dropped *here*, on the main thread, rather than waiting
    /// for the worker's round trip — the brushes are the memory, and the whole
    /// point of leave is to give it back now.
    pub fn leave(&mut self) {
        if !self.open {
            return;
        }
        self.open = false;
        self.scanned = false;
        self.entries = Vec::new();
        self.send(WallpaperCommand::Leave);
    }

    /// Clear the last error, for the page's dismiss affordance.
    pub fn dismiss_error(&mut self) {
        self.error = None;
    }

    /// Adopt the sending half of the command channel. Called once, at build.
    ///
    /// If the window opened straight onto the picker, [`enter`](Self::enter)
    /// ran before there was anywhere to send to — so the missed `Enter` is
    /// replayed the moment there is.
    fn attach(&mut self, sender: UnboundedSender<WallpaperCommand>) {
        self.sender = Some(sender);
        if self.open {
            self.send(WallpaperCommand::Enter);
        }
    }

    /// Fold one event into the state, on the main thread.
    fn apply(&mut self, event: WallpaperEvent) {
        match event {
            WallpaperEvent::Found(paths) => {
                // A scan that finished after leave() describes a grid that no
                // longer exists.
                if !self.open {
                    return;
                }
                self.scanned = true;
                self.entries = paths
                    .into_iter()
                    .map(|path| WallpaperEntry {
                        path,
                        thumbnail: None,
                    })
                    .collect();
            }
            WallpaperEvent::Thumbnail { path, brush } => {
                // Looked up by path rather than index: entries may already
                // have shrunk (an Unreadable) or vanished (a leave) since the
                // decode was queued.
                if let Some(entry) = self.entries.iter_mut().find(|entry| entry.path == path) {
                    entry.thumbnail = Some(brush);
                }
            }
            WallpaperEvent::Unreadable { path } => {
                self.entries.retain(|entry| entry.path != path);
            }
            WallpaperEvent::Current(path) => self.current = Some(path),
            WallpaperEvent::Online => self.online = true,
            WallpaperEvent::Offline => self.online = false,
            WallpaperEvent::Error(message) => self.error = Some(message),
        }
    }
}

// --- MARK: The worker view ---

/// The wallpaper backend, as an element-less view.
///
/// Rides in the alongside slot of [`fork`](xilem::core::fork) next to
/// [`wifi_worker`](crate::net::wifi::wifi_worker), and lives exactly as long
/// as the window — deliberately *not* as long as the Appearance page, whose
/// slide-transition lifetime is torn down and rebuilt on every navigation.
/// The expensive state follows [`WallpaperCommand::Enter`]/`Leave` instead.
///
/// Two projections for the same reason as the Wi-Fi worker's: `wallpaper`
/// reaches the runtime state this module owns, `on_event` is the caller's
/// chance to mirror the daemon's word into `appearance.ron`.
pub fn wallpaper_worker<State, F>(
    wallpaper: fn(&mut State) -> &mut WallpaperState,
    on_event: F,
) -> impl View<State, (), ViewCtx, Element = NoElement> + Send + Sync
where
    State: 'static,
    F: Fn(&mut State, &WallpaperEvent) + Send + Sync + 'static,
{
    worker_raw(
        run,
        move |state: &mut State, sender| wallpaper(state).attach(sender),
        move |state: &mut State, event: WallpaperEvent| {
            on_event(state, &event);
            wallpaper(state).apply(event);
        },
    )
}

// --- MARK: The worker future ---

/// What woke the loop. Carries no borrows, so the handling below gets the
/// whole loop state back — which is what lets one `select!` wait on a client
/// it must also be free to replace.
enum Wake {
    Command(Option<WallpaperCommand>),
    Changed(Result<crownpaper::Changed, IpcError>),
    Retry,
}

/// The whole async half of this module: watch the daemon, serve the picker.
///
/// Returns only when the command channel closes, i.e. with the window.
///
/// Unlike the Wi-Fi loop, commands are handled inline rather than on spawned
/// tasks: every call here is a local-socket round trip the daemon answers
/// immediately, and the only long-running work — decoding — already lives on
/// its own task. A hung daemon would stall the loop, but a hung daemon has
/// already stalled the desktop it draws.
async fn run(
    proxy: MessageProxy<WallpaperEvent>,
    mut commands: UnboundedReceiver<WallpaperCommand>,
) {
    let mut daemon = Daemon::new();
    // What the daemon is currently asked to keep decoded: the last scan while
    // the picker is open, empty otherwise. Kept here — not in the UI state —
    // because a reconnect must re-warm the cache without asking anybody.
    let mut warm: Vec<String> = Vec::new();
    // The thumbnail decoder, while one is running. Aborted on leave, so a
    // closed picker stops costing CPU between two files.
    let mut decode: Option<JoinHandle<()>> = None;

    loop {
        daemon.reconnect_when_due(&proxy, &warm).await;

        let wake = match daemon.client.as_mut() {
            Some(client) => xilem::tokio::select! {
                command = commands.recv() => Wake::Command(command),
                changed = client.next_event::<crownpaper::Changed>() => Wake::Changed(changed),
            },
            None => xilem::tokio::select! {
                command = commands.recv() => Wake::Command(command),
                () = wait_until(daemon.retry_at) => Wake::Retry,
            },
        };

        match wake {
            // Consumed at the top of the loop; waking was the whole job.
            Wake::Retry => {}
            Wake::Changed(Ok(changed)) => {
                let _ = proxy.message(WallpaperEvent::Current(changed.path));
            }
            // The stream ending means the daemon ended; the timer takes over.
            Wake::Changed(Err(_)) => daemon.lost(&proxy),
            Wake::Command(None) => {
                if let Some(task) = decode.take() {
                    task.abort();
                }
                return;
            }
            Wake::Command(Some(WallpaperCommand::Enter)) => {
                let found = xilem::tokio::task::spawn_blocking(scan)
                    .await
                    .unwrap_or_default();
                let _ = proxy.message(WallpaperEvent::Found(found.clone()));
                warm = found;
                daemon.preload(&proxy, &warm).await;
                if let Some(previous) = decode.take() {
                    previous.abort();
                }
                decode = Some(xilem::tokio::spawn(decode_all(warm.clone(), proxy.clone())));
            }
            Wake::Command(Some(WallpaperCommand::Leave)) => {
                if let Some(task) = decode.take() {
                    task.abort();
                }
                let released = std::mem::take(&mut warm);
                daemon.unload(&proxy, released).await;
            }
            Wake::Command(Some(WallpaperCommand::Set { path })) => {
                daemon.set(&proxy, &warm, path).await;
            }
        }
    }
}

/// Sleep until `deadline`, or forever when there is nothing pending.
async fn wait_until(deadline: Option<Instant>) {
    match deadline {
        Some(deadline) => sleep_until(deadline).await,
        None => std::future::pending().await,
    }
}

// --- MARK: The daemon link ---

/// The connection to crownpaper, plus the discipline of getting it back.
///
/// The invariant that keeps the loop live is the Wi-Fi worker's: `client`
/// being `None` always comes with a `retry_at`, so a missing daemon is a timer
/// the `select!` is already waiting on, never a dead end.
struct Daemon {
    /// The connected, `Changed`-subscribed client, when there is one.
    client: Option<AsyncClient>,
    /// When the next connection attempt is due; `None` while connected.
    retry_at: Option<Instant>,
    /// Whether the UI has been told about the current outage, so a retry loop
    /// doesn't repeat the bad news every two seconds.
    announced_offline: bool,
}

impl Daemon {
    /// Not yet connected, first attempt due immediately.
    fn new() -> Self {
        Self {
            client: None,
            retry_at: Some(Instant::now()),
            announced_offline: false,
        }
    }

    /// Run the pending connection attempt, if one is due.
    async fn reconnect_when_due(&mut self, proxy: &MessageProxy<WallpaperEvent>, warm: &[String]) {
        if self.client.is_some() || !self.retry_at.is_some_and(|due| due <= Instant::now()) {
            return;
        }
        self.retry_at = None;
        self.connect(proxy, warm).await;
    }

    /// One connection attempt: connect, subscribe, re-warm, announce.
    async fn connect(&mut self, proxy: &MessageProxy<WallpaperEvent>, warm: &[String]) {
        match subscribed_client().await {
            Ok(client) => {
                self.client = Some(client);
                self.announced_offline = false;
                let _ = proxy.message(WallpaperEvent::Online);
                // A daemon that restarted came back with an empty cache, so
                // whatever the open picker asked it to hold is asked again.
                if !warm.is_empty() {
                    self.preload(proxy, warm).await;
                }
            }
            Err(_) => self.lost(proxy),
        }
    }

    /// The daemon is gone: drop the client, arm the retry, say so once.
    fn lost(&mut self, proxy: &MessageProxy<WallpaperEvent>) {
        self.client = None;
        self.retry_at = Some(Instant::now() + RECONNECT_DELAY);
        if !self.announced_offline {
            self.announced_offline = true;
            let _ = proxy.message(WallpaperEvent::Offline);
        }
    }

    /// Ask the daemon to decode `paths` ahead of time. Fire-and-forget.
    async fn preload(&mut self, proxy: &MessageProxy<WallpaperEvent>, paths: &[String]) {
        let Some(client) = self.client.as_mut() else {
            return;
        };
        let message = crownpaper::preload {
            paths: paths.to_vec(),
        };
        if client
            .notify::<crownpaper::preload>(&message, Vec::new())
            .await
            .is_err()
        {
            self.lost(proxy);
        }
    }

    /// Ask the daemon to forget what [`preload`](Self::preload) asked for.
    ///
    /// Skipped entirely when disconnected: a daemon that went away took its
    /// cache with it, and the one that comes back starts empty.
    async fn unload(&mut self, proxy: &MessageProxy<WallpaperEvent>, paths: Vec<String>) {
        if paths.is_empty() {
            return;
        }
        let Some(client) = self.client.as_mut() else {
            return;
        };
        let message = crownpaper::unload { paths };
        if client
            .notify::<crownpaper::unload>(&message, Vec::new())
            .await
            .is_err()
        {
            self.lost(proxy);
        }
    }

    /// Transition the desktop to `path`.
    ///
    /// The one command whose failure the user is watching for, so it gets the
    /// full treatment: a click while disconnected earns an immediate
    /// connection attempt rather than the tail of a backoff, a daemon-side
    /// refusal is reported in the daemon's words, and a transport failure is
    /// reported as the outage it is.
    async fn set(&mut self, proxy: &MessageProxy<WallpaperEvent>, warm: &[String], path: String) {
        if self.client.is_none() {
            self.retry_at = None;
            self.connect(proxy, warm).await;
        }
        let Some(client) = self.client.as_mut() else {
            let _ = proxy.message(WallpaperEvent::Error(SET_OFFLINE.to_owned()));
            return;
        };
        let message = crownpaper::set {
            path,
            duration: SET_DURATION,
        };
        match client.call::<crownpaper::set>(&message, Vec::new()).await {
            Ok(()) => {}
            // The daemon answered: the connection is fine, the file is not.
            Err(IpcError::Remote(refusal)) => {
                let _ = proxy.message(WallpaperEvent::Error(format!(
                    "Could not set the wallpaper: {}",
                    refusal.message
                )));
            }
            Err(_) => {
                self.lost(proxy);
                let _ = proxy.message(WallpaperEvent::Error(SET_OFFLINE.to_owned()));
            }
        }
    }
}

/// Connect to crownpaper and subscribe to its `Changed` event, as one step —
/// a client that could miss a wallpaper change is not yet a client.
async fn subscribed_client() -> Result<AsyncClient, IpcError> {
    let mut client = AsyncClient::connect(crownpaper::SERVICE)?;
    client.subscribe::<crownpaper::Changed>().await?;
    Ok(client)
}

// --- MARK: Scanning ---

/// Every offerable wallpaper on this machine, sorted and de-duplicated.
///
/// Blocking (it walks the filesystem), so [`run`] calls it via
/// `spawn_blocking`.
fn scan() -> Vec<String> {
    let mut found = Vec::new();
    for root in wallpaper_dirs() {
        collect(&root, 1, &mut found);
    }
    // Full-path sort: the user's own folder groups before the system's, and
    // the order survives rescans, so tiles never trade places on re-entry.
    found.sort();
    found.dedup();
    found
}

/// The directories worth scanning, in the order their contents should list.
fn wallpaper_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(home) = std::env::var_os("HOME") {
        dirs.push(PathBuf::from(home).join(USER_WALLPAPERS));
    }
    dirs.push(PathBuf::from(SYSTEM_WALLPAPERS));
    dirs
}

/// Gather image files under `dir`, descending into at most `subdirs_left`
/// levels of subdirectories — distributions sort `/usr/share/backgrounds` one
/// folder deep, and anything deeper is not a wallpaper collection.
///
/// A directory that cannot be read contributes nothing; on most machines at
/// least one of the roots simply does not exist, and that is not an error.
fn collect(dir: &Path, subdirs_left: u8, found: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if subdirs_left > 0 {
                collect(&path, subdirs_left - 1, found);
            }
        } else if is_wallpaper(&path)
            && let Some(text) = path.to_str()
        {
            // Paths cross the IPC boundary as strings, so a non-UTF-8 name
            // could not be asked for even if it were offered.
            found.push(text.to_owned());
        }
    }
}

/// Whether a file's extension names one of the formats crownpaper decodes.
fn is_wallpaper(path: &Path) -> bool {
    path.extension()
        .and_then(std::ffi::OsStr::to_str)
        .is_some_and(|extension| {
            WALLPAPER_EXTENSIONS
                .iter()
                .any(|known| extension.eq_ignore_ascii_case(known))
        })
}

// --- MARK: Decoding ---

/// Decode every thumbnail, one file at a time, reporting each as it lands.
///
/// Sequential on purpose: one blocking-pool thread at work keeps the grid
/// filling visibly without contending with the compositor for every core.
/// Aborting this task (a leave, or a re-enter's fresh scan) cancels between
/// files; the decode in flight finishes on the blocking pool and its result
/// is discarded, which is the cheapest correct cancellation there is.
async fn decode_all(paths: Vec<String>, proxy: MessageProxy<WallpaperEvent>) {
    for path in paths {
        let file = path.clone();
        let decoded = xilem::tokio::task::spawn_blocking(move || thumbnail(Path::new(&file))).await;
        let event = match decoded {
            Ok(Some(brush)) => WallpaperEvent::Thumbnail { path, brush },
            // Decode failed (or the decoder panicked on a malformed file):
            // either way this file must not be offered as a wallpaper.
            _ => WallpaperEvent::Unreadable { path },
        };
        if proxy.message(event).is_err() {
            return;
        }
    }
}

/// One file's grid-sized pixels, or `None` for anything undecodable.
fn thumbnail(path: &Path) -> Option<ImageBrush> {
    let decoded = image::ImageReader::open(path).ok()?.decode().ok()?;
    // `thumbnail` fits within the box preserving aspect ratio, and uses a
    // fast integer path for the large downscales wallpapers always are.
    let scaled = decoded
        .thumbnail(THUMBNAIL_EDGE, THUMBNAIL_EDGE)
        .into_rgba8();
    let (width, height) = scaled.dimensions();
    Some(ImageBrush::new(ImageData {
        data: Blob::from(scaled.into_raw()),
        format: ImageFormat::Rgba8,
        alpha_type: ImageAlphaType::Alpha,
        width,
        height,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use xilem::tokio::sync::mpsc::unbounded_channel;

    /// A 1×1 brush, for events that need one.
    fn brush() -> ImageBrush {
        ImageBrush::new(ImageData {
            data: Blob::from(vec![0_u8; 4]),
            format: ImageFormat::Rgba8,
            alpha_type: ImageAlphaType::Alpha,
            width: 1,
            height: 1,
        })
    }

    // --- MARK: Scanning ---

    #[test]
    fn only_the_daemon_formats_are_offered() {
        assert!(is_wallpaper(Path::new("/w/dunes.jpg")));
        assert!(is_wallpaper(Path::new("/w/dunes.JPEG")));
        assert!(is_wallpaper(Path::new("/w/dunes.png")));
        assert!(is_wallpaper(Path::new("/w/dunes.webp")));
        assert!(is_wallpaper(Path::new("/w/dunes.bmp")));
        assert!(
            !is_wallpaper(Path::new("/w/dunes.gif")),
            "crownpaper cannot decode it"
        );
        assert!(!is_wallpaper(Path::new("/w/dunes.jpg.part")));
        assert!(!is_wallpaper(Path::new("/w/dunes")));
    }

    // --- MARK: The lifecycle ---

    #[test]
    fn the_picker_only_pays_while_it_is_open() {
        let (sender, mut receiver) = unbounded_channel();
        let mut state = WallpaperState::default();
        state.attach(sender);

        state.enter();
        assert_eq!(receiver.try_recv().ok(), Some(WallpaperCommand::Enter));
        state.enter();
        assert!(
            receiver.try_recv().is_err(),
            "a second enter is not a rescan"
        );

        state.apply(WallpaperEvent::Found(vec![
            "/w/a.png".into(),
            "/w/b.png".into(),
        ]));
        assert!(state.scanned);
        assert_eq!(state.entries.len(), 2);

        state.apply(WallpaperEvent::Thumbnail {
            path: "/w/b.png".into(),
            brush: brush(),
        });
        assert!(state.entries[1].thumbnail.is_some());
        assert!(state.entries[0].thumbnail.is_none());

        state.leave();
        assert_eq!(receiver.try_recv().ok(), Some(WallpaperCommand::Leave));
        assert!(
            state.entries.is_empty(),
            "leaving is when the memory goes back"
        );
        assert!(!state.scanned);
        state.leave();
        assert!(
            receiver.try_recv().is_err(),
            "a second leave has nothing to release"
        );
    }

    #[test]
    fn a_scan_finishing_after_leave_changes_nothing() {
        let mut state = WallpaperState::default();
        state.enter();
        state.leave();

        state.apply(WallpaperEvent::Found(vec!["/w/late.png".into()]));
        assert!(
            state.entries.is_empty() && !state.scanned,
            "the grid it describes no longer exists"
        );
    }

    #[test]
    fn an_unreadable_file_is_withdrawn_from_the_grid() {
        let mut state = WallpaperState::default();
        state.enter();
        state.apply(WallpaperEvent::Found(vec![
            "/w/ok.png".into(),
            "/w/bad.png".into(),
        ]));

        state.apply(WallpaperEvent::Unreadable {
            path: "/w/bad.png".into(),
        });
        assert_eq!(state.entries.len(), 1);
        assert_eq!(state.entries[0].path, "/w/ok.png");
    }

    #[test]
    fn an_enter_before_the_worker_exists_is_replayed_at_attach() {
        let mut state = WallpaperState::default();
        state.enter(); // Nowhere to send it yet.

        let (sender, mut receiver) = unbounded_channel();
        state.attach(sender);
        assert_eq!(
            receiver.try_recv().ok(),
            Some(WallpaperCommand::Enter),
            "the window can open straight onto the picker"
        );
    }

    // --- MARK: Event folding ---

    #[test]
    fn the_daemon_events_land_where_the_page_reads() {
        let mut state = WallpaperState::default();
        assert!(!state.online, "honest before the first connect");

        state.apply(WallpaperEvent::Online);
        assert!(state.online);
        state.apply(WallpaperEvent::Current("/w/dunes.png".into()));
        assert_eq!(state.current.as_deref(), Some("/w/dunes.png"));

        state.apply(WallpaperEvent::Error("no such file".into()));
        assert_eq!(state.error.as_deref(), Some("no such file"));
        state.dismiss_error();
        assert!(state.error.is_none());

        state.apply(WallpaperEvent::Offline);
        assert!(!state.online);
    }

    // --- MARK: The command channel ---

    #[test]
    fn a_closed_channel_is_dropped_rather_than_retried() {
        let (sender, receiver) = unbounded_channel::<WallpaperCommand>();
        let mut state = WallpaperState::default();
        state.attach(sender);
        drop(receiver);

        state.send(WallpaperCommand::Enter);
        assert!(
            state.sender.is_none(),
            "a send to a gone worker retires the channel"
        );
        // And a second send must still not panic.
        state.send(WallpaperCommand::Leave);
    }
}
