//! The buttons that send a verb to the daemon, and why one is disabled.

use crownui::kit::{ButtonKind, ButtonOptions, ButtonVariant, button};
use crownui::prelude::{Element, IntoProp, Listeners, Runtime, Semantic, Signal, View, bind};

use crate::layout::paragraph;
use crate::net::updates::{FailedStep, UpdatesCommand, UpdatesState, Verb};

/// A button that sends `command`, disabled while the daemon would refuse it.
pub fn verb_button(
    label: impl IntoProp<String> + 'static,
    variant: ButtonVariant,
    updates: Signal<UpdatesState>,
    command: UpdatesCommand,
) -> Element<ButtonKind> {
    let verb = command.verb();
    button(label)
        .variant(variant)
        .disabled(bind(move |runtime| {
            updates.with(runtime, |state| state.refuses(verb))
        }))
        .on_click(move |cx, _| updates.update(cx, |state| state.request(command)))
}

/// Why the daemon refuses `verb` while no step runs; the progress card
/// already tells about a running step.
pub fn refusal_reason(updates: Signal<UpdatesState>, verb: Verb) -> impl View {
    paragraph(bind(move |runtime: &mut Runtime| {
        updates.with(runtime, |state| {
            state
                .snapshot
                .as_ref()
                .filter(|snapshot| snapshot.is_idle())
                .and_then(|snapshot| snapshot.refusal(verb))
                .unwrap_or_default()
                .to_owned()
        })
    }))
}

/// "Update now", or "Continue update" while an interrupted update waits.
pub fn apply_label(state: &UpdatesState) -> String {
    let label = if state.continues_update() {
        "Continue update"
    } else {
        "Update now"
    };
    label.to_owned()
}

/// The command that tries the failed step again: the restart that is still
/// owed, the interrupted update, the failed install or download, or a new
/// check.
pub fn retry_command(state: &UpdatesState) -> (UpdatesCommand, &'static str) {
    const TRY_AGAIN: &str = "Try again";
    let Some(snapshot) = state.snapshot.as_ref() else {
        return (UpdatesCommand::Check, TRY_AGAIN);
    };
    if let Some(pending) = &snapshot.pending_restart {
        return (UpdatesCommand::Restart(pending.kind), TRY_AGAIN);
    }
    if snapshot.in_progress.is_some() {
        return (UpdatesCommand::Apply, "Continue update");
    }
    match snapshot.failure.as_ref().map(|failure| failure.step) {
        Some(FailedStep::Apply(_)) if snapshot.next.is_some() => (UpdatesCommand::Apply, TRY_AGAIN),
        Some(FailedStep::Download) => (UpdatesCommand::Download, TRY_AGAIN),
        _ => (UpdatesCommand::Check, TRY_AGAIN),
    }
}
