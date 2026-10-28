//! Resolution, refresh, scale, orientation and adaptive sync, for the one
//! monitor the user picked out of the list.
//!
//! Every choice here is offered as a list of rows, and every one of those
//! lists has the same rule: whatever the monitor is *actually* on gets a row,
//! even when it is not something this page would have offered. A select with
//! nothing highlighted is how a user finds out their monitor is on a setting
//! the page cannot name, and then cannot get back to it.

use crownui::kit::{select, toggle};
use crownui::prelude::{Cx, IntoProp, Runtime, Semantic, Signal, View, bind, vstack};

use crate::layout::{
    optional, setting_row, setting_row_desc, settings_card_titled, settings_divider, when,
};
use crate::net::outputs::{MonitorDraft, OutputState, Transform};
use crate::state::Store;
use crate::util::bridge::{bridge, derived};

/// The scales offered. Quarter steps are what users recognise, and every one
/// is exactly representable in `wp_fractional_scale_v1`'s 120ths.
const SCALES: [f64; 9] = [1.0, 1.25, 1.5, 1.75, 2.0, 2.25, 2.5, 2.75, 3.0];

/// How close two scales have to be to count as the same one.
const SCALE_EPSILON: f64 = 0.001;

/// What the card is built from; anything finer is bound.
#[derive(Debug, Clone, PartialEq)]
struct Selection {
    name: String,
    title: String,
    modes: Vec<String>,
    vrr_capable: bool,
}

fn selection(outputs: &OutputState) -> Option<Selection> {
    let name = outputs.selected.clone()?;
    let monitor = outputs.monitor(&name)?;
    outputs.draft(&name)?;
    Some(Selection {
        title: monitor.title(),
        modes: monitor.modes.iter().map(|mode| mode.label()).collect(),
        vrr_capable: monitor.vrr_capable,
        name,
    })
}

pub fn card(store: Store) -> impl View {
    let outputs = store.outputs;
    optional(
        move |runtime| outputs.with(runtime, selection),
        move |cx, selected| {
            let Selection {
                name,
                title,
                modes,
                vrr_capable,
            } = selected;
            let draft = Draft { outputs, name };
            settings_card_titled(
                &title,
                (
                    setting_row("Resolution", resolution(cx, draft.clone(), modes)),
                    settings_divider(),
                    setting_row("Scale", scale(cx, draft.clone())),
                    settings_divider(),
                    setting_row("Orientation", orientation(cx, draft.clone())),
                    when(
                        move |_| vrr_capable,
                        move |cx| {
                            vstack((settings_divider(), adaptive_sync_row(cx, draft.clone())))
                        },
                    ),
                ),
            )
        },
    )
}

/// One monitor's draft, as the rows read and edit it.
#[derive(Clone)]
struct Draft {
    outputs: Signal<OutputState>,
    name: String,
}

impl Draft {
    fn read<R: Default>(&self, runtime: &mut Runtime, read: impl FnOnce(&MonitorDraft) -> R) -> R {
        self.outputs
            .with(runtime, |outputs| outputs.draft(&self.name).map(read))
            .unwrap_or_default()
    }

    fn edit(&self, runtime: &mut Runtime, edit: impl FnOnce(&mut MonitorDraft)) {
        self.outputs
            .update(runtime, |outputs| outputs.edit(&self.name, edit));
    }

    /// Bound to "this monitor is switched off in the draft".
    fn off(&self) -> impl IntoProp<bool> + use<> {
        let draft = self.clone();
        bind(move |runtime| !draft.read(runtime, |draft| draft.enabled))
    }
}

/// The mode list is per monitor, so its labels cannot be a static table the
/// way a schema enum's are.
fn resolution(cx: &mut Cx, draft: Draft, modes: Vec<String>) -> impl View + use<> {
    let labels = cx.signal(modes);
    let (reading, editing) = (draft.clone(), draft.clone());
    let selected = bridge(
        cx,
        move |runtime| reading.read(runtime, |draft| draft.mode),
        move |runtime, index| {
            if index.is_some() {
                editing.edit(runtime, |draft| draft.mode = index);
            }
        },
    );
    select(labels, selected).disabled(draft.off())
}

