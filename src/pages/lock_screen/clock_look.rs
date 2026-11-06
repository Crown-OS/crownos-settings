//! Editing one part of the clock's look, which the settings store as one key.

use crownos_config::schema::lock::ClockKey;
use crownos_config::schema::{ClockLook, Lock};
use crownui::kit::ToggleGroupKind;
use crownui::prelude::{Cx, Element, Runtime, Signal};

use crate::controls::{Options, segments};
use crate::state::Store;
use crate::util::bridge::bridge;

/// Segments over `options` for one part of the clock's look.
pub fn look_choice<T: Copy + PartialEq + 'static>(
    cx: &mut Cx,
    store: Store,
    options: &'static Options<T>,
    distance: fn(T, T) -> f32,
    read: fn(&ClockLook) -> T,
    write: fn(&mut ClockLook, T),
) -> Element<ToggleGroupKind> {
    segments(
        cx,
        options,
        distance,
        move |runtime| store.section(runtime, |lock: &Lock| read(&lock.clock)),
        move |runtime, value| edit(runtime, store, |look| write(look, value)),
    )
}

/// One part of the clock's look as a signal a widget edits.
pub fn look_value<T: Clone + PartialEq + 'static>(
    cx: &mut Cx,
    store: Store,
    read: fn(&ClockLook) -> T,
    write: fn(&mut ClockLook, T),
) -> Signal<T> {
    bridge(
        cx,
        move |runtime| store.section(runtime, |lock: &Lock| read(&lock.clock)),
        move |runtime, value| edit(runtime, store, |look| write(look, value)),
    )
}

pub fn edit(runtime: &mut Runtime, store: Store, change: impl FnOnce(&mut ClockLook)) {
    let mut look = store.get(runtime, ClockKey);
    change(&mut look);
    store.set(runtime, ClockKey, look);
}
