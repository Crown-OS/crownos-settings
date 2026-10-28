//! What the page shows before anything is paired.

use crownui::kit::{ButtonOptions, ButtonVariant, button, icon_colored, icons};
use crownui::prelude::{Signal, Styled, View, vstack};

use super::describe::feature_copy;
use super::parts::{SECTION, acting};
use crate::layout::{
    INLINE_GAP, glyph, headline, inline, paragraph, setting_row_content, setting_row_icon,
    settings_card_titled, settings_divider, value_text,
};
use crate::net::crownconnect::{CrossDeviceState, Feature};

const HERO_GLYPH: f32 = 40.0;
const HERO_STROKE: f32 = 2.0;

/// The capabilities worth naming to someone who has not paired yet.
const HIGHLIGHTS: [Feature; 4] = [
    Feature::Mirror,
    Feature::Clipboard,
    Feature::Calls,
    Feature::Unicursor,
];

pub fn card(state: Signal<CrossDeviceState>) -> impl View {
    let hero = inline((
        icon_colored(icons::TABLET_SMARTPHONE, HERO_STROKE, |theme| {
            theme.accent.end
        })
        .size(HERO_GLYPH),
        vstack((
            headline("Connect your first device"),
            paragraph("Pair a phone, tablet or another computer to use it together with this one."),
        ))
        .gap(INLINE_GAP)
        .flex_1(),
    ));

    let highlights: Vec<_> = HIGHLIGHTS
        .into_iter()
        .map(|feature| {
            let copy = feature_copy(feature);
            vstack((
                settings_divider(),
                setting_row_icon(glyph(copy.glyph), copy.description, value_text("")),
            ))
        })
        .collect();

    let connect = acting(
        button("Connect a Device…")
            .variant(ButtonVariant::Primary)
            .leading_icon(icons::QR_CODE),
        state,
        CrossDeviceState::begin_pairing,
    );

    settings_card_titled(
        SECTION,
        (
            setting_row_content(hero),
            highlights,
            settings_divider(),
            setting_row_content(connect),
        ),
    )
}
