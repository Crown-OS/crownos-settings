//! Typed, self-persisting access to the on-disk settings sections.
//!
//! [`crownos_config`] speaks in `(section name, value)` pairs and leaves two things
//! to its caller: remembering which name goes with which type, and remembering
//! to save after every edit. This module ties both down.
//!
//! * [`Section`] pins a [`crownos_config::schema`] struct to its section name *and*
//!   to its signal in [`Settings`], which is what lets everything above this
//!   module be generic over "some section" instead of naming all nine.
//! * [`Settings::set`] and [`Settings::update`] are the only ways to change a
//!   value, and they persist what they changed. "Mutate the field but forget to
//!   write the file" stops being a mistake a caller is able to make.
//!
//! Each section is one `Signal`, so a control bound to `display::Brightness`
//! re-reads when `display.ron` changes and never when `sound.ron` does.
//!
//! Adding a section is one line in the [`sections!`] invocation at the bottom of
//! this file: the signal, the startup load, the [`Section`] impl and the
//! filesystem watcher are all generated from it.

use std::fmt::Debug;

use crownconfig::{SectionWatch, watch_section};
use crownos_config::Key;
use crownos_config::schema::{
    Appearance, Bluetooth, Display, Glass, Input, Keybinds, Lock, Notifications, Power, Screenshot,
    Sound, Theme, Wifi,
};
use crownui::prelude::{Cx, Runtime, Signal};
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::util::persist;

// --- MARK: Section ---

/// One settings section: a schema struct stored in exactly one RON file, plus
/// the knowledge of which [`Settings`] signal holds it.
pub trait Section:
    Default + Clone + PartialEq + Debug + Serialize + DeserializeOwned + Send + Sync + 'static
{
    /// The section name, i.e. the file is `<config dir>/<NAME>.ron`.
    const NAME: &'static str;

    /// This section's signal.
    fn slot(settings: Settings) -> Signal<Self>;
}

// --- MARK: Settings ---

impl Settings {
    /// Reads `S` and subscribes the running binding to it.
    pub fn section<S: Section, R>(self, runtime: &mut Runtime, read: impl FnOnce(&S) -> R) -> R {
        S::slot(self).with(runtime, read)
    }

    /// The current value of one key, subscribing the running binding to its
    /// section.
    pub fn get<K: Key>(self, runtime: &mut Runtime, _key: K) -> K::Value
    where
        K::Section: Section,
    {
        <K::Section as Section>::slot(self).with(runtime, K::get)
    }

    /// Write one key and persist the section it belongs to.
    pub fn set<K: Key>(self, runtime: &mut Runtime, _key: K, value: K::Value)
    where
        K::Section: Section,
    {
        self.update::<K::Section>(runtime, |section| K::set(section, value));
    }

    /// Edit a section and queue it for disk — there is no "Apply" step in this
    /// app, so the RON file *is* the state.
    ///
    /// A write that changes nothing is dropped: sliders emit a value on every
    /// pointer move, and re-serialising an unchanged section would wake every
    /// other app's watcher for nothing.
    pub fn update<S: Section>(self, runtime: &mut Runtime, edit: impl FnOnce(&mut S)) {
        let slot = S::slot(self);
        let mut next = slot.with_untracked(runtime, Clone::clone);
        edit(&mut next);
        if slot.with_untracked(runtime, |current| *current != next) {
            persist::save(S::NAME, next.clone());
            slot.set(runtime, next);
        }
    }

    /// Take a value that *came from* disk, without writing it back.
    fn adopt<S: Section>(slot: Signal<S>, runtime: &mut Runtime, value: S) {
        if slot.with_untracked(runtime, |current| *current != value) {
            slot.set(runtime, value);
        }
    }
}

/// Adopts every external edit of one section for as long as the calling
/// component lives.
fn follow<S: Section>(cx: &mut Cx, slot: Signal<S>, mut watch: SectionWatch<S>) {
    let next_edit = async move {
        let value = watch.changed().await;
        (watch, value)
    };
    cx.spawn(next_edit, move |cx, (watch, value)| {
        Settings::adopt(slot, cx, value);
        follow(cx, slot, watch);
    });
}

// --- MARK: Registration ---

/// Declares the app's config sections: [`Settings`] itself, its startup load,
/// one [`Section`] impl per entry, and the watchers that keep the signals in
/// step with the files.
macro_rules! sections {
    ($($field:ident : $ty:ty),+ $(,)?) => {
        /// Every settings section, one signal per RON file, loaded once at
        /// startup and kept in sync with disk from then on.
        ///
        /// Fields are private: reads go through [`Settings::get`] and writes
        /// through [`Settings::set`], so no caller can edit a section without
        /// persisting it.
        #[derive(Debug, Clone, Copy, PartialEq)]
        pub struct Settings {
            $($field: Signal<$ty>,)+
        }

        impl Settings {
            /// Read every section off disk.
            ///
            /// [`crownos_config::load`] materialises the default file when a
            /// section has never been written, so a fresh install ends up with
            /// all of these RON files on disk.
            pub fn load(runtime: &mut Runtime) -> Self {
                Self {
                    $($field: runtime.signal(crownos_config::load::<$ty>(<$ty as Section>::NAME)),)+
                }
            }

            /// Every section at its defaults, for tests that must not touch
            /// the filesystem.
            #[cfg(test)]
            pub fn defaults(runtime: &mut Runtime) -> Self {
                Self {
                    $($field: runtime.signal(<$ty as Default>::default()),)+
                }
            }

            /// Adopt external edits to every section while the calling
            /// component is mounted.
            pub fn watch(self, cx: &mut Cx) {
                $(
                    let initial = self.$field.with_untracked(cx, Clone::clone);
                    follow(cx, self.$field, watch_section(<$ty as Section>::NAME, initial));
                )+
            }
        }

        $(
            impl Section for $ty {
                const NAME: &'static str = <$ty>::SECTION;

                fn slot(settings: Settings) -> Signal<Self> {
                    settings.$field
                }
            }
        )+
    };
}

