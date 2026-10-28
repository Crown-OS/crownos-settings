//! Pieces every Wi-Fi card shares.

use crownui::kit::{
    ButtonKind, ButtonOptions, ButtonSize, ButtonVariant, StatusOptions, Tone, button, icon,
    icon_colored, icon_muted, icons, status,
};
use crownui::prelude::{Element, Listeners, Runtime, Signal, Styled, View, hstack, keyed, spacer};

use crate::layout::{
    INLINE_GAP, setting_row_content, settings_card, settings_card_titled, value_text,
};
use crate::net::wifi::{ActiveWifi, WifiCommand, WifiState};

/// A spinner sized to sit on a row's baseline without changing its height.
pub const ROW_SPINNER: f32 = 14.0;
/// Trailing lock and signal glyphs. A step under the row's own leading icon,
/// because they annotate the name rather than introduce it.
const TRAILING_ICON: f32 = 15.0;
const ICON_STROKE: f32 = 2.0;

/// Signal strength percentages at which the wifi glyph gains another arc.
/// Four glyphs, so three thresholds; `WIFI_ZERO` is what is left below.
const SIGNAL_TIERS: [(u8, &str); 3] = [
    (75, icons::WIFI),
    (50, icons::WIFI_HIGH),
    (25, icons::WIFI_LOW),
];

/// Which of the four wifi glyphs a strength percentage earns.
pub fn signal_glyph(strength: u8) -> &'static str {
    SIGNAL_TIERS
        .iter()
        .find(|(floor, _)| strength >= *floor)
        .map_or(icons::WIFI_ZERO, |(_, glyph)| *glyph)
}

/// The signal glyph for whatever strength `strength` derives.
pub fn signal_icon(strength: impl FnMut(&mut Runtime) -> u8 + 'static) -> impl View {
    let mut strength = strength;
    keyed(
        move |runtime| signal_glyph(strength(runtime)),
        |_, glyph| icon(glyph).size(TRAILING_ICON),
    )
}

/// The padlock a secured network wears, tinted when its security is one of
/// the discouraged ones — the warning is about the lock, not the signal.
pub fn lock_icon(weak: impl FnMut(&mut Runtime) -> bool + 'static) -> impl View {
    keyed(weak, |_, weak| {
        if weak {
            icon_colored(icons::LOCK, ICON_STROKE, |theme| theme.status.warning).size(TRAILING_ICON)
        } else {
            icon_muted(icons::LOCK).size(TRAILING_ICON)
        }
    })
}

/// A muted decoration for the cards that have nothing to report.
pub fn muted_icon(glyph: &'static str) -> impl View {
    icon_muted(glyph).size(TRAILING_ICON)
}

/// A small text button in `variant`, the size every row action is.
pub fn row_button(label: &'static str, variant: ButtonVariant) -> Element<ButtonKind> {
    button(label)
        .variant(variant)
        .control_size(ButtonSize::Small)
}

/// Reads the network this machine is on, if any.
pub fn with_active<R>(
    runtime: &mut Runtime,
    wifi: Signal<WifiState>,
    read: impl FnOnce(&ActiveWifi) -> R,
) -> Option<R> {
    wifi.with(runtime, |wifi| {
        wifi.snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.connection.as_ref())
            .map(read)
    })
}

/// Asks the worker to do something.
pub fn send(runtime: &mut Runtime, wifi: Signal<WifiState>, command: WifiCommand) {
    wifi.update(runtime, |wifi| wifi.send(command));
}

/// A card that says one thing and offers nothing to press.
pub fn quiet_card(leading: impl View, message: String) -> impl View {
    settings_card_titled(
        "Wi-Fi",
        (setting_row_content(
            hstack((leading, value_text(message)))
                .gap(INLINE_GAP)
                .items_center(),
        ),),
    )
}

/// The dismissible last failure.
///
/// A card of its own rather than a line inside another one, because the error
/// outlives whichever row caused it — the network it names may already have
/// gone out of range by the time it is read.
pub fn error_card(wifi: Signal<WifiState>, message: String) -> impl View {
    settings_card((setting_row_content(
        hstack((
            status(message).tone(Tone::Danger),
            spacer(),
            row_button("Dismiss", ButtonVariant::Ghost)
                .on_click(move |cx, _| wifi.update(cx, WifiState::dismiss_error)),
        ))
        .gap(INLINE_GAP)
        .items_center()
        .self_stretch(),
    ),))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An off-by-one in the ladder is invisible until a network sits exactly
    /// on a boundary.
    #[test]
    fn every_strength_lands_on_a_glyph() {
        assert_eq!(signal_glyph(100), icons::WIFI);
        assert_eq!(signal_glyph(75), icons::WIFI);
        assert_eq!(signal_glyph(74), icons::WIFI_HIGH);
        assert_eq!(signal_glyph(50), icons::WIFI_HIGH);
        assert_eq!(signal_glyph(49), icons::WIFI_LOW);
        assert_eq!(signal_glyph(25), icons::WIFI_LOW);
        assert_eq!(signal_glyph(24), icons::WIFI_ZERO);
        assert_eq!(signal_glyph(0), icons::WIFI_ZERO);
    }
}
