//! Pieces every Android apps card shares.

use crownui::kit::{
    ButtonKind, ButtonOptions, ButtonSize, ButtonVariant, StatusOptions, Tone, button, status,
};
use crownui::prelude::{Element, Listeners, Signal, View, spacer};

use crate::layout::{setting_row_content, settings_card, spread};
use crate::net::android::AndroidState;

/// A small text button in `variant`, the size every row action is.
pub fn row_button(label: &'static str, variant: ButtonVariant) -> Element<ButtonKind> {
    button(label)
        .variant(variant)
        .control_size(ButtonSize::Small)
}

/// Runs `action` against the state when clicked.
pub fn acting(
    button: Element<ButtonKind>,
    state: Signal<AndroidState>,
    action: impl Fn(&mut AndroidState) + 'static,
) -> Element<ButtonKind> {
    button.on_click(move |cx, _| state.update(cx, &action))
}

/// The dismissible last failure.
pub fn error_card(state: Signal<AndroidState>, message: String) -> impl View {
    settings_card((setting_row_content(spread((
        status(message).tone(Tone::Danger),
        spacer(),
        acting(
            row_button("Dismiss", ButtonVariant::Ghost),
            state,
            AndroidState::dismiss_error,
        ),
    ))),))
}
