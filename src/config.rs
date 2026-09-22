//! Typed, self-persisting access to the on-disk settings sections.
//!
//! [`crownos_config`] speaks in `(section name, value)` pairs and leaves two things
//! to its caller: remembering which name goes with which type, and remembering
//! to save after every edit. This module ties both down.
//!
//! * [`Section`] pins a [`crownos_config::schema`] struct to its section name *and*
//!   to its slot in [`ConfigStore`], which is what lets everything above this
//!   module be generic over "some section" instead of naming all seven.
//! * [`ConfigStore::set`] and [`ConfigStore::update`] are the only ways to change
//!   a value, and they persist what they changed. "Mutate the field but forget to
//!   write the file" stops being a mistake a caller is able to make.
//!
//! Adding a section is one line in the [`sections!`] invocation at the bottom of
//! this file: the store field, the startup load, the [`Section`] impl and the
//! filesystem watcher are all generated from it.
//!
//! # Sections and keys
//!
//! Both granularities are in play, and they answer different questions:
//!
//! * A **section** is a file. The store mirrors whole sections, so that is what
//!   [`load`](ConfigStore::load) reads, what [`persist`](ConfigStore::persist)
//!   writes, and what the watchers adopt.
//! * A **key** is a field, named by a [`Key`] type that `crownos-config`
//!   generates from the schema. That is the unit a *control* binds to, through
//!   [`ConfigStore::get`] and [`ConfigStore::set`] — a toggle is bound to
//!   `wifi::Enabled`, not to "the `Wifi` section, and by the way the field is
//!   called `enabled`".

use std::fmt::Debug;

use crownos_config::Key;
use crownos_config::schema::{
    Appearance, Bluetooth, Display, Input, Keybinds, Notifications, Power, Sound, Wifi,
};
use crownos_config::xilem_view::watch;
use serde::Serialize;
use serde::de::DeserializeOwned;
use xilem::ViewCtx;
use xilem::core::{NoElement, ViewSequence};

// --- MARK: Section ---

/// One settings section: a schema struct stored in exactly one RON file, plus
/// the knowledge of where it lives inside a [`ConfigStore`].
///
/// The two accessors are the interesting part. `crownos_config`'s schema types
/// carry a `SECTION` constant but no trait, so code that wanted to touch "the
/// section this control belongs to" had to name a concrete field. Implementing
/// `Section` turns that field access into something generic code can ask for,
/// which is what collapses seven near-identical `save_*` methods into one
/// [`ConfigStore::update`].
///
/// The supertraits are the union of what [`crownos_config::load`], [`crownos_config::save`]
/// and [`watch`] need, so a `Section` can do all three.
pub trait Section:
    Default + Clone + PartialEq + Debug + Serialize + DeserializeOwned + Send + Sync + 'static
{
    /// The section name, i.e. the file is `<config dir>/<NAME>.ron`.
    const NAME: &'static str;

    /// This section's slot in the store.
    fn slot(store: &ConfigStore) -> &Self;

    /// This section's slot in the store, mutably.
    ///
    /// Deliberately not public API for callers: reach a section through
    /// [`ConfigStore::update`] instead, which also writes it back to disk.
    fn slot_mut(store: &mut ConfigStore) -> &mut Self;
}

// --- MARK: Store ---

impl ConfigStore {
    /// The current value of one section.
    ///
    /// ```ignore
    /// let wifi: &Wifi = store.section();
    /// ```
    pub fn section<S: Section>(&self) -> &S {
        S::slot(self)
    }

    /// The current value of one key.
    ///
    /// ```ignore
    /// let dark_mode: bool = store.get(appearance::DarkMode);
    /// ```
    ///
    /// The `bool` is not a choice this signature made — it is the key's own
    /// [`Key::Value`], so a control bound to `appearance::DarkMode` cannot be
    /// handed anything else.
    pub fn get<K: Key>(&self, key: K) -> K::Value
    where
        K::Section: Section,
    {
        let _ = key; // Zero-sized: the type is the argument.
        K::get(self.section::<K::Section>())
    }

    /// Write one key and persist the section it belongs to.
    ///
    /// The [`Key`] carries which section that is, so — unlike [`update`](Self::update)
    /// — a caller does not name the section at all, and cannot name the wrong one.
    pub fn set<K: Key>(&mut self, key: K, value: K::Value)
    where
        K::Section: Section,
    {
        let _ = key;
        self.update::<K::Section>(|section| K::set(section, value));
    }

