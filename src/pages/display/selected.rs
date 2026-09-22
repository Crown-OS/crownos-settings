//! Resolution, refresh, scale, orientation and adaptive sync, for the one
//! monitor the user picked out of the list.
//!
//! Every choice here is offered as a list of rows, and every one of those
//! lists has the same rule: whatever the monitor is *actually* on gets a row,
//! even when it is not something this page would have offered. A select with
//! nothing highlighted is how a user finds out their monitor is on a setting
//! the page cannot name, and then cannot get back to it.

use crownuikit::widgets::{select, toggle};
use xilem::WidgetView;

use crate::layout::{setting_row, setting_row_desc, settings_card_titled, settings_divider};
use crate::net::outputs::{MonitorDraft, Transform};
use crate::state::Store;

/// The scales offered. Quarter steps are what users recognise, and every one
/// is exactly representable in `wp_fractional_scale_v1`'s 120ths.
const SCALES: [f64; 9] = [1.0, 1.25, 1.5, 1.75, 2.0, 2.25, 2.5, 2.75, 3.0];

/// How close two scales have to be to count as the same one.
const SCALE_EPSILON: f64 = 0.001;

pub fn card(store: &Store) -> Option<impl WidgetView<Store> + use<>> {
    let outputs = &store.outputs;
    let name = outputs.selected.clone()?;
    let monitor = outputs.monitor(&name)?;
    let draft = outputs.draft(&name)?;

    Some(settings_card_titled(
        monitor.title(),
        (
            resolution_row(&name, monitor.modes.iter().map(|mode| mode.label()).collect(), draft),
            settings_divider(),
            scale_row(&name, draft),
            settings_divider(),
            orientation_row(&name, draft),
            monitor.vrr_capable.then(|| {
                (settings_divider(), adaptive_sync_row(&name, draft))
            }),
        ),
    ))
}

/// The mode list is per monitor, so its labels cannot be a static table the
/// way a schema enum's are.
fn resolution_row(
    name: &str,
    modes: Vec<String>,
    draft: &MonitorDraft,
) -> impl WidgetView<Store> + use<> {
    let name = name.to_owned();

    setting_row(
        "Resolution",
        select(modes, draft.mode, move |store: &mut Store, index| {
            store.outputs.edit(&name, |draft| draft.mode = Some(index));
        })
        .disabled(!draft.enabled),
    )
}

fn scale_row(name: &str, draft: &MonitorDraft) -> impl WidgetView<Store> + use<> {
    let name = name.to_owned();
    let scales = scales(draft.scale);
    let selected = scales
        .iter()
        .position(|value| (value - draft.scale).abs() < SCALE_EPSILON);
    let labels: Vec<String> = scales.iter().map(|value| format!("{:.0}%", value * 100.0)).collect();

    setting_row(
        "Scale",
        select(labels, selected, move |store: &mut Store, index| {
            let Some(value) = scales.get(index).copied() else {
                return;
            };
            store.outputs.edit(&name, |draft| draft.scale = value);
        })
        .disabled(!draft.enabled),
    )
}

fn orientation_row(name: &str, draft: &MonitorDraft) -> impl WidgetView<Store> + use<> {
    let name = name.to_owned();
    let choices = orientations(draft.transform);
    let selected = choices.iter().position(|choice| *choice == draft.transform);
    let labels: Vec<&'static str> = choices.iter().map(|choice| choice.label()).collect();

    setting_row(
        "Orientation",
        select(labels, selected, move |store: &mut Store, index| {
            let Some(choice) = choices.get(index).copied() else {
                return;
            };
            store.outputs.edit(&name, |draft| draft.transform = choice);
        })
        .disabled(!draft.enabled),
    )
}

/// Only offered for a monitor the compositor said something about adaptive
/// sync for — the protocol has no capability event, so its silence is how a
/// connector that cannot do it says so.
fn adaptive_sync_row(name: &str, draft: &MonitorDraft) -> impl WidgetView<Store> + use<> {
    let name = name.to_owned();

    setting_row_desc(
        "Adaptive sync",
        "Match the refresh rate to what is being drawn",
        toggle("", draft.adaptive_sync, move |store: &mut Store, on| {
            store.outputs.edit(&name, |draft| draft.adaptive_sync = on);
        })
        .disabled(!draft.enabled),
    )
}

/// [`SCALES`], plus whatever the monitor is on when that is not one of them.
fn scales(current: f64) -> Vec<f64> {
    let mut scales = SCALES.to_vec();
    if !scales.iter().any(|value| (value - current).abs() < SCALE_EPSILON) {
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