sections! {
    wifi: Wifi,
    bluetooth: Bluetooth,
    appearance: Appearance,
    glass: Glass,
    display: Display,
    sound: Sound,
    input: Input,
    keybinds: Keybinds,
    notifications: Notifications,
    power: Power,
    lock: Lock,
    theme: Theme,
    screenshot: Screenshot,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crownos_config::schema::wifi;

    /// Something that is definitely not RON, used to prove a write *didn't*
    /// happen: if the store had saved, this would be gone.
    const SENTINEL: &[u8] = b"not RON, and must survive";

    /// One test function, because `CROWN_CONFIG_DIR` is process-global and cargo
    /// runs test functions on parallel threads.
    #[test]
    fn settings_read_write_and_skip_no_op_saves() {
        let dir = std::env::temp_dir().join(format!("crownsettings-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create temp config dir");
        // SAFETY: single-threaded section of a single test function; no other
        // test in this binary touches the environment.
        unsafe { std::env::set_var(crownos_config::CONFIG_DIR_ENV, &dir) };

        let wifi_path = crownos_config::path_for(Wifi::NAME);
        let mut runtime = Runtime::new();
        let runtime = &mut runtime;

        // --- every section is materialised on first load -------------------
        let settings = Settings::load(runtime);
        for section in [
            Wifi::NAME,
            Bluetooth::NAME,
            Appearance::NAME,
            Display::NAME,
            Sound::NAME,
            Input::NAME,
            Keybinds::NAME,
            Notifications::NAME,
            Power::NAME,
        ] {
            assert!(
                crownos_config::path_for(section).exists(),
                "load() should materialise {section}.ron"
            );
        }
        assert_eq!(settings.section(runtime, Wifi::clone), Wifi::default());

        // --- a write reaches the interface at once and the disk later ------
        settings.update::<Wifi>(runtime, |wifi| wifi.network = Some("Workshop".to_owned()));
        assert_eq!(
            settings.get(runtime, wifi::Network).as_deref(),
            Some("Workshop")
        );
        assert_eq!(
            crownos_config::load::<Wifi>(Wifi::NAME).network,
            None,
            "the write is queued, not made, inside the callback"
        );
        persist::flush();
        assert_eq!(
            crownos_config::load::<Wifi>(Wifi::NAME).network.as_deref(),
            Some("Workshop"),
            "update() must persist the edit"
        );

        // --- a key round-trips through the signal and the file -------------
        settings.set(runtime, wifi::Enabled, false);
        assert!(!settings.get(runtime, wifi::Enabled));
        persist::flush();
        assert!(
            !crownos_config::load::<Wifi>(Wifi::NAME).enabled,
            "set() must persist the edit"
        );
        assert_eq!(
            settings.get(runtime, wifi::Network).as_deref(),
            Some("Workshop"),
            "setting one key must leave its neighbours alone"
        );
        settings.set(runtime, wifi::Enabled, true);

        // --- a key write that changes nothing doesn't touch the file -------
        persist::flush();
        std::fs::write(&wifi_path, SENTINEL).expect("plant sentinel");
        settings.set(runtime, wifi::Enabled, true);
        persist::flush();
        assert_eq!(
            std::fs::read(&wifi_path).ok().as_deref(),
            Some(SENTINEL),
            "a no-op set must not rewrite the file"
        );

        // --- adopt takes a value that is already on disk, silently ---------
        Settings::adopt(
            Wifi::slot(settings),
            runtime,
            Wifi {
                enabled: true,
                network: None,
            },
        );
        persist::flush();
        assert_eq!(settings.section(runtime, Wifi::clone), Wifi::default());
        assert_eq!(
            std::fs::read(&wifi_path).ok().as_deref(),
            Some(SENTINEL),
            "adopt() must not write back a value that came from disk"
        );

        // --- sections are independent -------------------------------------
        settings.update::<Sound>(runtime, |sound| sound.output_volume = 77.0);
        persist::flush();
        assert_eq!(
            crownos_config::load::<Sound>(Sound::NAME).output_volume,
            77.0
        );
        assert_eq!(
            settings.section(runtime, Display::clone),
            Display::default(),
            "editing one section must not disturb another"
        );

        // SAFETY: as above.
        unsafe { std::env::remove_var(crownos_config::CONFIG_DIR_ENV) };
        let _ = std::fs::remove_dir_all(&dir);
    }
}
