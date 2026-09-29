//! Pieces every Cross-device view shares.

use crownuikit::config::theme;
use crownuikit::util::INTER;
use crownuikit::widgets::{ButtonVariant, Tone, button, status};
use xilem::masonry::core::ArcStr;
use xilem::masonry::properties::types::AsUnit;
use xilem::style::Style;
use xilem::view::{CrossAxisAlignment, FlexSequence, FlexSpacer, flex_row, label, prose};
use xilem::{FontWeight, WidgetView};

use crate::controls::value_text;
use crate::layout::{setting_row_content, settings_card, settings_card_titled};
use crate::state::Store;

/// Gap between the things side by side inside a row.
pub const INLINE_GAP: f64 = 10.0;
pub const ROW_SPINNER: f64 = 14.0;
pub const SECTION: &str = "Cross-device";

const HEADLINE_SIZE: f32 = 17.0;
const PARAGRAPH_SIZE: f32 = 13.0;

/// Things laid side by side on a row's line, vertically centred.
pub fn inline<Seq>(children: Seq) -> impl WidgetView<Store> + use<Seq>
where
    Seq: FlexSequence<Store> + Send + Sync + 'static,
{
    flex_row(children)
        .gap(INLINE_GAP.px())
        .cross_axis_alignment(CrossAxisAlignment::Center)
}

pub fn headline(text: impl Into<ArcStr>) -> impl WidgetView<Store> {
    label(text.into())
        .text_size(HEADLINE_SIZE)
        .weight(FontWeight::SEMI_BOLD)
        .font(INTER)
        .color(theme().text.primary)
}

/// Muted text that wraps instead of running off the card.
pub fn paragraph(text: impl Into<ArcStr>) -> impl WidgetView<Store> {
    prose(text.into())
        .text_size(PARAGRAPH_SIZE)
        .text_color(theme().text.muted)
}

/// A card that says one thing and offers nothing to press.
pub fn quiet_card<V>(leading: V, message: &'static str) -> impl WidgetView<Store> + use<V>
where
    V: WidgetView<Store>,
{
    settings_card_titled(
        SECTION,
        (setting_row_content(inline((leading, value_text(message)))),),
    )
}

/// The dismissible last failure.
pub fn error_card(message: String) -> impl WidgetView<Store> {
    settings_card((setting_row_content(inline((
        status(message).tone(Tone::Danger),
        FlexSpacer::Flex(1.0),
        button("Dismiss", |store: &mut Store| {
            store.cross_device.dismiss_error();
        })
        .variant(ButtonVariant::Ghost)
        .small(),
    ))),))
}
