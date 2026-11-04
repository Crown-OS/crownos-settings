//! How the clock's digits look: their height, weight, light and bend, the
//! glass and shadow they are drawn with and the cut-out in front of them —
//! each a few named choices.

use std::path::Path;

use crownos_config::schema::lock::ClockKey;
use crownos_config::schema::{ClockFrame, ClockLighting, ClockLook, Lock};
use crownui::kit::{
    ButtonOptions, ButtonSize, StatusOptions, TextInputOptions, ToggleGroupKind, Tone, button,
    status, toggle,
};
use crownui::prelude::{Cx, Element, Listeners, Runtime, Semantic, Signal, View, vstack};

use super::looks::{WARPS, Warp};
use crate::controls::{Options, apart, exact, field_text, segments};
use crate::layout::{
    optional, setting_row, setting_row_content, setting_row_desc, settings_divider,
};
use crate::state::Store;
use crate::util::bridge::bridge;
use crate::util::home::expand_home;

const PATH_WIDTH: f32 = 280.0;

static HEIGHTS: Options<f32> = Options::new(&[
    ("Regular", 1.0),
    ("Tall", 1.6),
    ("Taller", 2.2),
    ("Tallest", 2.8),
]);

static WEIGHTS: Options<Option<u16>> = Options::new(&[
    ("Auto", None),
    ("Thin", Some(200)),
    ("Light", Some(300)),
    ("Regular", Some(400)),
    ("Bold", Some(700)),
]);

static LIGHTING: Options<ClockLighting> = Options::new(&[
    ("None", ClockLighting::None),
    ("Glow", ClockLighting::Glow),
    ("Sheen", ClockLighting::Sheen),
]);

static SHADOWS: Options<f32> = Options::new(&[("None", 0.0), ("Soft", 0.35), ("Strong", 0.8)]);

pub fn look_rows(cx: &mut Cx, store: Store) -> impl View + use<> {
    let height = look_choice(
        cx,
        store,
        &HEIGHTS,
        apart,
        |look| look.stretch,
        |look, stretch| {
            look.stretch = stretch;
        },
    );
    let weight = look_choice(
        cx,
        store,
        &WEIGHTS,
        weights_apart,
        |look| look.weight,
        |look, weight| {
            look.weight = weight;
        },
    );
    let lighting = look_choice(
        cx,
        store,
        &LIGHTING,
        exact,
        |look| look.lighting,
        |look, lighting| {
            look.lighting = lighting;
        },
    );
    let warp = look_choice(
        cx,
        store,
        &WARPS,
        exact,
        |look| Warp::nearest(look.frame.warp),
        |look, warp| look.frame.warp = warp.corners(),
    );
    let shadow = look_choice(
        cx,
        store,
        &SHADOWS,
        apart,
        |look| look.shadow,
        |look, shadow| {
            look.shadow = shadow;
        },
    );
    let glass = look_value(cx, store, |look| look.glass, |look, on| look.glass = on);
    let morph = look_value(
        cx,
        store,
        |look| look.morph_digits,
        |look, on| look.morph_digits = on,
    );
    vstack((
        vstack((
            settings_divider(),
            setting_row_desc(
                "Height",
                "Stems grow taller while curves stay round",
                height.label("Height"),
            ),
            settings_divider(),
            setting_row("Weight", weight.label("Weight")),
            settings_divider(),
            setting_row_desc(
                "Lighting",
                "A halo around the digits, or light sweeping across them",
                lighting.label("Lighting"),
            ),
            settings_divider(),
            setting_row_desc(
                "Warp",
                "Bends the clock; drag its corners in the preview for any other",
                warp.label("Warp"),
            ),
        )),
        vstack((
            settings_divider(),
            setting_row_desc(
                "Frosted glass",
                "The background shows through the digits, blurred",
                toggle(glass).label("Frosted glass"),
            ),
            settings_divider(),
            setting_row("Shadow", shadow.label("Shadow")),
            settings_divider(),
            setting_row_desc(
                "Animate changes",
                "A digit blurs away as the next one arrives",
                toggle(morph).label("Animate changes"),
            ),
            depth_rows(cx, store),
            settings_divider(),
            setting_row_desc(
                "Position",
                "Drag the clock in the preview to move, scale, warp or split it",
                button("Reset")
                    .control_size(ButtonSize::Small)
                    .label("Reset the clock's position")
                    .on_click(move |cx, _| {
                        edit(cx, store, |look| look.frame = ClockFrame::default());
                    }),
            ),
        )),
    ))
}

fn weights_apart(left: Option<u16>, right: Option<u16>) -> f32 {
    match (left, right) {
        (Some(left), Some(right)) => f32::from(left.abs_diff(right)),
        (None, None) => 0.0,
        _ => f32::from(u16::MAX),
    }
}

fn depth_rows(cx: &mut Cx, store: Store) -> impl View + use<> {
    let relative = cx.signal(false);
    let path = field_text(
        cx,
        move |runtime| {
            store.section(runtime, |lock: &Lock| {
                lock.clock.depth.clone().unwrap_or_default()
            })
        },
        move |runtime, typed| {
            let path = expand_home(typed.trim());
            let absolute = Path::new(path.as_ref()).is_absolute();
            relative.set(runtime, !path.is_empty() && !absolute);
            if path.is_empty() || absolute {
                let depth = (!path.is_empty()).then(|| path.into_owned());
                edit(runtime, store, |look| look.depth = depth);
            }
        },
    )
    .placeholder("/path/to/subject.png")
    .width(PATH_WIDTH)
    .label("Depth cut-out");
    vstack((
        settings_divider(),
        setting_row_desc(
            "Depth cut-out",
            "The background's subject on a transparent ground, to stand in front of the clock; looks right with background blur off",
            path,
        ),
        optional(
            move |runtime| {
                relative
                    .get(runtime)
                    .then_some("Give the full path, starting with / or ~/")
            },
            |_, problem| setting_row_content(status(problem).tone(Tone::Warning)),
        ),
    ))
}

/// Segments over `options` for one part of the clock's look.
fn look_choice<T: Copy + PartialEq + 'static>(
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

/// One part of the clock's look, read and written through the whole key.
fn look_value<T: Clone + PartialEq + 'static>(
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

fn edit(runtime: &mut Runtime, store: Store, change: impl FnOnce(&mut ClockLook)) {
    let mut look = store.get(runtime, ClockKey);
    change(&mut look);
    store.set(runtime, ClockKey, look);
}
