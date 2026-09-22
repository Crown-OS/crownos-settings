//! The two kinds of state a page can see, joined into one.
//!
//! Up to now a page was handed a [`ConfigStore`] and that was the whole world:
//! seven RON files, every value declared by the user, nothing to discover. Wi-Fi
//! breaks that, because "which networks are in range" is not a setting — it is a
//! fact about the room, it changes while the window is open, and writing it to
//! `~/.config/crownos/wifi.ron` would be a lie.
//!
//! So there are two halves now, and [`Store`] is both of them:
//!
//! * [`ConfigStore`] — declared, persisted, authoritative because the user said
//!   so. Kept private here, exactly as it keeps its own sections private, and
//!   reached only through the delegating [`get`](Store::get) /
//!   [`set`](Store::set) / [`update`](Store::update) / [`section`](Store::section)
//!   methods so that "mutate without persisting" stays unspeakable.
//! * [`WifiState`] — observed, transient, authoritative because the hardware
//!   said so. A public field, because it has no such invariant to protect: a
//!   page reads its snapshot and pushes commands at it, and nothing needs
//!   saving. [`WallpaperState`] is the same kind of thing, with the crownpaper
//!   daemon standing where the hardware stands.
//!
//! Pages and controls are generic over neither — they name `Store` — so the
//! split costs a page exactly one decision: `store.get(some::Key)` for a
//! preference, `store.wifi` for a fact.
//!
//! ## Who owns the seam
//!
//! When the two halves disagree, the fact wins and the preference is corrected:
//! [`mirror_wifi`](Store::mirror_wifi) writes NetworkManager's radio state and
//! current SSID back into `wifi.ron` every time a snapshot arrives. That is not
//! this app talking to itself — it is how the CrownOS bar and every other app
//! watching that file learn what the network is doing, without each of them
//! opening its own D-Bus connection.
//!
//! It is cheap because [`ConfigStore::set`] already drops writes that change
//! nothing, and the overwhelming majority of snapshots change neither field.

use crownos_config::Key;
use crownos_config::schema::{appearance, wifi};

use crate::config::{ConfigStore, Section};
use crate::net::outputs::OutputState;
use crate::net::wallpaper::{WallpaperEvent, WallpaperState};
use crate::net::wifi::{WifiEvent, WifiSnapshot, WifiState};

/// Everything a page is allowed to see: the settings on disk, and the network
/// as it actually is.
///
/// This is the state every [`crate::pages`] builder and every
/// [`crate::controls`] widget is written against, and — through
/// [`xilem::core::lens`] in [`crate::app`] — all they can reach. Navigation,
/// window chrome and the page registry are deliberately not in here.
pub struct Store {
    /// Private for the same reason [`ConfigStore`]'s own fields are: every
    /// write has to go through something that persists it.
    config: ConfigStore,
    /// Public because there is nothing to protect — it is a mirror of the
    /// world, and the page both reads it and posts commands into it.
    pub wifi: WifiState,
    /// The other observed half: the crownpaper daemon and the thumbnails the
    /// Appearance page's picker draws. Public for the same reason `wifi` is.
    pub wallpaper: WallpaperState,
    /// The monitors the compositor is driving, and the edits not yet applied
    /// to them. A fact like the other two — the compositor owns the
    /// arrangement and persists it, so nothing here is written to a RON file.
    pub outputs: OutputState,
    /// The microphones this machine has, and which one "Automatic" currently
    /// resolves to.
    ///
    /// The third fact, and the only one that never changes while the window is
    /// open — see [`crate::net::audio`] for why it is read once instead of
    /// watched. Held here rather than being read by the Input page because a
    /// page builder runs on every rebuild, and enumerating an audio host on
    /// every frame of a slide transition would be absurd.
    pub microphones: Microphones,
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
    /// Load the settings off disk and start with an empty view of the network.
    ///
    /// "Empty" is the honest starting point: the worker has not run yet, so
    /// `wifi.snapshot` is `None` and the page draws its looking-for-networks
    /// state until the first [`WifiEvent::Snapshot`] lands.
    pub fn load() -> Self {
        Self {
            config: ConfigStore::load(),
            wifi: WifiState::default(),
            wallpaper: WallpaperState::default(),
            outputs: OutputState::default(),
            microphones: Microphones::probe(),
        }
    }

    /// A store of nothing but defaults, for tests that must not touch the
    /// filesystem — or NetworkManager, or the audio host.
    #[cfg(test)]
    pub fn defaults() -> Self {
        Self {
            config: ConfigStore::defaults(),
            wifi: WifiState::default(),
            wallpaper: WallpaperState::default(),
            outputs: OutputState::default(),
            microphones: Microphones::default(),
        }
    }

    // --- MARK: Config delegation ---

    /// The current value of one section. See [`ConfigStore::section`].
    pub fn section<S: Section>(&self) -> &S {
        self.config.section::<S>()
    }

    /// The current value of one key. See [`ConfigStore::get`].
    pub fn get<K: Key>(&self, key: K) -> K::Value
    where
        K::Section: Section,
    {
        self.config.get(key)
    }

