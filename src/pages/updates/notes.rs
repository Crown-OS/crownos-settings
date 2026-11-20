//! Release notes, drawn from their Markdown blocks.

use crownui::kit::{INTER_FAMILY, TEXT_SIZE, use_theme};
use crownui::prelude::{AnyView, Styled, View, component, hstack, text, vstack};

use crate::layout::{headline, paragraph, settings_divider};
use crate::util::markdown::Block;
use crate::util::theme::tint;

const BLOCK_GAP: f32 = 8.0;
const INDENT_PER_DEPTH: f32 = 16.0;
const MARKER_GAP: f32 = 6.0;
const CODE_SIZE: f32 = 12.0;
const CODE_FAMILY: &str = "monospace";
const QUOTE_RULE_WIDTH: f32 = 2.0;
const QUOTE_GAP: f32 = 10.0;
const LARGEST_HEADLINE_LEVEL: u8 = 2;

pub fn notes_view(blocks: Vec<Block>) -> impl View {
    let views: Vec<AnyView> = blocks.into_iter().map(block_view).collect();
    vstack(views).gap(BLOCK_GAP).self_stretch()
}

fn block_view(block: Block) -> AnyView {
    match block {
        Block::Heading { level, text } if level <= LARGEST_HEADLINE_LEVEL => headline(text).boxed(),
        Block::Heading { text, .. } => subheading(text).boxed(),
        Block::Paragraph(text) => paragraph(text).boxed(),
        Block::Item {
            depth,
            marker,
            text,
        } => hstack((paragraph(marker), wrapping(text)))
            .gap(MARKER_GAP)
            .pl(depth as f32 * INDENT_PER_DEPTH)
            .boxed(),
        Block::Code(code) => code_text(code).boxed(),
        Block::Quote(text) => quote(text).boxed(),
        Block::Rule => settings_divider().boxed(),
    }
}

fn wrapping(content: String) -> impl View {
    vstack(paragraph(content)).flex_1()
}

fn subheading(content: String) -> impl View {
    component(move |cx| {
        let theme = use_theme(cx);
        text(content)
            .text_size(TEXT_SIZE)
            .font_semibold()
            .font_family(INTER_FAMILY)
            .text_color(tint(theme, |theme| theme.text.body))
    })
}

fn code_text(code: String) -> impl View {
    component(move |cx| {
        let theme = use_theme(cx);
        text(code)
            .text_size(CODE_SIZE)
            .font_family(CODE_FAMILY)
            .text_color(tint(theme, |theme| theme.text.muted))
    })
}

fn quote(content: String) -> impl View {
    component(move |cx| {
        let theme = use_theme(cx);
        hstack((
            vstack(())
                .w(QUOTE_RULE_WIDTH)
                .self_stretch()
                .bg(tint(theme, |theme| theme.surface.border)),
            wrapping(content),
        ))
        .gap(QUOTE_GAP)
    })
}
