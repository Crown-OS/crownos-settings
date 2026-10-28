//! A switch per kind of effect, and its parameters while it is on.

use crownui::kit::{SliderOptions, slider, toggle};
use crownui::prelude::{AnyView, Cx, Semantic, Styled, View, bind, vstack};

use super::effects::{EDGES, EffectKind, Param};
use super::{edit, read, with_readout};
use crate::controls::field_choice;
use crate::layout::{setting_row, setting_row_desc, settings_divider, value_text, when};
use crate::pages::lock_screen::moment::Moment;
use crate::state::Store;
use crate::util::bridge::bridge;

/// How far a parameter row sits in from its effect's switch.
const PARAM_INDENT: f32 = 24.0;

pub fn effect_rows(
    cx: &mut Cx,
    store: Store,
    moment: Moment,
    kind: &'static EffectKind,
) -> impl View + use<> {
    let is_on = move |runtime: &mut _| {
        read(runtime, store, moment, |transition| {
            transition
                .effects
                .iter()
                .any(|effect| kind.is_kind_of(effect))
        })
    };
    let on = bridge(cx, is_on, move |runtime, on| {
        edit(runtime, store, moment, |transition| {
            kind.switch(&mut transition.effects, on)
        });
    });
    vstack((
        settings_divider(),
        setting_row_desc(
            kind.name,
            kind.description,
            toggle(on).label(format!("{} {}", moment.title(), kind.name)),
        ),
        when(is_on, move |cx| {
            let rows: Vec<AnyView> = kind
                .params
                .iter()
                .map(|param| {
                    vstack((
                        settings_divider(),
                        param_row(cx, store, moment, kind, param),
                    ))
                    .boxed()
                })
                .collect();
            vstack(rows).pl(PARAM_INDENT)
        }),
    ))
}

fn param_row(
    cx: &mut Cx,
    store: Store,
    moment: Moment,
    kind: &'static EffectKind,
    param: &'static Param,
) -> AnyView {
    let current = move |runtime: &mut _| {
        read(runtime, store, moment, |transition| {
            transition
                .effects
                .iter()
                .find(|effect| kind.is_kind_of(effect))
                .copied()
        })
        .unwrap_or(kind.default)
    };
    let edit_effect = move |runtime: &mut _, change: &dyn Fn(&mut _)| {
        edit(runtime, store, moment, |transition| {
            if let Some(effect) = transition
                .effects
                .iter_mut()
                .find(|effect| kind.is_kind_of(effect))
            {
                change(effect);
            }
        });
    };
    match *param {
        Param::Amount {
            label,
            shown,
            step,
            unit,
            shown_per_stored,
            read,
            write,
        } => {
            let value = bridge(
                cx,
                move |runtime| read(&current(runtime)).unwrap_or_default() * shown_per_stored,
                move |runtime, value: f32| {
                    edit_effect(runtime, &|effect| write(effect, value / shown_per_stored));
                },
            );
            setting_row(
                label,
                with_readout(
                    slider(value)
                        .range(shown.0, shown.1)
                        .step(step)
                        .label(label),
                    value_text(bind(move |runtime| {
                        format!("{} {unit}", value.get(runtime).round())
                    })),
                ),
            )
            .boxed()
        }
        Param::Edge { label, read, write } => setting_row(
            label,
            field_choice(
                cx,
                &EDGES,
                move |runtime| read(&current(runtime)).unwrap_or_default(),
                move |runtime, edge| edit_effect(runtime, &|effect| write(effect, edge)),
            )
            .label(label),
        )
        .boxed(),
    }
}
