//! Pieces every Cross-device view shares.

use crownui::kit::{
    ButtonKind, ButtonOptions, ButtonSize, ButtonVariant, StatusOptions, Tone, button, status,
};
use crownui::prelude::{Element, Listeners, Signal, View, spacer};

use crate::layout::{
    inline, setting_row_content, settings_card, settings_card_titled, spread, value_text,
};
use crate::net::crownconnect::CrossDeviceState;

pub const ROW_SPINNER: f32 = 14.0;
pub const SECTION: &str = "Cross-device";

/// A card that says one thing and offers nothing to press.
pub fn quiet_card(leading: impl View, message: &'static str) -> impl View {
    settings_card_titled(
        SECTION,
        (setting_row_content(inline((leading, value_text(message)))),),
    )
}

/// A small text button in `variant`, the size every row action is.
pub fn row_button(label: &'static str, variant: ButtonVariant) -> Element<ButtonKind> {
    button(label)
        .variant(variant)
        .control_size(ButtonSize::Small)
}

/// Runs `action` against the state when clicked.
pub fn acting(
    button: Element<ButtonKind>,
    state: Signal<CrossDeviceState>,
    action: impl Fn(&mut CrossDeviceState) + 'static,
) -> Element<ButtonKind> {
    button.on_click(move |cx, _| state.update(cx, &action))
}

/// The dismissible last failure.
pub fn error_card(state: Signal<CrossDeviceState>, message: String) -> impl View {
    settings_card((setting_row_content(spread((
        status(message).tone(Tone::Danger),
        spacer(),
        acting(
            row_button("Dismiss", ButtonVariant::Ghost),
            state,
            CrossDeviceState::dismiss_error,
        ),
    ))),))
}
