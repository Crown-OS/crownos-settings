//! Typed, self-persisting access to the on-disk settings sections.
//!
//! [`crownconfig`] speaks in `(section name, value)` pairs and leaves two things
//! to its caller: remembering which name goes with which type, and remembering
//! to save after every edit. This module ties both down.
//!
//! * [`Section`] pins a [`crownconfig::schema`] struct to its section name *and*
//!   to its slot in [`ConfigStore`], which is what lets everything above this
//!   module be generic over "some section" instead of naming all seven.
//! * [`ConfigStore::update`] is the only way to change a value, and it persists
//!   what it changed. "Mutate the field but forget to write the file" stops
//!   being a mistake a caller is able to make.
//!
//! Adding a section is one line in the [`sections!`] invocation at the bottom of
//! this file: the store field, the startup load, the [`Section`] impl and the
//! filesystem watcher are all generated from it.

use std::fmt::Debug;

use crownconfig::schema::{Appearance, Bluetooth, Display, Notifications, Power, Sound, Wifi};
use crownconfig::xilem_view::watch;
use serde::Serialize;
use serde::de::DeserializeOwned;
use xilem::ViewCtx;
use xilem::core::{NoElement, ViewSequence};

// --- MARK: Section ---

/// One settings section: a schema struct stored in exactly one RON file, plus
/// the knowledge of where it lives inside a [`ConfigStore`].
///
/// The two accessors are the interesting part. `crownconfig`'s schema types
/// carry a `SECTION` constant but no trait, so code that wanted to touch "the
/// section this control belongs to" had to name a concrete field. Implementing
/// `Section` turns that field access into something generic code can ask for,
/// which is what collapses seven near-identical `save_*` methods into one
/// [`ConfigStore::update`].
///
/// The supertraits are the union of what [`crownconfig::load`], [`crownconfig::save`]
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

    /// Edit a section and write it straight back to disk — there is no separate
    /// "Apply" step anywhere in this app, so the RON file *is* the state.
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

    /// Saving is best effort: a read-only config dir shouldn't take the window
    /// down, but it also shouldn't fail silently.
    fn persist<S: Section>(&self) {
        if let Err(err) = crownconfig::save(S::NAME, S::slot(self)) {
            eprintln!("crownsettings: could not save {}.ron: {err}", S::NAME);
        }
    }
}

// --- MARK: Registration ---

/// Declares the app's config sections: the [`ConfigStore`] itself, its startup
/// load, one [`Section`] impl per entry, and the set of filesystem watchers that
/// keeps the store in step with the files.
///
/// Each entry is `field: Type`, where `Type` is a [`crownconfig::schema`] struct.
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
            /// [`crownconfig::load`] materialises the default file when a
            /// section has never been written, so a fresh install ends up with
            /// all of these RON files on disk.
            pub fn load() -> Self {
                Self {
                    $($field: crownconfig::load::<$ty>(<$ty as Section>::NAME),)+
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
    notifications: Notifications,
    power: Power,
}

#[cfg(test)]
mod tests {
    use super::*;

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
        unsafe { std::env::set_var(crownconfig::CONFIG_DIR_ENV, &dir) };

        let wifi_path = crownconfig::path_for(Wifi::NAME);

        // --- every section is materialised on first load -------------------
        let mut store = ConfigStore::load();
        for section in [
            Wifi::NAME,
            Bluetooth::NAME,
            Appearance::NAME,
            Display::NAME,
            Sound::NAME,
            Notifications::NAME,
            Power::NAME,
        ] {
            assert!(
                crownconfig::path_for(section).exists(),
                "load() should materialise {section}.ron"
            );
        }
        assert_eq!(store.section::<Wifi>(), &Wifi::default());

        // --- update edits the section and writes it through ----------------
        store.update::<Wifi>(|wifi| wifi.network = Some("Workshop".to_owned()));
        assert_eq!(store.section::<Wifi>().network.as_deref(), Some("Workshop"));
        assert_eq!(
            crownconfig::load::<Wifi>(Wifi::NAME).network.as_deref(),
            Some("Workshop"),
            "update() must persist the edit"
        );

        // --- an update that changes nothing doesn't touch the file ---------
        std::fs::write(&wifi_path, SENTINEL).expect("plant sentinel");
        store.update::<Wifi>(|wifi| wifi.network = Some("Workshop".to_owned()));
        assert_eq!(
            std::fs::read(&wifi_path).unwrap(),
            SENTINEL,
            "a no-op update must not rewrite the file"
        );

        // --- an update that does change something still writes -------------
        store.update::<Wifi>(|wifi| wifi.enabled = false);
        let on_disk = crownconfig::load::<Wifi>(Wifi::NAME);
        assert!(!on_disk.enabled);
        assert_eq!(on_disk.network.as_deref(), Some("Workshop"));

        // --- adopt takes a value that is already on disk, silently ---------
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
        assert_eq!(crownconfig::load::<Sound>(Sound::NAME).output_volume, 77.0);
        assert_eq!(
            store.section::<Display>(),
            &Display::default(),
            "editing one section must not disturb another"
        );

        // SAFETY: as above.
        unsafe { std::env::remove_var(crownconfig::CONFIG_DIR_ENV) };
        let _ = std::fs::remove_dir_all(&dir);
    }
}
