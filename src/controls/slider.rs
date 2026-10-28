use crownos_config::Key;
use crownui::kit::{SliderKind, SliderOptions, slider};
use crownui::prelude::{Cx, Element, Styled};

use crate::config::Section;
use crate::state::Store;
use crate::util::bridge::bridge;

/// Every slider in the app presents a 0–100 scale, whatever the stored field's
/// own range is.
const PERCENT: (f32, f32) = (0.0, 100.0);
/// The widest a slider grows: it takes what its row leaves, up to this.
const SLIDER_WIDTH: f32 = 480.0;
/// How much harder a slider shrinks than the text beside it, so a narrow row
/// narrows the slider (down to its minimum) before the label has to wrap.
const SLIDER_SHRINK: f32 = 100.0;

/// A number a slider can stand for: the `f64` most sections store, or the
/// `f32` a few do.
pub trait Stored: Copy + 'static {
    fn widen(self) -> f64;
    fn narrow(shown: f64) -> Self;
}

impl Stored for f64 {
    fn widen(self) -> f64 {
        self
    }

    fn narrow(shown: f64) -> Self {
        shown
    }
}

impl Stored for f32 {
    fn widen(self) -> f64 {
        f64::from(self)
    }

    fn narrow(shown: f64) -> Self {
        shown as f32
    }
}

/// A 0–100 slider bound to a key that stores that same 0–100 scale.
pub fn percent_slider<K>(cx: &mut Cx, store: Store, key: K) -> Element<SliderKind>
where
    K: Key,
    K::Value: Stored,
    K::Section: Section,
{
    scaled(cx, store, key, 1.0)
}

/// A 0–100 slider bound to a key that stores a `0.0..=1.0` fraction: the user
/// thinks in percent, the RON file keeps the fraction.
pub fn fraction_slider<K>(cx: &mut Cx, store: Store, key: K) -> Element<SliderKind>
where
    K: Key,
    K::Value: Stored,
    K::Section: Section,
{
    scaled(cx, store, key, 100.0)
}

fn scaled<K>(cx: &mut Cx, store: Store, key: K, factor: f64) -> Element<SliderKind>
where
    K: Key,
    K::Value: Stored,
    K::Section: Section,
{
    let value = bridge(
        cx,
        move |runtime| (store.get(runtime, key).widen() * factor) as f32,
        move |runtime, shown| {
            store.set(runtime, key, Stored::narrow(f64::from(shown) / factor));
        },
    );
    slider(value)
        .range(PERCENT.0, PERCENT.1)
        .basis(SLIDER_WIDTH)
        .shrink(SLIDER_SHRINK)
}
