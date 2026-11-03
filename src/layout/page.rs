use crownui::prelude::{Semantic, Styled, View, ViewSeq, scroll_view, vstack};

/// Padding around the whole scrollable page.
const PAGE_PADDING: f32 = 32.0;
/// Breathing room between a page's cards.
const CARD_GAP: f32 = 24.0;

/// The frame every page shares: a stack of cards, padded inside a scroll
/// region that fills the content pane and is named after the page.
pub fn settings_page(title: impl Into<String>, cards: impl ViewSeq + 'static) -> impl View {
    scroll_view(vstack(cards).gap(CARD_GAP).p(PAGE_PADDING))
        .flex_1()
        .label(title.into())
}

/// Cards stacked with the gap a page puts between them, for a branch of a page
/// that holds more than one.
pub fn card_stack(cards: impl ViewSeq) -> impl View {
    vstack(cards).gap(CARD_GAP)
}
