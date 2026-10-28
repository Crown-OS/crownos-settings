//! Live system state, as opposed to configured state.
//!
//! Everything under [`crate::config`] mirrors a RON file: the user asked for it,
//! it survives a reboot, and the app is free to believe it. Everything under
//! this module is the opposite — it is whatever the machine happens to be doing
//! right now, it is discovered rather than declared, and it is never written to
//! disk by this app.
//!
//! The two are deliberately kept apart. `wifi.ron` saying `enabled: true` is a
//! *preference*; NetworkManager saying the radio is up is a *fact*, and when
//! they disagree the fact wins. A page that reads the fact and a page that reads
//! the preference should not be able to confuse the two, so they come from
//! different types: [`crate::config::ConfigStore`] for the preference,
//! [`wifi::WifiState`] for the fact. [`crate::state::Store`] is the one place
//! that holds both.
//!
//! ## Shape of a backend
//!
//! Each submodule here follows the same three-part shape, the one
//! [`spawn_worker`](crate::util::worker::spawn_worker) is built around:
//!
//! * a **command** enum — everything the UI can ask the subsystem to do,
//! * an **event** enum — everything the subsystem can tell the UI, including a
//!   UI-ready snapshot type that no backend crate's types leak into,
//! * a **state** struct, held in a `Signal` in the [`Store`](crate::state::Store),
//!   that holds the last event's worth of information and owns the channel
//!   that commands travel down.
//!
//! The actual I/O happens in one long-lived future, spawned once by the root
//! component and cancelled with it, exactly like the config watchers. Nothing
//! above this module ever awaits anything, holds a lock, or names a backend
//! crate's type — D-Bus for Wi-Fi, `crownos-ipc` for the wallpaper daemon.

//! Not every fact needs that much machinery, mind: [`audio`] is a list of
//! device names that is read once and never commanded, so it is a function
//! rather than a worker. Its module docs say why.

pub mod audio;
pub mod crownconnect;
pub mod outputs;
pub mod plugins;
pub mod wallpaper;
pub mod wifi;