    /// Edit a section and write it straight back to disk — there is no separate
    /// "Apply" step anywhere in this app, so the RON file *is* the state.
    ///
    /// Prefer [`set`](Self::set) for a single field; this is for the rest: edits
    /// that touch more than one key, or a whole value at once.
    ///
    /// A write that wouldn't change anything is skipped: sliders emit a value on
    /// every pointer move, and re-serialising an unchanged section would churn
    /// the file (and wake every other app's watcher) for nothing.
    pub fn update<S: Section>(&mut self, edit: impl FnOnce(&mut S)) {
        let before = S::slot(self).clone();
        edit(S::slot_mut(self));
        if *S::slot(self) != before {
            self.persist::<S>();
        }
    }

    /// Take a value that *came from* disk, without writing it back.
    ///
    /// Used by the watchers: an external edit is already on disk, and saving it
    /// again would be a pointless round trip.
    fn adopt<S: Section>(&mut self, value: S) {
        *S::slot_mut(self) = value;
    }

    /// Hands the section to [`crate::util::persist`], which writes it on its
    /// own thread once the edits have stopped coming.
    ///
    /// Deliberately not a write: this runs inside the widget callback that
    /// changed the value, and a slider's callback runs on every frame the
    /// pointer moves.
    fn persist<S: Section>(&self) {
        crate::util::persist::save(S::NAME, S::slot(self).clone());
    }
}

// --- MARK: Registration ---

/// Declares the app's config sections: the [`ConfigStore`] itself, its startup
/// load, one [`Section`] impl per entry, and the set of filesystem watchers that
/// keeps the store in step with the files.
///
/// Each entry is `field: Type`, where `Type` is a [`crownos_config::schema`] struct.
/// The generated watcher set is a tuple, so this tops out at the 16 elements
/// xilem's `ViewSequence` implements for tuples — far more sections than a
/// settings sidebar can reasonably hold.
macro_rules! sections {
    ($($field:ident : $ty:ty),+ $(,)?) => {
        /// Every settings section, loaded once at startup and kept in sync with
        /// disk from then on.
        ///
        /// One field per RON file. Fields are private: reads go through
        /// [`ConfigStore::section`] and writes through [`ConfigStore::update`],
        /// so no caller can hold a section that isn't backed by its file.
        #[derive(Debug, Clone, PartialEq)]
        pub struct ConfigStore {
            $($field: $ty,)+
        }

        impl ConfigStore {
            /// Read every section off disk.
            ///
            /// [`crownos_config::load`] materialises the default file when a
            /// section has never been written, so a fresh install ends up with
            /// all of these RON files on disk.
            pub fn load() -> Self {
                Self {
                    $($field: crownos_config::load::<$ty>(<$ty as Section>::NAME),)+
                }
            }

            /// A store of nothing but defaults, for tests that must not touch
            /// the filesystem.
            ///
            /// Not available outside tests on purpose: everywhere else, a store
            /// is a mirror of files on disk, and one that isn't would silently
            /// report settings the user does not have.
            #[cfg(test)]
            pub fn defaults() -> Self {
                Self {
                    $($field: <$ty as Default>::default(),)+
                }
            }

            /// Watchers that push external edits into the store.
            ///
            /// `store` projects the app's state down to the store, so this
            /// module never has to know what else that state holds. The result
            /// is element-less (it draws nothing) and belongs in the alongside
            /// slot of [`xilem::core::fork`]; each subscription lives exactly as
            /// long as the view tree it sits in.
            ///
            /// Returned as one tuple rather than chained one-per-section: `fork`
            /// takes a sequence, so a tuple of watchers costs a single `fork`
            /// where chaining would nest one per section to the same effect.
            pub fn watchers<State: 'static>(
                store: fn(&mut State) -> &mut Self,
            ) -> impl ViewSequence<State, (), ViewCtx, NoElement> + Send + Sync {
                ($(
                    watch(
                        <$ty as Section>::NAME,
                        move |state: &mut State, value: $ty| store(state).adopt(value),
                    ),
                )+)
            }
        }

        $(
            impl Section for $ty {
                const NAME: &'static str = <$ty>::SECTION;

                fn slot(store: &ConfigStore) -> &Self {
                    &store.$field
                }

                fn slot_mut(store: &mut ConfigStore) -> &mut Self {
                    &mut store.$field
                }
            }
        )+
    };
}

