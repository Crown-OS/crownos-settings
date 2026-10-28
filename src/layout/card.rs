use crownui::kit::{card, divider, section};
use crownui::prelude::{Styled, View, ViewSeq};

/// A grouped card of setting rows, with [`settings_divider`]s placed between
/// them by the caller.
pub fn settings_card(rows: impl ViewSeq + 'static) -> impl View {
    card(rows).self_stretch()
}

/// [`settings_card`] under a small uppercase, muted section label — the
/// "General", "Network", … headers macOS System Settings prints above each
/// card.
pub fn settings_card_titled<R: ViewSeq + 'static>(title: &str, rows: R) -> impl View + use<R> {
    section(title.to_uppercase(), rows)
}

/// Hairline between rows inside a card: full width, 1px, no extra breathing
/// room (the rows already carry their own vertical padding).
pub fn settings_divider() -> impl View {
    divider()
}
