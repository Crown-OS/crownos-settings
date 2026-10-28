//! The glass cards: how the rim of the compositor's frosted glass is lit, for
//! windows, panels and menus. Each card edits one `EdgeGlow` in `glass.ron`.

use crownos_config::Key;
use crownos_config::schema::EdgeGlow;
use crownui::kit::{SliderKind, SliderOptions as _, ToggleKind, slider, toggle};
use crownui::prelude::{Cx, Element, Styled, View};

use crate::config::Section;
use crate::layout::{setting_row, setting_row_desc, settings_card_titled, settings_divider};
use crate::state::Store;
use crate::util::bridge::bridge;

const SLIDER_WIDTH: f32 = 480.0;
const SLIDER_SHRINK: f32 = 100.0;
/// The widest bevel worth offering, in logical pixels.
const MAX_WIDTH: f32 = 12.0;

pub(super) fn rim_card<K>(cx: &mut Cx, store: Store, title: &str, key: K) -> impl View + use<K>
where
    K: Key<Value = EdgeGlow>,
    K::Section: Section,
{
    settings_card_titled(
        title,
        (
            setting_row("Edge glow", enabled(cx, store, key)),
            settings_divider(),
            setting_row_desc(
                "Width",
                "Pixels of bevel along the edge",
                field_slider(cx, store, key, (0.0, MAX_WIDTH), 1.0, Field::WIDTH),
            ),
            settings_divider(),
            setting_row_desc(
                "Intensity",
                "Brightness of the highlight",
                field_slider(cx, store, key, (0.0, 100.0), 100.0, Field::INTENSITY),
            ),
            settings_divider(),
            setting_row_desc(
                "Light angle",
                "Degrees clockwise from straight up",
                field_slider(cx, store, key, (-180.0, 180.0), 1.0, Field::LIGHT_ANGLE),
            ),
            settings_divider(),
            setting_row_desc(
                "Adaptive tint",
                "How much the edge takes the colour of the window below",
                field_slider(cx, store, key, (0.0, 100.0), 100.0, Field::ADAPTIVE_TINT),
            ),
            settings_divider(),
            setting_row_desc(
                "Inner shadow",
                "Shade just inside the edge",
                field_slider(cx, store, key, (0.0, 100.0), 100.0, Field::INNER_SHADOW),
            ),
        ),
    )
}

/// One numeric field of an [`EdgeGlow`], read and written in place.
#[derive(Clone, Copy)]
struct Field {
    read: fn(&EdgeGlow) -> f64,
    write: fn(&mut EdgeGlow, f64),
}

impl Field {
    const WIDTH: Self = Self {
        read: |glow| glow.width,
        write: |glow, value| glow.width = value,
    };
    const INTENSITY: Self = Self {
        read: |glow| glow.intensity,
        write: |glow, value| glow.intensity = value,
    };
    const LIGHT_ANGLE: Self = Self {
        read: |glow| glow.light_angle,
        write: |glow, value| glow.light_angle = value,
    };
    const ADAPTIVE_TINT: Self = Self {
        read: |glow| glow.adaptive_tint,
        write: |glow, value| glow.adaptive_tint = value,
    };
    const INNER_SHADOW: Self = Self {
        read: |glow| glow.inner_shadow,
        write: |glow, value| glow.inner_shadow = value,
    };
}

fn enabled<K>(cx: &mut Cx, store: Store, key: K) -> Element<ToggleKind>
where
    K: Key<Value = EdgeGlow>,
    K::Section: Section,
{
    let on = bridge(
        cx,
        move |runtime| store.get(runtime, key).enabled,
        move |runtime, on| {
            let mut glow = store.get(runtime, key);
            glow.enabled = on;
            store.set(runtime, key, glow);
        },
    );
    toggle(on)
}

/// A slider over `range` showing `field` multiplied by `scale`: the file keeps
/// fractions, the user reads percent.
fn field_slider<K>(
    cx: &mut Cx,
    store: Store,
    key: K,
    range: (f32, f32),
    scale: f64,
    field: Field,
) -> Element<SliderKind>
where
    K: Key<Value = EdgeGlow>,
    K::Section: Section,
{
    let value = bridge(
        cx,
        move |runtime| ((field.read)(&store.get(runtime, key)) * scale) as f32,
        move |runtime, shown| {
            let mut glow = store.get(runtime, key);
            (field.write)(&mut glow, f64::from(shown) / scale);
            store.set(runtime, key, glow);
        },
    );
    slider(value)
        .range(range.0, range.1)
        .basis(SLIDER_WIDTH)
        .shrink(SLIDER_SHRINK)
}