    /// Write one key and persist its section. See [`ConfigStore::set`].
    pub fn set<K: Key>(&mut self, key: K, value: K::Value)
    where
        K::Section: Section,
    {
        self.config.set(key, value);
    }

    /// Edit a section and persist it. See [`ConfigStore::update`].
    ///
    /// Delegated for completeness — every page so far binds single keys, so
    /// this is the multi-field escape hatch nothing has needed yet.
    #[allow(dead_code)]
    pub fn update<S: Section>(&mut self, edit: impl FnOnce(&mut S)) {
        self.config.update(edit);
    }

    /// The config half, for the watchers that adopt external edits.
    ///
    /// The one hole in the "private field" story, and a deliberate one:
    /// [`ConfigStore::watchers`] needs a projection down to the store it fills,
    /// and it can only be handed a `fn(&mut State) -> &mut ConfigStore`.
    pub fn config_mut(&mut self) -> &mut ConfigStore {
        &mut self.config
    }

    // --- MARK: The seam ---

    /// Copy what NetworkManager just said into `wifi.ron`.
    ///
    /// Runs on the main thread, once per event, alongside
    /// [`WifiState`]'s own handling — see [`crate::net::wifi::wifi_worker`].
    /// Only snapshots carry the two facts worth mirroring; a `Connecting` or an
    /// error is this window's business alone.
    ///
    /// Note the direction. Every *other* control in this app writes the config
    /// and expects the system to follow. Here the system leads and the config
    /// follows, because the radio can be turned off by a laptop's function key,
    /// by `nmcli`, or by another app, and none of those go through us.
    pub fn mirror_wifi(&mut self, event: &WifiEvent) {
        let WifiEvent::Snapshot(snapshot) = event else {
            return;
        };
        let (enabled, network) = mirrored_wifi(snapshot);
        self.config.set(wifi::Enabled, enabled);
        self.config.set(wifi::Network, network);
    }

    /// Copy what crownpaper just said into `appearance.ron`.
    ///
    /// The same seam as [`mirror_wifi`](Self::mirror_wifi), for the same
    /// reason: the wallpaper can be changed by `crownpaper set` in a terminal
    /// or by any other app, and none of those go through us — so the daemon's
    /// `Changed` leads and the config follows. Only that one event carries a
    /// fact worth persisting; everything else (thumbnails, connectivity,
    /// errors) is this window's business alone.
    pub fn mirror_wallpaper(&mut self, event: &WallpaperEvent) {
        let WallpaperEvent::Current(path) = event else {
            return;
        };
        self.config.set(appearance::Wallpaper, path.clone());
    }
}

/// What `wifi.ron`'s two fields should say, given a snapshot.
///
/// Split out from [`Store::mirror_wifi`] so the rule can be asserted without a
/// store — writing through a real [`ConfigStore`] means writing a real file, and
/// a unit test has no business doing that to the machine it runs on.
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
    use crate::net::wifi::{ActiveWifi, LinkState, WifiSnapshot};
    use crownos_config::schema::Wifi;

    /// Nothing in here is allowed to make [`ConfigStore`] persist: a write goes
    /// to a real RON file under `$CROWN_CONFIG_DIR` (or the developer's own
    /// `~/.config/crownos` when that isn't set), and the one test in
    /// [`crate::config`] that legitimately needs a config dir owns that
    /// process-global variable for its duration. So the rule is tested pure,
    /// through [`mirrored_wifi`], and the store is only used for assertions
    /// that provably change nothing.
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
        let mut store = Store::defaults();

        store.mirror_wifi(&WifiEvent::Connecting("Attic".to_owned()));
        store.mirror_wifi(&WifiEvent::Error("nope".to_owned()));
        store.mirror_wifi(&WifiEvent::Unavailable("gone".to_owned()));

        assert_eq!(
            store.section::<Wifi>(),
            &Wifi::default(),
            "a transient event is this window's business alone"
        );
    }

    #[test]
    fn only_the_daemons_current_wallpaper_is_mirrored() {
        use crownos_config::schema::Appearance;

        let mut store = Store::defaults();

        // Same rule as the Wi-Fi seam: these change nothing, and — because
        // `ConfigStore::set` skips no-op writes — provably touch no file.
        store.mirror_wallpaper(&WallpaperEvent::Online);
        store.mirror_wallpaper(&WallpaperEvent::Offline);
        store.mirror_wallpaper(&WallpaperEvent::Error("nope".to_owned()));
        store.mirror_wallpaper(&WallpaperEvent::Found(vec!["/w/dunes.png".to_owned()]));

        assert_eq!(
            store.section::<Appearance>(),
            &Appearance::default(),
            "a transient event is this window's business alone"
        );
    }

    #[test]
    fn the_delegated_reads_see_the_same_store() {
        let store = Store::defaults();
        let section = store.section::<Wifi>();

        assert_eq!(store.get(wifi::Enabled), section.enabled);
        assert_eq!(store.get(wifi::Network), section.network);
    }
}
