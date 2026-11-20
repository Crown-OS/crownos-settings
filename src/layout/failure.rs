use crownui::kit::{ButtonOptions, ButtonSize, ButtonVariant, StatusOptions, Tone, button, status};
use crownui::prelude::{Listeners, Signal, View, spacer};

use super::{optional, setting_row_content, settings_card, spread};

/// A card that shows the message in `failure` while there is one, with a
/// button that clears it.
pub fn dismissible_failure(failure: Signal<Option<String>>) -> impl View {
    optional(
        move |runtime| failure.get(runtime),
        move |_, message| {
            settings_card((setting_row_content(spread((
                status(message).tone(Tone::Danger),
                spacer(),
                button("Dismiss")
                    .variant(ButtonVariant::Ghost)
                    .control_size(ButtonSize::Small)
                    .on_click(move |cx, _| failure.set(cx, None)),
            ))),))
        },
    )
}
