//! One card per moment the lock screen animates: a preset, or the effects,
//! duration and easing by hand, and a button to watch it in the preview.

mod editors;
mod effects;
mod preset;

use crownos_config::schema::lock::Transitions;
use crownos_config::schema::{Easing, Lock, Transition};
use crownui::prelude::{Cx, Runtime, Semantic, View};

use super::moment::Moment;
use super::preview::Preview;
use crate::controls::{Options, apart, field_choice, segments};
use crate::layout::{
    inline, setting_row, setting_row_desc, settings_card_titled, settings_divider,
};
use crate::state::Store;
use effects::EFFECTS;
use preset::{PRESETS, Preset};

/// What a preset picked over an instant transition runs for.
const FIRST_DURATION_MS: u32 = 300;

static DURATIONS: Options<f32> = Options::new(&[
    ("Instant", 0.0),
    ("Quick", 150.0),
    ("Normal", 300.0),
    ("Relaxed", 500.0),
    ("Slow", 800.0),
]);

static EASINGS: Options<Easing> = Options::new(&[
    ("Ease out", Easing::EaseOut),
    ("Ease in", Easing::EaseIn),
    ("Ease in and out", Easing::EaseInOut),
    ("Linear", Easing::Linear),
    ("Spring", Easing::Spring),
]);

pub fn transition_card(
    cx: &mut Cx,
    store: Store,
    preview: &Preview,
    moment: Moment,
) -> impl View + use<> {
    let preset = field_choice(
        cx,
        &PRESETS,
        move |runtime| {
            read(runtime, store, moment, |transition| {
                Preset::of(&transition.effects)
            })
        },
        move |runtime, preset| {
            let Some(effects) = preset.effects() else {
                return;
            };
            edit(runtime, store, moment, |transition| {
                if transition.duration_ms == 0 {
                    transition.duration_ms = FIRST_DURATION_MS;
                }
                transition.effects = effects;
            });
        },
    )
    .label(format!("{} effect", moment.title()));
    let easing = field_choice(
        cx,
        &EASINGS,
        move |runtime| read(runtime, store, moment, |transition| transition.easing),
        move |runtime, easing| {
            edit(runtime, store, moment, |transition| {
                transition.easing = easing
            })
        },
    );
    let effects: Vec<_> = EFFECTS
        .iter()
        .map(|kind| editors::effect_rows(cx, store, moment, kind))
        .collect();
    settings_card_titled(
        &format!("{} transition", moment.title()),
        (
            setting_row_desc(
                "Effect",
                moment.summary(),
                inline((preset, preview.play_button(moment, "Play"))),
            ),
            settings_divider(),
            setting_row("Duration", duration_choice(cx, store, moment)),
            settings_divider(),
            setting_row("Easing", easing),
            effects,
        ),
    )
}

fn duration_choice(cx: &mut Cx, store: Store, moment: Moment) -> impl View + use<> {
    segments(
        cx,
        &DURATIONS,
        apart,
        move |runtime| {
            read(runtime, store, moment, |transition| {
                transition.duration_ms as f32
            })
        },
        move |runtime, millis: f32| {
            edit(runtime, store, moment, |transition| {
                transition.duration_ms = millis as u32
            });
        },
    )
    .label("Duration")
}

/// `pick` of `moment`'s transition, without copying the section.
fn read<R>(
    runtime: &mut Runtime,
    store: Store,
    moment: Moment,
    pick: impl FnOnce(&Transition) -> R,
) -> R {
    store.section(runtime, |lock: &Lock| pick(moment.of(&lock.transitions)))
}

fn edit(runtime: &mut Runtime, store: Store, moment: Moment, change: impl FnOnce(&mut Transition)) {
    let mut transitions = store.get(runtime, Transitions);
    change(moment.of_mut(&mut transitions));
    store.set(runtime, Transitions, transitions);
}
