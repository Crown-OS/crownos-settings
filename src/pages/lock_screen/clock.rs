//! How the clock reads: its time and date formats.

use crownos_config::Key;
use crownos_config::schema::Lock;
use crownos_config::schema::lock::{DateFormat, TimeFormat};
use crownui::kit::{TextInputOptions, select};
use crownui::prelude::{Cx, Runtime, Semantic, View, vstack};
use lockscreen::clock::ClockFace;

use crate::controls::field_text;
use crate::layout::{setting_row, setting_row_desc, settings_card_titled, settings_divider, when};
use crate::state::Store;
use crate::util::bridge::bridge;

const TEXT_WIDTH: f32 = 220.0;

/// `(name, strftime pattern)`.
type Format = (&'static str, &'static str);

const TIME_FORMATS: &[Format] = &[
    ("24-hour", "%H:%M"),
    ("12-hour", "%I:%M %p"),
    ("With seconds", "%H:%M:%S"),
];

const DATE_FORMATS: &[Format] = &[
    ("Default", "%a, %b %-d"),
    ("Full", "%A, %e %B"),
    ("Short", "%a %e %b"),
    ("Numeric", "%d/%m/%Y"),
    ("ISO", "%Y-%m-%d"),
];

pub fn clock_card(cx: &mut Cx, store: Store) -> impl View + use<> {
    settings_card_titled(
        "Time & date",
        (
            format_rows(cx, store, TimeFormat, "Time", TIME_FORMATS, |face| {
                face.time
            }),
            settings_divider(),
            format_rows(cx, store, DateFormat, "Date", DATE_FORMATS, |face| {
                face.date
            }),
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
