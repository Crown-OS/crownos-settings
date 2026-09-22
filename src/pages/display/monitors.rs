//! The monitor list, and the two ways an arrangement leaves this page.
//!
//! Applying is a two-step — see [`crate::net::outputs`] — so there are two
//! action rows and never both at once: one offers to apply what has been
//! drafted, the other asks whether to keep what was applied.

use crownuikit::widgets::{button, toggle};
use xilem::WidgetView;
use xilem::view::flex_row;

use crate::controls::value_text;
use crate::layout::{setting_row_content, setting_row_desc, settings_card_titled};
use crate::net::outputs::{Monitor, MonitorDraft, REVERT_AFTER};
use crate::pages::display::status_row;
use crate::state::Store;

/// The monitor list, plus whatever has to be said about the whole arrangement.
pub fn card(store: &Store) -> impl WidgetView<Store> + use<> {
    let outputs = &store.outputs;

    let status = match (&outputs.unavailable, outputs.loaded) {
        (Some(reason), _) => Some(reason.clone()),
        (None, false) => Some("Looking for monitors…".to_owned()),
        (None, true) if outputs.monitors.is_empty() => {
            Some("No monitors are connected.".to_owned())
        }
        (None, true) => None,
    };

    let rows: Vec<_> = outputs
        .monitors
        .iter()
        .map(|monitor| row(store, monitor))
        .collect();

    settings_card_titled(
        "Monitors",
        (
            status.map(status_row),
            rows,
            outputs.error.clone().map(status_row),
            confirm_actions(store),
            apply_actions(store),
        ),
    )
}

/// One monitor: what it is, what it is about to become, and whether it is on.
///
/// The summary is read off the *draft*, not the monitor, so an edit made in
/// the card below shows up here before it is applied.
fn row(store: &Store, monitor: &Monitor) -> impl WidgetView<Store> + use<> {
    let name = monitor.name.clone();
    let selected = store.outputs.selected.as_deref() == Some(name.as_str());
    let draft = store.outputs.draft(&name);
    let enabled = draft.is_none_or(|draft| draft.enabled);

    let (toggle_name, select_name) = (name.clone(), name.clone());

    setting_row_desc(
        monitor.title(),
        summary(monitor, draft),
        flex_row((
            button(
                if selected { "Selected" } else { "Settings" },
                move |store: &mut Store| {
                    store.outputs.selected = Some(select_name.clone());
                },
            ),
            toggle("", enabled, move |store: &mut Store, on| {
                store.outputs.edit(&toggle_name, |draft| draft.enabled = on);
            }),
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

    format!(
        "{} at {:.0}%",
        mode.label(),
        draft.scale * 100.0
    )
}

/// "Keep these display settings?", shown while a countdown is running.
///
/// Two `Option`s rather than one, because the two rows are different widget
/// trees and `impl Trait` is one concrete type. At most one is ever `Some`.
fn confirm_actions(store: &Store) -> Option<impl WidgetView<Store> + use<>> {
    store.outputs.pending_revert.as_ref()?;

    // The window is named rather than counted down: a live number would need
    // the page to repaint every second, and what the user has to know is that
    // doing nothing is safe, not how many seconds are left.
    let prompt = format!(
        "Keep these display settings? They go back in {} seconds.",
        REVERT_AFTER.as_secs()
    );

    Some(setting_row_content(flex_row((
        value_text(prompt),
        button("Keep", |store: &mut Store| store.outputs.confirm()),
        button("Revert", |store: &mut Store| store.outputs.revert()),
    ))))
}

fn apply_actions(store: &Store) -> Option<impl WidgetView<Store> + use<>> {
    // Hidden while a confirmation is outstanding: applying on top of an
    // unconfirmed change would leave nothing sane to revert to.
    if store.outputs.pending_revert.is_some() || !store.outputs.is_dirty() {
        return None;
    }

    Some(setting_row_content(flex_row((
        value_text("Unapplied changes"),
        button("Apply", |store: &mut Store| store.outputs.apply()),
        button("Discard", |store: &mut Store| store.outputs.reset_drafts()),
    ))))
}
