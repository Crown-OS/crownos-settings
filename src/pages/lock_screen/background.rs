//! What the lock screen is drawn over, and how much it is softened.

use std::path::Path;

use crownos_config::schema::lock::{Background, BlurKey, Dim};
use crownos_config::schema::theme::COLOR_TOKENS;
use crownos_config::schema::{LockBackground, Theme, appearance};
use crownui::kit::{StatusOptions, TextInputOptions, Tone, select, status};
use crownui::prelude::{
    Color, Cx, Runtime, Semantic, Signal, Styled, TRANSPARENT, View, bind, keyed, vstack,
};
use lockscreen::palette::Palette;

use super::preview::Preview;
use crate::controls::{Options, field_choice, field_text, fraction_slider};
use crate::layout::{
    inline, nothing, optional, setting_row, setting_row_content, setting_row_desc,
    settings_card_titled, settings_divider,
};
use crate::state::Store;
use crate::util::bridge::bridge;
use crate::util::home::expand_home;

const PATH_WIDTH: f32 = 280.0;
const SWATCH_SIZE: f32 = 16.0;
const SWATCH_RADIUS: f32 = 4.0;
/// The color a switch to "Color" starts on.
const FIRST_TOKEN: &str = "accent.end";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Source {
    Desktop,
    Picture,
    Color,
}

static SOURCES: Options<Source> = Options::new(&[
    ("Desktop wallpaper", Source::Desktop),
    ("Picture", Source::Picture),
    ("Color", Source::Color),
]);

fn source_of(background: &LockBackground) -> Source {
    match background {
        LockBackground::Desktop => Source::Desktop,
        LockBackground::Image(_) => Source::Picture,
        LockBackground::Token(_) => Source::Color,
    }
}

pub fn background_card(cx: &mut Cx, store: Store, preview: &Preview) -> impl View + use<> {
    let unreadable = preview.picture_unreadable();
    let source = field_choice(
        cx,
        &SOURCES,
        move |runtime| source_of(&store.get(runtime, Background)),
        move |runtime, source| {
            let background = match source {
                Source::Desktop => LockBackground::Desktop,
                Source::Picture => LockBackground::Image(store.get(runtime, appearance::Wallpaper)),
                Source::Color => LockBackground::Token(FIRST_TOKEN.to_owned()),
            };
            store.set(runtime, Background, background);
        },
    );
    settings_card_titled(
        "Background",
        (
            setting_row("Show", source.label("Background")),
            keyed(
                move |runtime| source_of(&store.get(runtime, Background)),
                move |cx, source| match source {
                    Source::Desktop => nothing().boxed(),
                    Source::Picture => picture_rows(cx, store, unreadable).boxed(),
                    Source::Color => color_rows(cx, store).boxed(),
                },
            ),
            settings_divider(),
            setting_row_desc(
                "Dim",
                "How much darker the background gets",
                fraction_slider(cx, store, Dim),
            ),
            settings_divider(),
            setting_row_desc(
                "Blur",
                "As a share of the desktop's own blur",
                fraction_slider(cx, store, BlurKey),
            ),
        ),
    )
}

fn picture_rows(cx: &mut Cx, store: Store, unreadable: Signal<bool>) -> impl View + use<> {
    let relative = cx.signal(false);
    let path = field_text(
        cx,
        move |runtime| match store.get(runtime, Background) {
            LockBackground::Image(path) => path,
            _ => String::new(),
        },
        move |runtime, typed| {
            let path = expand_home(typed.trim());
            let absolute = Path::new(path.as_ref()).is_absolute();
            relative.set(runtime, !absolute);
            if absolute {
                store.set(
                    runtime,
                    Background,
                    LockBackground::Image(path.into_owned()),
                );
            }
        },
    )
    .placeholder("/path/to/picture.jpg")
    .width(PATH_WIDTH)
    .label("Picture");
    vstack((
        settings_divider(),
        setting_row_desc("Picture", "Any JPEG, PNG, WebP or SVG file", path),
        optional(
            move |runtime| picture_problem(relative.get(runtime), unreadable.get(runtime)),
            |_, problem| setting_row_content(status(problem).tone(Tone::Warning)),
        ),
    ))
}

fn picture_problem(relative: bool, unreadable: bool) -> Option<&'static str> {
    if relative {
        Some("Give the full path, starting with / or ~/")
    } else if unreadable {
        Some("This picture can't be opened, so the lock screen will show none")
    } else {
        None
    }
}

fn color_rows(cx: &mut Cx, store: Store) -> impl View + use<> {
    let labels = cx.signal(
        COLOR_TOKENS
            .iter()
            .map(|token| (*token).to_owned())
            .collect(),
    );
    let selected = bridge(
        cx,
        move |runtime| {
            let token = current_token(runtime, store)?;
            COLOR_TOKENS
                .iter()
                .position(|candidate| *candidate == token)
        },
        move |runtime, index: Option<usize>| {
            if let Some(token) = index.and_then(|index| COLOR_TOKENS.get(index)) {
                store.set(
                    runtime,
                    Background,
                    LockBackground::Token((*token).to_owned()),
                );
            }
        },
    );
    vstack((
        settings_divider(),
        setting_row(
            "Color",
            inline((swatch(store), select(labels, selected).label("Color"))),
        ),
    ))
}

fn current_token(runtime: &mut Runtime, store: Store) -> Option<String> {
    match store.get(runtime, Background) {
        LockBackground::Token(token) => Some(token),
        _ => None,
    }
}

/// The chosen color as the lock screen paints it: always on its dark palette.
fn swatch(store: Store) -> impl View {
    vstack(())
        .size(SWATCH_SIZE)
        .rounded(SWATCH_RADIUS)
        .bg(bind(move |runtime| {
            let Some(token) = current_token(runtime, store) else {
                return TRANSPARENT;
            };
            let [red, green, blue, alpha] =
                store.section(runtime, |theme: &Theme| Palette::new(theme).color(&token));
            Color::new(red, green, blue, alpha)
        }))
}