fn scale(cx: &mut Cx, draft: Draft) -> impl View + use<> {
    let (listing, reading, editing) = (draft.clone(), draft.clone(), draft.clone());
    let labels = derived(cx, move |runtime| {
        scales(listing.read(runtime, |draft| draft.scale))
            .iter()
            .map(|value| format!("{:.0}%", value * 100.0))
            .collect::<Vec<_>>()
    });
    let selected = bridge(
        cx,
        move |runtime| {
            let current = reading.read(runtime, |draft| draft.scale);
            scales(current)
                .iter()
                .position(|value| (value - current).abs() < SCALE_EPSILON)
        },
        move |runtime, index: Option<usize>| {
            let current = editing.read(runtime, |draft| draft.scale);
            if let Some(value) = index.and_then(|index| scales(current).get(index).copied()) {
                editing.edit(runtime, |draft| draft.scale = value);
            }
        },
    );
    select(labels, selected).disabled(draft.off())
}

fn orientation(cx: &mut Cx, draft: Draft) -> impl View + use<> {
    let (listing, reading, editing) = (draft.clone(), draft.clone(), draft.clone());
    let labels = derived(cx, move |runtime| {
        orientations(listing.read(runtime, |draft| draft.transform))
            .iter()
            .map(|choice| choice.label().to_owned())
            .collect::<Vec<_>>()
    });
    let selected = bridge(
        cx,
        move |runtime| {
            let current = reading.read(runtime, |draft| draft.transform);
            orientations(current)
                .iter()
                .position(|choice| *choice == current)
        },
        move |runtime, index: Option<usize>| {
            let current = editing.read(runtime, |draft| draft.transform);
            if let Some(choice) = index.and_then(|index| orientations(current).get(index).copied())
            {
                editing.edit(runtime, |draft| draft.transform = choice);
            }
        },
    );
    select(labels, selected).disabled(draft.off())
}

/// Only offered for a monitor the compositor said something about adaptive
/// sync for — the protocol has no capability event, so its silence is how a
/// connector that cannot do it says so.
fn adaptive_sync_row(cx: &mut Cx, draft: Draft) -> impl View + use<> {
    let (reading, editing) = (draft.clone(), draft.clone());
    let on = bridge(
        cx,
        move |runtime| reading.read(runtime, |draft| draft.adaptive_sync),
        move |runtime, on| editing.edit(runtime, |draft| draft.adaptive_sync = on),
    );
    setting_row_desc(
        "Adaptive sync",
        "Match the refresh rate to what is being drawn",
        toggle(on).disabled(draft.off()),
    )
}

/// [`SCALES`], plus whatever the monitor is on when that is not one of them.
fn scales(current: f64) -> Vec<f64> {
    let mut scales = SCALES.to_vec();
    if !scales
        .iter()
        .any(|value| (value - current).abs() < SCALE_EPSILON)
    {
        scales.push(current);
        scales.sort_by(f64::total_cmp);
    }
    scales
}

/// [`Transform::CHOICES`], plus the mirrored orientation the monitor is on
/// when it is on one. They exist in the protocol but are not something a user
/// picks, so they are never offered — only kept.
fn orientations(current: Transform) -> Vec<Transform> {
    let mut choices = Transform::CHOICES.to_vec();
    if !choices.contains(&current) {
        choices.push(current);
    }
    choices
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_scale_the_page_does_not_offer_still_gets_a_row() {
        let scales = scales(1.3333);

        assert_eq!(scales.len(), SCALES.len() + 1);
        assert!(scales.contains(&1.3333));
        assert!(
            scales.windows(2).all(|pair| pair[0] < pair[1]),
            "the odd one out belongs in order, not at the end"
        );
    }

    #[test]
    fn an_offered_scale_is_not_listed_twice() {
        assert_eq!(scales(1.5), SCALES.to_vec());
    }

    #[test]
    fn a_mirrored_panel_keeps_its_orientation_without_offering_it() {
        assert_eq!(orientations(Transform::Normal), Transform::CHOICES.to_vec());
        assert_eq!(
            orientations(Transform::Flipped90).last(),
            Some(&Transform::Flipped90)
        );
    }
}
