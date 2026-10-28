use crownui::kit::{INTER_FAMILY, use_theme};
use crownui::prelude::{IntoProp, Styled, View, ViewSeq, component, hstack, text};

use crate::util::theme::tint;

/// Gap between the things side by side inside a row.
pub const INLINE_GAP: f32 = 10.0;
const VALUE_TEXT_SIZE: f32 = 16.0;
const HEADLINE_SIZE: f32 = 17.0;
const PARAGRAPH_SIZE: f32 = 13.0;

/// A read-only row value, in the muted tone a description uses — which is
/// what makes it read as "informational, not editable".
pub fn value_text(content: impl IntoProp<String> + 'static) -> impl View {
    component(move |cx| {
        let theme = use_theme(cx);
        text(content)
            .text_size(VALUE_TEXT_SIZE)
            .font_family(INTER_FAMILY)
            .text_color(tint(theme, |theme| theme.text.muted))
    })
}

/// A card's lead sentence.
pub fn headline(content: impl IntoProp<String> + 'static) -> impl View {
    component(move |cx| {
        let theme = use_theme(cx);
        text(content)
            .text_size(HEADLINE_SIZE)
            .font_semibold()
            .font_family(INTER_FAMILY)
            .text_color(tint(theme, |theme| theme.text.primary))
    })
}

/// Muted text that wraps instead of running off the card.
pub fn paragraph(content: impl IntoProp<String> + 'static) -> impl View {
    component(move |cx| {
        let theme = use_theme(cx);
        text(content)
            .text_size(PARAGRAPH_SIZE)
            .font_family(INTER_FAMILY)
            .text_color(tint(theme, |theme| theme.text.muted))
    })
}

/// Things laid side by side on a row's line, vertically centred.
pub fn inline(children: impl ViewSeq + 'static) -> impl View {
    hstack(children).gap(INLINE_GAP).items_center()
}

/// [`inline`] across the whole row, so a `spacer()` inside pushes what
/// follows it to the trailing edge.
pub fn spread(children: impl ViewSeq + 'static) -> impl View {
    hstack(children)
        .gap(INLINE_GAP)
        .items_center()
        .self_stretch()
}
