use crownui::kit::{INTER_FAMILY, icon_colored, use_theme};
use crownui::prelude::{
    Semantic, Styled, View, ViewSeq, component, hstack, scroll_view, text, vstack,
};

use crate::util::theme::tint;

/// Padding around the whole scrollable page.
const PAGE_PADDING: f32 = 32.0;
/// Gap between the page header and the first card.
const PAGE_TITLE_GAP: f32 = 24.0;
/// Breathing room between a page's cards.
const CARD_GAP: f32 = 24.0;
const PAGE_TITLE_SIZE: f32 = 22.0;
/// Gap between the header's icon chip and the title.
const PAGE_ICON_GAP: f32 = 12.0;
/// The chip's footprint: a rounded square, matching the kit's +2px-rounder
/// corners rather than a pill.
const PAGE_ICON_CHIP_SIZE: f32 = 34.0;
const PAGE_ICON_CHIP_RADIUS: f32 = 10.0;
const PAGE_ICON_SIZE: f32 = 18.0;
const PAGE_ICON_STROKE: f32 = 2.0;
/// How much of the accent the chip's wash carries — the badge's convention,
/// so a page header and a badge read as one family.
const PAGE_ICON_TINT_ALPHA: f32 = 0.14;

/// The frame every page shares: an accent icon chip and title over a stack of
/// cards, padded inside a scroll region that fills the content pane.
///
/// Takes the icon a page's sidebar row also wears, so the two cannot drift.
pub fn settings_page(
    title: impl Into<String>,
    icon: &'static str,
    cards: impl ViewSeq + 'static,
) -> impl View {
    let title = title.into();
    component(move |cx| {
        let theme = use_theme(cx);
        let chip = vstack(
            icon_colored(icon, PAGE_ICON_STROKE, |theme| theme.accent.end).size(PAGE_ICON_SIZE),
        )
        .size(PAGE_ICON_CHIP_SIZE)
        .items_center()
        .justify_center()
        .rounded(PAGE_ICON_CHIP_RADIUS)
        .bg(tint(theme, |theme| {
            theme.accent.end.with_alpha_factor(PAGE_ICON_TINT_ALPHA)
        }));
        let header = hstack((
            chip,
            text(title.clone())
                .text_size(PAGE_TITLE_SIZE)
                .font_bold()
                .font_family(INTER_FAMILY)
                .text_color(tint(theme, |theme| theme.text.primary)),
        ))
        .gap(PAGE_ICON_GAP)
        .items_center();

        scroll_view(
            vstack((header, card_stack(cards)))
                .gap(PAGE_TITLE_GAP)
                .p(PAGE_PADDING),
        )
        .flex_1()
        .label(title)
    })
}

/// Cards stacked with the gap a page puts between them, for a branch of a page
/// that holds more than one.
pub fn card_stack(cards: impl ViewSeq) -> impl View {
    vstack(cards).gap(CARD_GAP)
}
