//! What the page shows before anything is paired.

use blinc_icons::icons;
use crownuikit::config::theme;
use crownuikit::widgets::{ButtonVariant, button, icon};
use xilem::WidgetView;
use xilem::masonry::properties::types::AsUnit;
use xilem::view::{CrossAxisAlignment, FlexExt, flex_col};

use super::describe::feature_copy;
use super::parts::{INLINE_GAP, SECTION, headline, inline, paragraph};
use crate::controls::value_text;
use crate::layout::{
    setting_row_content, setting_row_icon, settings_card_titled, settings_divider,
};
use crate::net::crownconnect::Feature;
use crate::pages::PageView;
use crate::state::Store;

const HERO_GLYPH: f64 = 40.0;

/// The capabilities worth naming to someone who has not paired yet.
const HIGHLIGHTS: [Feature; 4] = [
    Feature::Mirror,
    Feature::Clipboard,
    Feature::Calls,
    Feature::Unicursor,
];

pub fn card() -> impl WidgetView<Store> {
    let hero = inline((
        icon(icons::TABLET_SMARTPHONE)
            .size(HERO_GLYPH)
            .color(theme().accent.end),
        flex_col((
            headline("Connect your first device"),
            paragraph("Pair a phone, tablet or another computer to use it together with this one."),
        ))
        .gap(INLINE_GAP.px())
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .flex(1.0),
    ));

    let mut rows: Vec<PageView> = vec![setting_row_content(hero).boxed()];
    for feature in HIGHLIGHTS {
        let copy = feature_copy(feature);
        rows.push(settings_divider().boxed());
        rows.push(setting_row_icon(copy.glyph, copy.description, value_text("")).boxed());
    }

    let connect = button("Connect a Device…", |store: &mut Store| {
        store.cross_device.begin_pairing();
    })
    .variant(ButtonVariant::Primary)
    .leading_icon(icons::QR_CODE);

    rows.push(settings_divider().boxed());
    rows.push(setting_row_content(connect).boxed());
    settings_card_titled(SECTION, rows)
}
