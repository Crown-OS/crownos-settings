//! The Clock & colour sheet, after ColorOS's: the three layouts and the
//! three fonts, each drawn by the lock screen's own code, a weight slider
//! from a thin "A" to a bold one, and a slider from frosted glass to solid
//! white.

mod tile;

use crownos_config::schema::ClockLook;
use crownui::kit::{INTER_FAMILY, SliderOptions, slider, use_theme};
use crownui::prelude::{Color, Cx, Semantic, Styled, View, WHITE, component, hstack, text, vstack};

use super::clock_look::look_value;
use super::preview::{FONTS, LAYOUTS, Preview};
use crate::layout::{setting_row_content, spread};
use crate::state::Store;
use crate::util::theme::tint;
use tile::tile;

const COLUMNS: u16 = 3;
const TILE_GAP: f32 = 12.0;
const FONT_TILE_HEIGHT: f32 = 44.0;
const LETTER_SIZE: f32 = 20.0;
const THIN: u16 = 100;
const BOLD: u16 = 900;
const SWATCH_SIZE: f32 = 18.0;
/// Logical pixels each slider end takes.
const END_WIDTH: f32 = 24.0;
const SWATCH_RING: f32 = 1.5;
/// The frosted end of the whiteness slider: a pane of clear glass.
const FROST: Color = Color::new(1.0, 1.0, 1.0, 0.18);

pub fn clock_sheet(cx: &mut Cx, store: Store, preview: &Preview) -> impl View + use<> {
    let layouts: Vec<_> = LAYOUTS
        .into_iter()
        .enumerate()
        .map(|(index, (layout, name))| {
            tile(
                store,
                preview.layout_picture(index),
                format!("{name} layout"),
                move |look| look.layout == layout,
                move |look| look.layout = layout,
            )
        })
        .collect();
    let fonts: Vec<_> = FONTS
        .into_iter()
        .enumerate()
        .map(|(index, (font, name))| {
            tile(
                store,
                preview.font_picture(index, FONT_TILE_HEIGHT),
                format!("{name} font"),
                move |look| look.font == font,
                move |look| look.font = font,
            )
        })
        .collect();
    let weight = look_value(cx, store, ClockLook::clamped_weight, |look, weight| {
        look.weight = weight
    });
    let whiteness = look_value(
        cx,
        store,
        ClockLook::clamped_whiteness,
        |look, whiteness| look.whiteness = whiteness,
    );
    vstack((
        setting_row_content(grid(layouts)),
        setting_row_content(grid(fonts)),
        setting_row_content(spread((
            end(letter(THIN)),
            slider(weight)
                .range(ClockLook::MIN_WEIGHT, ClockLook::MAX_WEIGHT)
                .flex_1()
                .label("Weight"),
            end(letter(BOLD)),
        ))),
        setting_row_content(spread((
            end(swatch(FROST)),
            slider(whiteness)
                .range(0.0, 1.0)
                .flex_1()
                .label("From frosted glass to white"),
            end(swatch(WHITE)),
        ))),
    ))
}

fn grid(tiles: Vec<impl View>) -> impl View {
    vstack(tiles)
        .grid()
        .grid_cols(COLUMNS)
        .gap(TILE_GAP)
        .w_full()
}

/// One end of a slider, as wide as the other ends, so the sliders line up.
fn end(mark: impl View) -> impl View {
    hstack(mark).w(END_WIDTH).justify_center()
}

/// An "A" at `weight`, for an end of the weight slider.
fn letter(weight: u16) -> impl View {
    component(move |cx| {
        let theme = use_theme(cx);
        text("A")
            .text_size(LETTER_SIZE)
            .font_family(INTER_FAMILY)
            .font_weight(weight)
            .text_color(tint(theme, |theme| theme.text.muted))
    })
}

/// A dot of `fill`, for an end of the whiteness slider.
fn swatch(fill: Color) -> impl View {
    component(move |cx| {
        let theme = use_theme(cx);
        vstack(())
            .size(SWATCH_SIZE)
            .rounded(SWATCH_SIZE / 2.0)
            .bg(fill)
            .border(SWATCH_RING)
            .border_color(tint(theme, |theme| theme.text.muted))
    })
}
