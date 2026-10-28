//! How the clock reads: its style, where it sits, its formats and font.

use crownos_config::Key;
use crownos_config::schema::lock::{ClockStyleKey, DateFormat, Font, Layout, TimeFormat};
use crownos_config::schema::{ClockStyle, Lock, LockLayout, Theme};
use crownui::kit::{TextInputOptions, select};
use crownui::prelude::{Cx, Runtime, Semantic, View, vstack};
use lockscreen::clock::ClockFace;

use super::clock_look::look_rows;
use crate::controls::{Options, choice, field_text};
use crate::layout::{setting_row, setting_row_desc, settings_card_titled, settings_divider, when};
use crate::state::Store;
use crate::util::bridge::bridge;

const TEXT_WIDTH: f32 = 220.0;

static STYLES: Options<ClockStyle> = Options::new(&[
    ("Classic", ClockStyle::Classic),
    ("Large", ClockStyle::Large),
    ("Minimal", ClockStyle::Minimal),
]);

static LAYOUTS: Options<LockLayout> = Options::new(&[
    ("Centered", LockLayout::Center),
    ("Left", LockLayout::Left),
    ("Bottom", LockLayout::Bottom),
]);

/// `(name, strftime pattern)`.
type Format = (&'static str, &'static str);

const TIME_FORMATS: &[Format] = &[
    ("24-hour", "%H:%M"),
    ("12-hour", "%I:%M %p"),
    ("With seconds", "%H:%M:%S"),
];

const DATE_FORMATS: &[Format] = &[
    ("Full", "%A, %e %B"),
    ("Short", "%a %e %b"),
    ("Numeric", "%d/%m/%Y"),
    ("ISO", "%Y-%m-%d"),
];

pub fn clock_card(cx: &mut Cx, store: Store) -> impl View + use<> {
    let font_placeholder =
        cx.untrack(|runtime| store.section(runtime, |theme: &Theme| theme.font.family.clone()));
    settings_card_titled(
        "Clock",
        (
            setting_row("Style", choice(cx, store, ClockStyleKey, &STYLES)),
            look_rows(cx, store),
            settings_divider(),
            setting_row("Layout", choice(cx, store, Layout, &LAYOUTS)),
            settings_divider(),
            format_rows(cx, store, TimeFormat, "Time", TIME_FORMATS, |face| {
                face.time
            }),
            settings_divider(),
            format_rows(cx, store, DateFormat, "Date", DATE_FORMATS, |face| {
                face.date
            }),
            settings_divider(),
            setting_row_desc(
                "Font",
                "A font family installed on this computer; empty keeps the theme's",
                field_text(
                    cx,
                    move |runtime| store.get(runtime, Font).unwrap_or_default(),
                    move |runtime, family| {
                        let family = family.trim();
                        store.set(
                            runtime,
                            Font,
                            (!family.is_empty()).then(|| family.to_owned()),
                        );
                    },
                )
                .placeholder(font_placeholder)
                .width(TEXT_WIDTH)
                .label("Font"),
            ),
        ),
    )
}

/// A select of `formats`, each named with how it reads right now, plus
/// "Custom", which opens a field for any strftime pattern.
fn format_rows<K>(
    cx: &mut Cx,
    store: Store,
    key: K,
    title: &'static str,
    formats: &'static [Format],
    shows: fn(ClockFace) -> String,
) -> impl View + use<K>
where
    K: Key<Section = Lock, Value = String>,
{
    let custom_index = formats.len();
    let labels = cx.signal(
        formats
            .iter()
            .map(|(name, pattern)| format!("{name} · {}", sample::<K>(pattern, shows)))
            .chain(["Custom".to_owned()])
            .collect(),
    );
    let picked_custom = cx.signal(false);
    let is_custom = move |runtime: &mut Runtime| {
        picked_custom.get(runtime) || preset_of(formats, &store.get(runtime, key)).is_none()
    };
    let selected = bridge(
        cx,
        move |runtime| match is_custom(runtime) {
            true => Some(custom_index),
            false => preset_of(formats, &store.get(runtime, key)),
        },
        move |runtime, index: Option<usize>| {
            let Some(index) = index else { return };
            picked_custom.set(runtime, index == custom_index);
            if let Some((_, pattern)) = formats.get(index) {
                store.set(runtime, key, (*pattern).to_owned());
            }
        },
    );
    vstack((
        setting_row(title, select(labels, selected).label(title)),
        when(is_custom, move |cx| {
            vstack((
                settings_divider(),
                setting_row_desc(
                    "Pattern",
                    "strftime, as in %H:%M or %A %e %B",
                    field_text(
                        cx,
                        move |runtime| store.get(runtime, key),
                        move |runtime, pattern| {
                            if !pattern.trim().is_empty() {
                                store.set(runtime, key, pattern);
                            }
                        },
                    )
                    .width(TEXT_WIDTH)
                    .label(format!("{title} pattern")),
                ),
            ))
        }),
    ))
}

fn preset_of(formats: &[Format], pattern: &str) -> Option<usize> {
    formats.iter().position(|(_, preset)| *preset == pattern)
}

/// How `pattern` reads now, formatted the way the lock screen formats it.
fn sample<K: Key<Section = Lock, Value = String>>(
    pattern: &str,
    shows: fn(ClockFace) -> String,
) -> String {
    let mut lock = Lock::default();
    K::set(&mut lock, pattern.to_owned());
    shows(ClockFace::now(&lock))
}
