//! The two kinds of state a page can see, joined into one.
//!
//! * [`Settings`] — declared, persisted, authoritative because the user said
//!   so. Kept private here and reached only through the delegating
//!   [`get`](Store::get) / [`set`](Store::set) / [`section`](Store::section)
//!   methods, so that "mutate without persisting" stays unspeakable.
//! * The backends' states — observed, transient, authoritative because the
//!   hardware (or a daemon) said so. Public signals, because there is nothing
//!   to protect: a page reads the snapshot and pushes commands at it, and
//!   nothing needs saving.
//!
//! Every field is a `Copy` handle into the reactive runtime, so a [`Store`] is
//! passed around by value and a binding that reads one fact re-runs when that
//! fact changes, and only then.
//!
//! ## Who owns the seam
//!
//! When the two halves disagree, the fact wins and the preference is corrected:
//! [`mirror_wifi`](Store::mirror_wifi) writes NetworkManager's radio state and
//! current SSID back into `wifi.ron` every time a snapshot arrives. That is how
//! the CrownOS bar and every other app watching that file learn what the
//! network is doing, without each of them opening its own D-Bus connection.
//! It is cheap because [`Settings::set`] drops writes that change nothing.

use crownos_config::Key;
use crownos_config::schema::{appearance, wifi};
use crownui::prelude::{Cx, Runtime, Signal};

use crate::config::{Section, Settings};
use crate::net::crownconnect::{CrossDeviceState, crownconnect_worker};
use crate::net::outputs::{OutputState, output_worker};
use crate::net::plugins::{PluginsState, plugins_worker};
use crate::net::wallpaper::{WallpaperEvent, WallpaperState, wallpaper_worker};
use crate::net::wifi::{WifiEvent, WifiSnapshot, WifiState, wifi_worker};

/// Everything a page is allowed to see: the settings on disk, and the system
/// as it actually is. Navigation and window chrome are deliberately not in
/// here.
#[derive(Debug, Clone, Copy)]
pub struct Store {
    /// Private: every write has to go through something that persists it.
    config: Settings,
    pub wifi: Signal<WifiState>,
    /// The crownpaper daemon and the thumbnails the Appearance page draws.
    pub wallpaper: Signal<WallpaperState>,
    /// The monitors the compositor is driving, and the edits not yet applied
    /// to them. The compositor persists the arrangement, so nothing here is
    /// written to a RON file.
    pub outputs: Signal<OutputState>,
    /// The devices crownconnect pairs with, and the pairing flow in progress.
    pub cross_device: Signal<CrossDeviceState>,
    /// The themes and plugins crownplugind runs, and their settings.
    pub plugins: Signal<PluginsState>,
    /// The microphones this machine has — read once, see
    /// [`crate::net::audio`] for why it is not watched.
    pub microphones: Signal<Microphones>,
}

/// What is plugged in, as the Input page's device picker needs it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Microphones {
    /// Every input device, by the name `cpal` gives it.
    pub names: Vec<String>,
    /// The name the system default currently resolves to, for labelling the
    /// "Automatic" choice with what it actually means right now.
    pub default: Option<String>,
}

impl Microphones {
    /// Ask the audio host. See [`crate::net::audio`].
    fn probe() -> Self {
        Self {
            names: crate::net::audio::microphones(),
            default: crate::net::audio::default_microphone(),
        }
    }
}

impl Store {
    /// Load the settings off disk and start with an empty view of the system:
    /// no worker has run yet, so every page starts in its "looking…" state.
    pub fn load(runtime: &mut Runtime) -> Self {
        let config = Settings::load(runtime);
        Self::with(runtime, config, Microphones::probe())
    }

    /// A store of nothing but defaults, for tests that must not touch the
    /// filesystem — or NetworkManager, or the audio host.
    #[cfg(test)]
    pub fn defaults(runtime: &mut Runtime) -> Self {
        let config = Settings::defaults(runtime);
        Self::with(runtime, config, Microphones::default())
    }

    fn with(runtime: &mut Runtime, config: Settings, microphones: Microphones) -> Self {
        Self {
            config,
            wifi: runtime.signal(WifiState::default()),
            wallpaper: runtime.signal(WallpaperState::default()),
            outputs: runtime.signal(OutputState::default()),
            cross_device: runtime.signal(CrossDeviceState::default()),
            plugins: runtime.signal(PluginsState::default()),
            microphones: runtime.signal(microphones),
        }
    }

    /// Start following the config files and every backend, for as long as the
    /// calling component lives.
    pub fn start(self, cx: &mut Cx) {
        self.config.watch(cx);
        wifi_worker(cx, self.wifi, move |cx, event| self.mirror_wifi(cx, event));
        wallpaper_worker(cx, self.wallpaper, move |cx, event| {
            self.mirror_wallpaper(cx, event);
        });
        output_worker(cx, self.outputs);
        crownconnect_worker(cx, self.cross_device);
        plugins_worker(cx, self.plugins);
    }

    // --- MARK: Config delegation ---

    /// Reads one section. See [`Settings::section`].
    pub fn section<S: Section, R>(self, runtime: &mut Runtime, read: impl FnOnce(&S) -> R) -> R {
        self.config.section(runtime, read)
    }

