//! One card per moment the lock screen animates: a preset, or the effects,
//! duration and easing by hand, and a button to watch it in the preview.

mod editors;
mod effects;
mod preset;

use crownos_config::schema::lock::Transitions;
use crownos_config::schema::{Easing, Lock, Transition};
use crownui::kit::{SliderKind, SliderOptions, slider};
use crownui::prelude::{Cx, Element, Runtime, Semantic, Styled, View, bind, hstack, vstack};

use super::moment::Moment;
use super::preview::Preview;
use crate::controls::{Options, field_choice};
use crate::layout::{
    INLINE_GAP, inline, setting_row, setting_row_desc, settings_card_titled, settings_divider,
    value_text,
};
use crate::state::Store;
use crate::util::bridge::bridge;
use effects::EFFECTS;
use preset::{PRESETS, Preset};

/// The longest a transition may run, as the schema caps it.
const LONGEST_MS: u32 = 2000;
/// What a preset picked over an instant transition runs for.
const FIRST_DURATION_MS: u32 = 300;
const DURATION_STEP_MS: f32 = 10.0;
const SLIDER_WIDTH: f32 = 380.0;
const SLIDER_SHRINK: f32 = 100.0;
/// Room for a slider's readout, so a changing number never moves the slider.
const READOUT_WIDTH: f32 = 64.0;

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
            setting_row("Duration", duration_slider(cx, store, moment)),
            settings_divider(),
            setting_row("Easing", easing),
            effects,
        ),
    )
}

fn duration_slider(cx: &mut Cx, store: Store, moment: Moment) -> impl View + use<> {
    let shown = bridge(
        cx,
        move |runtime| {
            read(runtime, store, moment, |transition| {
                transition.duration_ms as f32
            })
        },
        move |runtime, millis: f32| {
            let millis = (millis.round() as u32).min(LONGEST_MS);
            edit(runtime, store, moment, |transition| {
                transition.duration_ms = millis
            });
        },
    );
    with_readout(
        slider(shown)
            .range(0.0, LONGEST_MS as f32)
            .step(DURATION_STEP_MS)
            .label("Duration"),
        value_text(bind(move |runtime| {
            format!("{} ms", shown.get(runtime).round())
        })),
    )
}

/// `slider` with its value read out after it, as wide as a lone slider.
fn with_readout(slider: Element<SliderKind>, readout: impl View) -> impl View {
    hstack((
        slider.flex_1(),
        vstack(readout).w(READOUT_WIDTH).items_end(),
    ))
    .gap(INLINE_GAP)
    .items_center()
    .basis(SLIDER_WIDTH)
    .shrink(SLIDER_SHRINK)
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
