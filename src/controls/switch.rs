use crownos_config::Key;
use crownui::kit::{ToggleKind, toggle};
use crownui::prelude::{Cx, Element, Runtime};

use crate::config::Section;
use crate::state::Store;
use crate::util::bridge::bridge;

/// A toggle bound to a `bool` key.
pub fn switch<K>(cx: &mut Cx, store: Store, key: K) -> Element<ToggleKind>
where
    K: Key<Value = bool>,
    K::Section: Section,
{
    switch_then(cx, store, key, |_, _| {})
}

/// A [`switch`] that runs `after_toggle` with every value the user flips it
/// to, once the value is stored — never for edits that arrive from disk.
pub fn switch_then<K>(
    cx: &mut Cx,
    store: Store,
    key: K,
    mut after_toggle: impl FnMut(&mut Runtime, bool) + 'static,
) -> Element<ToggleKind>
where
    K: Key<Value = bool>,
    K::Section: Section,
{
    let on = bridge(
        cx,
        move |runtime| store.get(runtime, key),
        move |runtime, on| {
            store.set(runtime, key, on);
            after_toggle(runtime, on);
        },
    );
    toggle(on)
}