    /// The current value of one key. See [`Settings::get`].
    pub fn get<K: Key>(self, runtime: &mut Runtime, key: K) -> K::Value
    where
        K::Section: Section,
    {
        self.config.get(runtime, key)
    }

    /// Write one key and persist its section. See [`Settings::set`].
    pub fn set<K: Key>(self, runtime: &mut Runtime, key: K, value: K::Value)
    where
        K::Section: Section,
    {
        self.config.set(runtime, key, value);
    }

    // --- MARK: The seam ---

    /// Copy what NetworkManager just said into `wifi.ron`.
    ///
    /// Note the direction. Every *other* control in this app writes the config
    /// and expects the system to follow. Here the system leads and the config
    /// follows, because the radio can be turned off by a laptop's function key,
    /// by `nmcli`, or by another app, and none of those go through us.
    pub fn mirror_wifi(self, runtime: &mut Runtime, event: &WifiEvent) {
        let WifiEvent::Snapshot(snapshot) = event else {
            return;
        };
        let (enabled, network) = mirrored_wifi(snapshot);
        self.config.set(runtime, wifi::Enabled, enabled);
        self.config.set(runtime, wifi::Network, network);
    }

    /// Copy what crownpaper just said into `appearance.ron` — the same seam as
    /// [`mirror_wifi`](Self::mirror_wifi), for the same reason: the wallpaper
    /// can be changed by `crownpaper set` in a terminal, and none of those go
    /// through us.
    pub fn mirror_wallpaper(self, runtime: &mut Runtime, event: &WallpaperEvent) {
        let WallpaperEvent::Current(path) = event else {
            return;
        };
        self.config
            .set(runtime, appearance::Wallpaper, path.clone());
    }
}

/// What `wifi.ron`'s two fields should say, given a snapshot.
///
/// Split out from [`Store::mirror_wifi`] so the rule can be asserted without
/// writing a real file.
fn mirrored_wifi(snapshot: &WifiSnapshot) -> (bool, Option<String>) {
    let network = snapshot
        .connection
        .as_ref()
        .map(|current| current.ssid.clone());
    (snapshot.radio_enabled, network)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::wifi::{ActiveWifi, LinkState};
    use crownos_config::schema::{Appearance, Wifi};

    fn joined(ssid: &str) -> WifiSnapshot {
        WifiSnapshot {
            radio_enabled: true,
            present: true,
            connection: Some(ActiveWifi {
                ssid: ssid.to_owned(),
                state: LinkState::Connected,
                strength: Some(64),
                ip4_address: Some("192.168.1.20/24".to_owned()),
                weak_security: false,
            }),
            ..WifiSnapshot::default()
        }
    }

    #[test]
    fn a_snapshot_dictates_both_config_fields() {
        assert_eq!(
            mirrored_wifi(&joined("Workshop")),
            (true, Some("Workshop".to_owned()))
        );
        assert_eq!(
            mirrored_wifi(&WifiSnapshot::default()),
            (false, None),
            "radio off means no network, and the mirror must say both"
        );
        assert_eq!(
            mirrored_wifi(&WifiSnapshot {
                radio_enabled: true,
                present: true,
                ..WifiSnapshot::default()
            }),
            (true, None),
            "radio on but unjoined clears the stored SSID"
        );
    }

    #[test]
    fn only_snapshots_are_mirrored() {
        let mut runtime = Runtime::new();
        let store = Store::defaults(&mut runtime);

        store.mirror_wifi(&mut runtime, &WifiEvent::Connecting("Attic".to_owned()));
        store.mirror_wifi(&mut runtime, &WifiEvent::Error("nope".to_owned()));
        store.mirror_wifi(&mut runtime, &WifiEvent::Unavailable("gone".to_owned()));

        assert_eq!(
            store.section(&mut runtime, Wifi::clone),
            Wifi::default(),
            "a transient event is this window's business alone"
        );
    }

    #[test]
    fn only_the_daemons_current_wallpaper_is_mirrored() {
        let mut runtime = Runtime::new();
        let store = Store::defaults(&mut runtime);

        store.mirror_wallpaper(&mut runtime, &WallpaperEvent::Online);
        store.mirror_wallpaper(&mut runtime, &WallpaperEvent::Offline);
        store.mirror_wallpaper(&mut runtime, &WallpaperEvent::Error("nope".to_owned()));
        store.mirror_wallpaper(
            &mut runtime,
            &WallpaperEvent::Found(vec!["/w/dunes.png".to_owned()]),
        );

        assert_eq!(
            store.section(&mut runtime, Appearance::clone),
            Appearance::default(),
            "a transient event is this window's business alone"
        );
    }

    #[test]
    fn the_delegated_reads_see_the_same_settings() {
        let mut runtime = Runtime::new();
        let store = Store::defaults(&mut runtime);
        let section = store.section(&mut runtime, Wifi::clone);

        assert_eq!(store.get(&mut runtime, wifi::Enabled), section.enabled);
        assert_eq!(store.get(&mut runtime, wifi::Network), section.network);
    }
}
