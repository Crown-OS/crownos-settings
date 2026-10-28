//! How the clock's digits look: their arrangement, weight, height and glass,
//! the shadow under them and the cut-out standing in front of them.

use std::path::Path;

use crownos_config::schema::lock::ClockKey;
use crownos_config::schema::{ClockArrangement, ClockFrame, ClockLook, Lock};
use crownui::kit::{
    ButtonOptions, ButtonSize, SliderKind, SliderOptions, StatusOptions, TextInputOptions, Tone,
    button, slider, status, toggle,
};
use crownui::prelude::{Cx, Element, Listeners, Runtime, Semantic, Signal, View, vstack};

use crate::controls::{Options, field_choice, field_text};
use crate::layout::{
    optional, setting_row, setting_row_content, setting_row_desc, settings_divider,
};
use crate::state::Store;
use crate::util::bridge::bridge;
use crate::util::home::expand_home;

const PATH_WIDTH: f32 = 280.0;
/// Every slider shows 0 to 100, whatever it stands for.
const SHOWN: f32 = 100.0;
const WEIGHT_STEP: u16 = 100;
/// The weight a clock style leaves unchanged reads as this on the slider.
const STYLE_WEIGHT: u16 = 400;

static ARRANGEMENTS: Options<ClockArrangement> = Options::new(&[
    ("Side by side", ClockArrangement::Row),
    ("Hours over minutes", ClockArrangement::Stacked),
]);

pub fn look_rows(cx: &mut Cx, store: Store) -> impl View + use<> {
    let weight = look_value(
        cx,
        store,
        |look| {
            let weight = look.weight.unwrap_or(STYLE_WEIGHT);
            f32::from(weight - ClockLook::MIN_WEIGHT)
                / f32::from(ClockLook::MAX_WEIGHT - ClockLook::MIN_WEIGHT)
                * SHOWN
        },
        |look, shown| {
            let span = f32::from(ClockLook::MAX_WEIGHT - ClockLook::MIN_WEIGHT);
            let steps = (shown / SHOWN * span / f32::from(WEIGHT_STEP)).round() as u16;
            look.weight = Some(ClockLook::MIN_WEIGHT + steps * WEIGHT_STEP);
        },
    );
    let stretch = look_value(
        cx,
        store,
        |look| {
            (look.stretch - ClockLook::MIN_STRETCH)
                / (ClockLook::MAX_STRETCH - ClockLook::MIN_STRETCH)
                * SHOWN
        },
        |look, shown| {
            look.stretch = ClockLook::MIN_STRETCH
                + shown / SHOWN * (ClockLook::MAX_STRETCH - ClockLook::MIN_STRETCH);
        },
    );
    let shadow = look_value(
        cx,
        store,
        |look| look.shadow * SHOWN,
        |look, shown| {
            look.shadow = shown / SHOWN;
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
            setting_row(
                "Arrangement",
                field_choice(
                    cx,
                    &ARRANGEMENTS,
                    move |runtime| store.section(runtime, |lock: &Lock| lock.clock.arrangement),
                    move |runtime, arrangement| {
                        edit(runtime, store, |look| look.arrangement = arrangement);
                    },
                )
                .label("Arrangement"),
            ),
            settings_divider(),
            setting_row("Weight", percent(weight).label("Weight")),
        )),
        vstack((
            settings_divider(),
            setting_row_desc(
                "Height",
                "Stands the digits taller than the font draws them",
                percent(stretch).label("Height"),
            ),
            settings_divider(),
            setting_row_desc(
                "Frosted glass",
                "The background shows through the digits, blurred",
                toggle(glass).label("Frosted glass"),
            ),
            settings_divider(),
            setting_row("Shadow", percent(shadow).label("Shadow")),
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

/// A 0–100 slider over `value`.
fn percent(value: Signal<f32>) -> Element<SliderKind> {
    slider(value).range(0.0, SHOWN)
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
