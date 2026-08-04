//! Setting controls that persist themselves.
//!
//! Every builder here takes the *current* value plus a `fn(&mut Section, T)`
//! setter, and hands back a `crownuikit` widget already wired to
//! [`ConfigStore::update`]. A page says which field a control edits and never
//! says how — or when — that field reaches the disk, so the "mutate the state,
//! then remember to call the right `save_*`" pair that used to appear in every
//! single callback now exists once, here.
//!
//! The setters are plain `fn` pointers rather than closures on purpose: a
//! non-capturing closure at the call site (`|w: &mut Wifi, v| w.enabled = v`)
//! coerces to one, it fixes the section type by inference, and it keeps every
//! returned view `Send + Sync + 'static` without further bounds.
//!
//! These views are over [`ConfigStore`], not over the app state, so nothing in
//! this module — or in any page built from it — knows the app has a sidebar.

use crownuikit::util::INTER;
use crownuikit::widgets::{SelectView, SliderView, ToggleView, select, slider, toggle};
use xilem::masonry::core::ArcStr;
use xilem::style::Style;
use xilem::view::label;
use xilem::{Color, WidgetView};

use crate::config::{ConfigStore, Section};

/// Every slider in the app presents a 0–100 scale, whatever the stored field's
/// own range is.
const PERCENT: (f64, f64) = (0.0, 100.0);

/// Muted colour for read-only value rows — close enough to the uikit's row
/// description tone to read as "informational, not editable".
const VALUE_TEXT: Color = Color::from_rgb8(0x8A, 0x8A, 0x8F);
const VALUE_TEXT_SIZE: f32 = 14.0;

// --- MARK: Option tables ---

/// A `(label, value)` table for a setting with a fixed set of choices.
///
/// Two jobs: it is the single place a variant's user-facing name lives, and it
/// translates between those names and the typed values, because `select` speaks
/// only in indices. Declare one as a `static` next to the page that uses it and
/// hand out `&'static` references:
///
/// ```ignore
/// static ACCENTS: Options<AccentColor> = Options::new(&[
///     ("Purple", AccentColor::Purple),
///     ("Blue", AccentColor::Blue),
/// ]);
/// ```
pub struct Options<T: 'static>(&'static [(&'static str, T)]);

impl<T: 'static> Options<T> {
    /// Build a table. Entry order is the order the select lists them in.
    pub const fn new(entries: &'static [(&'static str, T)]) -> Self {
        Self(entries)
    }

    /// The labels, in table order.
    fn labels(&self) -> impl Iterator<Item = &'static str> + use<'_, T> {
        self.0.iter().map(|(label, _)| *label)
    }
}

impl<T: Copy + PartialEq + 'static> Options<T> {
    /// Where `value` sits in the table, or `None` when the table doesn't offer
    /// it — which happens when a RON file names a variant this build doesn't
    /// have, or holds a number that isn't one of the offered ones.
    ///
    /// `None` makes the select show nothing selected. That is deliberately not
    /// "fall back to the first entry": highlighting `Purple` because the stored
    /// accent is unrecognised would misreport what is actually on disk.
    fn index_of(&self, value: T) -> Option<usize> {
        self.0.iter().position(|(_, candidate)| *candidate == value)
    }

    /// The value at `index`, or `None` for an index the table doesn't have.
    fn value_at(&self, index: usize) -> Option<T> {
        self.0.get(index).map(|(_, value)| *value)
    }
}

// --- MARK: Controls ---

/// A toggle bound to a `bool` field.
///
/// The widget's own label is always empty: the surrounding
/// [`setting_row`](crownuikit::layouts::settings::setting_row) already prints
/// the name, and a second copy inside the control would duplicate it.
pub fn switch<S: Section>(
    checked: bool,
    set: fn(&mut S, bool),
) -> ToggleView<impl Fn(&mut ConfigStore, bool) + Send + Sync + 'static> {
    toggle("", checked, move |store: &mut ConfigStore, value| {
        store.update::<S>(|section| set(section, value));
    })
}

/// A 0–100 slider bound to a field that stores that same 0–100 scale.
pub fn percent_slider<S: Section>(
    value: f64,
    set: fn(&mut S, f64),
) -> SliderView<impl Fn(&mut ConfigStore, f64) + Send + Sync + 'static> {
    slider(
        PERCENT.0,
        PERCENT.1,
        value,
        move |store: &mut ConfigStore, value| {
            store.update::<S>(|section| set(section, value));
        },
    )
}

/// A 0–100 slider bound to a field that stores a `0.0..=1.0` fraction.
///
/// The user thinks in percent, the RON file keeps the fraction, and the
/// conversion lives here instead of in each page's callback.
pub fn fraction_slider<S: Section>(
    fraction: f64,
    set: fn(&mut S, f64),
) -> SliderView<impl Fn(&mut ConfigStore, f64) + Send + Sync + 'static> {
    slider(
        PERCENT.0,
        PERCENT.1,
        fraction * 100.0,
        move |store: &mut ConfigStore, percent| {
            store.update::<S>(|section| set(section, percent / 100.0));
        },
    )
}

/// A select over an [`Options`] table, bound to a `Copy` field.
pub fn choice<S, T>(
    options: &'static Options<T>,
    current: T,
    set: fn(&mut S, T),
) -> SelectView<impl Fn(&mut ConfigStore, usize) + Send + Sync + 'static>
where
    S: Section,
    T: Copy + PartialEq + Send + Sync + 'static,
{
    select(
        options.labels(),
        options.index_of(current),
        move |store: &mut ConfigStore, index| {
            if let Some(value) = options.value_at(index) {
                store.update::<S>(|section| set(section, value));
            }
        },
    )
}

/// A select over a list of names, bound to an `Option<String>` field.
///
/// Separate from [`choice`] because these names come from the system rather than
/// from a schema enum — SSIDs, audio sinks — so the label *is* the value, and
/// "not configured" is a real state that shows as nothing selected.
pub fn name_choice<S: Section>(
    names: &'static [&'static str],
    current: Option<&String>,
    set: fn(&mut S, String),
) -> SelectView<impl Fn(&mut ConfigStore, usize) + Send + Sync + 'static> {
    let selected = current.and_then(|current| names.iter().position(|name| name == current));

    select(
        names.iter().copied(),
        selected,
        move |store: &mut ConfigStore, index| {
            if let Some(name) = names.get(index) {
                store.update::<S>(|section| set(section, (*name).to_owned()));
            }
        },
    )
}

/// A read-only row value, for state the app reports but doesn't let you edit.
pub fn value_text(text: impl Into<ArcStr>) -> impl WidgetView<ConfigStore> {
    label(text.into())
        .text_size(VALUE_TEXT_SIZE)
        .font(INTER)
        .color(VALUE_TEXT)
}
