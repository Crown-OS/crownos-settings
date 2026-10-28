//! Pieces every plugin view shares.

use crownui::kit::{
    ButtonKind, ButtonOptions, ButtonSize, ButtonVariant, StatusOptions, Tone, button, status,
};
use crownui::prelude::{Element, IntoProp, Listeners, Signal, View, spacer};

use crate::layout::{setting_row_content, settings_card, spread};
use crate::net::plugins::{EntityInfo, EntityStatus, PluginsState};

/// A small text button in `variant`, the size every row action is.
pub fn row_button(label: impl IntoProp<String>, variant: ButtonVariant) -> Element<ButtonKind> {
    button(label)
        .variant(variant)
        .control_size(ButtonSize::Small)
}

/// Runs `action` against the state when clicked.
pub fn acting(
    button: Element<ButtonKind>,
    state: Signal<PluginsState>,
    action: impl Fn(&mut PluginsState) + 'static,
) -> Element<ButtonKind> {
    button.on_click(move |cx, _| state.update(cx, &action))
}

/// The dismissible last failure.
pub fn error_card(state: Signal<PluginsState>, message: String) -> impl View {
    settings_card((setting_row_content(spread((
        status(message).tone(Tone::Danger),
        spacer(),
        acting(
            row_button("Dismiss", ButtonVariant::Ghost),
            state,
            PluginsState::dismiss_error,
        ),
    ))),))
}

/// One line on what an entity is doing, for its row and its detail view.
pub fn status_line(entity: &EntityInfo) -> String {
    match &entity.status {
        EntityStatus::Quarantined(message) => format!("Stopped after repeated crashes: {message}"),
        EntityStatus::Error(message) => format!("Error: {message}"),
        EntityStatus::Running => format!("v{} · Running", entity.version),
        EntityStatus::Inactive => format!("v{} · {}", entity.version, entity.author),
    }
}

/// Whether the entity is in a state the user should look at.
pub fn troubled(entity: &EntityInfo) -> bool {
    matches!(
        entity.status,
        EntityStatus::Quarantined(_) | EntityStatus::Error(_)
    )
}
