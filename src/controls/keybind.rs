use crownos_config::{Key, Keybind};
use crownui::kit::{KeybindInputKind, keybind_input};
use crownui::prelude::{Cx, Element};

use crate::config::Section;
use crate::state::Store;
use crate::util::bridge::bridge;

/// A shortcut recorder bound to a [`Keybind`] key: a page never says what a
/// shortcut *is* — only which setting it belongs to.
pub fn keybind<K>(cx: &mut Cx, store: Store, key: K) -> Element<KeybindInputKind>
where
    K: Key<Value = Keybind>,
    K::Section: Section,
{
    let chord = bridge(
        cx,
        move |runtime| store.get(runtime, key),
        move |runtime, chord| store.set(runtime, key, chord),
    );
    keybind_input(chord)
}
