//! The monitor list, and the two ways an arrangement leaves this page.
//!
//! Applying is a two-step — see [`crate::net::outputs`] — so there are two
//! action rows and never both at once: one offers to apply what has been
//! drafted, the other asks whether to keep what was applied.

use crownui::kit::{button, toggle};
use crownui::prelude::{Cx, Listeners, Signal, View, bind, for_each, hstack};

use super::status_row;
use crate::layout::{
    optional, setting_row_content, setting_row_desc, settings_card_titled, value_text, when,
};
use crate::net::outputs::{Monitor, MonitorDraft, OutputState, REVERT_AFTER};
use crate::state::Store;
use crate::util::bridge::{bridge, derived};

/// The monitor list, plus whatever has to be said about the whole arrangement.
pub fn card(cx: &mut Cx, store: Store) -> impl View + use<> {
    let outputs = store.outputs;
    let monitors = derived(cx, move |runtime| {
        outputs.with(runtime, |outputs| outputs.monitors.clone())
    });

    settings_card_titled(
        "Monitors",
        (
            optional(
                move |runtime| outputs.with(runtime, arrangement_status),
                |_, status| status_row(status),
            ),
            for_each(
                monitors,
                |monitor: &Monitor| monitor.name.clone(),
                move |cx, monitor| row(cx, outputs, monitor),
            ),
            optional(
                move |runtime| outputs.with(runtime, |outputs| outputs.error.clone()),
                |_, error| status_row(error),
            ),
            confirm_actions(outputs),
            apply_actions(outputs),
        ),
    )
}

/// What has to be said about the arrangement as a whole, if anything.
fn arrangement_status(outputs: &OutputState) -> Option<String> {
    match (&outputs.unavailable, outputs.loaded) {
        (Some(reason), _) => Some(reason.clone()),
        (None, false) => Some("Looking for monitors…".to_owned()),
        (None, true) if outputs.monitors.is_empty() => {
            Some("No monitors are connected.".to_owned())
        }
        (None, true) => None,
    }
}

/// One monitor: what it is, what it is about to become, and whether it is on.
///
/// The summary is read off the *draft*, not the monitor, so an edit made in
/// the card below shows up here before it is applied.
fn row(cx: &mut Cx, outputs: Signal<OutputState>, monitor: Signal<Monitor>) -> impl View + use<> {
    let name = monitor.with_untracked(cx, |monitor| monitor.name.clone());

    let (toggled, selecting, labelled) = (name.clone(), name.clone(), name.clone());
    let enabled = bridge(
        cx,
        move |runtime| {
            outputs.with(runtime, |outputs| {
                outputs.draft(&name).is_none_or(|draft| draft.enabled)
            })
        },
        move |runtime, on| {
            outputs.update(runtime, |outputs| {
                outputs.edit(&toggled, |draft| draft.enabled = on)
            })
        },
    );
    let is_selected = move |runtime: &mut _| {
        outputs.with(runtime, |outputs| {
            outputs.selected.as_deref() == Some(labelled.as_str())
        })
    };
    let summary_name = selecting.clone();

    setting_row_desc(
        bind(move |runtime| monitor.with(runtime, Monitor::title)),
        bind(move |runtime| {
            let current = monitor.with(runtime, Clone::clone);
            outputs.with(runtime, |outputs| {
                summary(&current, outputs.draft(&summary_name))
            })
        }),
        hstack((
            button(bind(move |runtime| {
                if is_selected(runtime) {
                    "Selected"
                } else {
                    "Settings"
                }
                .to_owned()
            }))
            .on_click(move |cx, _| {
                outputs.update(cx, |outputs| outputs.selected = Some(selecting.clone()));
            }),
            toggle(enabled),
        )),
    )
}

fn summary(monitor: &Monitor, draft: Option<&MonitorDraft>) -> String {
    let Some(draft) = draft.filter(|draft| draft.enabled) else {
        return "Off".to_owned();
    };
    let Some(mode) = draft.mode.and_then(|index| monitor.modes.get(index)) else {
        return "On".to_owned();
    };
    format!("{} at {:.0}%", mode.label(), draft.scale * 100.0)
}

/// "Keep these display settings?", shown while a countdown is running.
fn confirm_actions(outputs: Signal<OutputState>) -> impl View {
    when(
        move |runtime| outputs.with(runtime, |outputs| outputs.pending_revert.is_some()),
        move |_| {
            // The window is named rather than counted down: what the user has
            // to know is that doing nothing is safe, not how many seconds are
            // left.
            let prompt = format!(
                "Keep these display settings? They go back in {} seconds.",
                REVERT_AFTER.as_secs()
            );
            setting_row_content(hstack((
                value_text(prompt),
                button("Keep").on_click(move |cx, _| outputs.update(cx, OutputState::confirm)),
                button("Revert").on_click(move |cx, _| outputs.update(cx, OutputState::revert)),
            )))
        },
    )
}

/// Hidden while a confirmation is outstanding: applying on top of an
/// unconfirmed change would leave nothing sane to revert to.
fn apply_actions(outputs: Signal<OutputState>) -> impl View {
    when(
        move |runtime| {
            outputs.with(runtime, |outputs| {
                outputs.pending_revert.is_none() && outputs.is_dirty()
            })
        },
        move |_| {
            setting_row_content(hstack((
                value_text("Unapplied changes"),
                button("Apply").on_click(move |cx, _| outputs.update(cx, OutputState::apply)),
                button("Discard")
                    .on_click(move |cx, _| outputs.update(cx, OutputState::reset_drafts)),
            )))
        },
    )
}
