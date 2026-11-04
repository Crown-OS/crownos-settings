use crownui::kit::{ToggleGroupKind, ToggleItem, toggle_group};
use crownui::prelude::{Cx, Element, Runtime};

use super::Options;
use crate::util::bridge::bridge;

/// How far apart two values are, for showing the entry nearest a stored
/// value no entry names exactly.
pub type Distance<T> = fn(T, T) -> f32;

pub fn exact<T: PartialEq>(left: T, right: T) -> f32 {
    f32::from(u8::from(left != right))
}

pub fn apart(left: f32, right: f32) -> f32 {
    (left - right).abs()
}

/// A row of segments over an [`Options`] table for a value that may be only
/// part of a key: `read` picks it out, `write` puts a new one back. A value
/// between entries lights the nearest.
pub fn segments<T: Copy + PartialEq + 'static>(
    cx: &mut Cx,
    options: &'static Options<T>,
    distance: Distance<T>,
    mut read: impl FnMut(&mut Runtime) -> T + 'static,
    mut write: impl FnMut(&mut Runtime, T) + 'static,
) -> Element<ToggleGroupKind> {
    let selected = bridge(
        cx,
        move |runtime| options.nearest(read(runtime), distance),
        move |runtime, index: usize| {
            if let Some(value) = options.value_at(index) {
                write(runtime, value);
            }
        },
    );
    toggle_group(options.labels().into_iter().map(ToggleItem::text), selected)
}