sections! {
    wifi: Wifi,
    bluetooth: Bluetooth,
    appearance: Appearance,
    display: Display,
    sound: Sound,
    input: Input,
    keybinds: Keybinds,
    notifications: Notifications,
    power: Power,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crownos_config::schema::wifi;
    use crate::util::persist;

    /// Something that is definitely not RON, used to prove a write *didn't*
    /// happen: if the store had saved, this would be gone.
    const SENTINEL: &[u8] = b"not RON, and must survive";

    /// One test function, because `CROWN_CONFIG_DIR` is process-global and cargo
    /// runs test functions on parallel threads.
    #[test]
    fn store_reads_writes_and_skips_no_op_saves() {
        let dir = std::env::temp_dir().join(format!("crownsettings-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create temp config dir");
        // SAFETY: single-threaded section of a single test function; no other
        // test in this binary touches the environment.
        unsafe { std::env::set_var(crownos_config::CONFIG_DIR_ENV, &dir) };

        let wifi_path = crownos_config::path_for(Wifi::NAME);

        // --- every section is materialised on first load -------------------
        let mut store = ConfigStore::load();
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
        assert_eq!(store.section::<Wifi>(), &Wifi::default());

        // --- a write reaches the interface at once and the disk later ------
        // Persisting is debounced onto its own thread — see
        // [`crate::util::persist`] — so the store is authoritative the instant
        // the edit lands, and the file catches up. Every on-disk assertion
        // below therefore flushes first, which is the same thing the window's
        // shutdown does.
        store.update::<Wifi>(|wifi| wifi.network = Some("Workshop".to_owned()));
        assert_eq!(store.section::<Wifi>().network.as_deref(), Some("Workshop"));
        assert_eq!(
            crownos_config::load::<Wifi>(Wifi::NAME).network, None,
            "the write is queued, not made, inside the callback"
        );

        persist::flush();
        assert_eq!(
            crownos_config::load::<Wifi>(Wifi::NAME).network.as_deref(),
            Some("Workshop"),
            "update() must persist the edit"
        );

        // --- a key round-trips through the store and the file --------------
        // Note there is no section named anywhere on these two lines: the key
        // carries it, and `get` returns the field's own type.
        store.set(wifi::Enabled, false);
        let enabled: bool = store.get(wifi::Enabled);
        assert!(!enabled);
        persist::flush();
        assert!(
            !crownos_config::load::<Wifi>(Wifi::NAME).enabled,
            "set() must persist the edit"
        );
        assert_eq!(
            store.get(wifi::Network).as_deref(),
            Some("Workshop"),
            "setting one key must leave its neighbours alone"
        );
        store.set(wifi::Enabled, true);

        // --- a key write that changes nothing doesn't touch the file -------
        // Flushed first, or the write queued just above would land on the
        // sentinel and the assertion would be about the wrong thing.
        persist::flush();
        std::fs::write(&wifi_path, SENTINEL).expect("plant sentinel");
        store.set(wifi::Enabled, true);
        persist::flush();
        assert_eq!(
            std::fs::read(&wifi_path).unwrap(),
            SENTINEL,
            "a no-op set must not rewrite the file"
        );

        // --- an update that changes nothing doesn't touch the file ---------
        std::fs::write(&wifi_path, SENTINEL).expect("plant sentinel");
        store.update::<Wifi>(|wifi| wifi.network = Some("Workshop".to_owned()));
        persist::flush();
        assert_eq!(
            std::fs::read(&wifi_path).unwrap(),
            SENTINEL,
            "a no-op update must not rewrite the file"
        );

        // --- an update that does change something still writes -------------
        store.update::<Wifi>(|wifi| wifi.enabled = false);
        persist::flush();
        let on_disk = crownos_config::load::<Wifi>(Wifi::NAME);
        assert!(!on_disk.enabled);
        assert_eq!(on_disk.network.as_deref(), Some("Workshop"));

        // --- adopt takes a value that is already on disk, silently ---------
        persist::flush();
        std::fs::write(&wifi_path, SENTINEL).expect("plant sentinel");
        store.adopt(Wifi {
            enabled: true,
            network: None,
        });
        assert_eq!(store.section::<Wifi>(), &Wifi::default());
        assert_eq!(
            std::fs::read(&wifi_path).unwrap(),
            SENTINEL,
            "adopt() must not write back a value that came from disk"
        );

        // --- sections are independent -------------------------------------
        store.update::<Sound>(|sound| sound.output_volume = 77.0);
        persist::flush();
        assert_eq!(
            crownos_config::load::<Sound>(Sound::NAME).output_volume,
            77.0
        );
        assert_eq!(
            store.section::<Display>(),
            &Display::default(),
            "editing one section must not disturb another"
        );

        // SAFETY: as above.
        unsafe { std::env::remove_var(crownos_config::CONFIG_DIR_ENV) };
        let _ = std::fs::remove_dir_all(&dir);
    }
}
