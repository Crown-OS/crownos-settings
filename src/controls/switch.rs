use crownos_config::Key;
use crownui::kit::{ToggleKind, toggle};
use crownui::prelude::{Cx, Element};

use crate::config::Section;
use crate::state::Store;
use crate::util::bridge::bridge;

/// A toggle bound to a `bool` key.
pub fn switch<K>(cx: &mut Cx, store: Store, key: K) -> Element<ToggleKind>
where
    K: Key<Value = bool>,
    K::Section: Section,
{
    let on = bridge(
        cx,
        move |runtime| store.get(runtime, key),
        move |runtime, on| store.set(runtime, key, on),
    );
    toggle(on)
}
