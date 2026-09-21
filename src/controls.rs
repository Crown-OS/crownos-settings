//! Setting controls that persist themselves.
//!
//! Every builder here takes the store and a [`Key`], and hands back a
//! `crownuikit` widget already wired to [`Store::set`]. A page says which
//! field a control edits and never says how — or when — that field reaches the
//! disk, so the "mutate the state, then remember to call the right `save_*`" pair
//! that used to appear in every single callback now exists once, here.
//!
//! The key is the whole binding: it names the field once, and everything else
//! follows from it. The section to write is `K::Section`, the value type the
//! widget deals in is `K::Value`, and reading the current value is
//! `store.get(key)`. A control therefore cannot read one field and write
//! another, which the previous `(current value, fn(&mut Section, T) setter)`
//! pair could do whenever the two halves drifted apart:
//!
//! ```ignore
//! // before: the field is named twice, and nothing checks that it is the same field
//! switch(appearance.dark_mode, |a: &mut Appearance, on| a.dark_mode = on)
//! // after
//! switch(store, appearance::DarkMode)
//! ```
//!
//! Bounds worth knowing: `K::Value` decides what a control can bind to, so
//! `switch` asks for `Key<Value = bool>` and the sliders for `Key<Value = f64>`.
//! Pointing one at the wrong field is a compile error naming the value type,
//! not a silently mis-wired widget. Keys are `Copy` and zero-sized, which is
//! what keeps every returned view `Send + Sync + 'static`.
//!
//! These views are over [`Store`], not over the app state, so nothing in
//! this module — or in any page built from it — knows the app has a sidebar.

use crownos_config::{Key, Keybind};
use crownuikit::config::theme;
use crownuikit::util::INTER;
use crownuikit::widgets::{
    KeybindInputView, SelectView, SliderView, ToggleView, keybind_input, select, slider, toggle,
};
use xilem::WidgetView;
use xilem::masonry::core::ArcStr;
use xilem::style::Style;
use xilem::view::label;

use crate::config::Section;
use crate::state::Store;

/// Every slider in the app presents a 0–100 scale, whatever the stored field's
/// own range is.
const PERCENT: (f64, f64) = (0.0, 100.0);

const VALUE_TEXT_SIZE: f32 = 16.0;

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

/// A toggle bound to a `bool` key.
///
/// The widget's own label is always empty: the surrounding
/// [`setting_row`](crate::layout::setting_row) already prints
/// the name, and a second copy inside the control would duplicate it.
pub fn switch<K>(
    store: &Store,
    key: K,
) -> ToggleView<impl Fn(&mut Store, bool) + Send + Sync + use<K>>
where
    K: Key<Value = bool>,
    K::Section: Section,
{
    toggle("", store.get(key), move |store: &mut Store, value| {
        store.set(key, value);
    })
}

/// A 0–100 slider bound to a key that stores that same 0–100 scale.
pub fn percent_slider<K>(
    store: &Store,
    key: K,
) -> SliderView<impl Fn(&mut Store, f64) + Send + Sync + use<K>>
where
    K: Key<Value = f64>,
    K::Section: Section,
{
    slider(
        PERCENT.0,
        PERCENT.1,
        store.get(key),
        move |store: &mut Store, value| store.set(key, value),
    )
}

/// A 0–100 slider bound to a key that stores a `0.0..=1.0` fraction.
///
/// The user thinks in percent, the RON file keeps the fraction, and the
/// conversion lives here instead of in each page's callback.
pub fn fraction_slider<K>(
    store: &Store,
    key: K,
) -> SliderView<impl Fn(&mut Store, f64) + Send + Sync + use<K>>
where
    K: Key<Value = f64>,
    K::Section: Section,
{
    slider(
        PERCENT.0,
        PERCENT.1,
        store.get(key) * 100.0,
        move |store: &mut Store, percent| store.set(key, percent / 100.0),
    )
}

/// A select over an [`Options`] table, bound to a `Copy` key.
///
/// The table's entries are `K::Value`s, so a table of `DisplayScale`s only fits
/// a key whose field is a `DisplayScale`.
pub fn choice<K>(
    store: &Store,
    key: K,
    options: &'static Options<K::Value>,
) -> SelectView<impl Fn(&mut Store, usize) + Send + Sync + use<K>>
where
    K: Key,
    K::Value: Copy + Sync,
    K::Section: Section,
{
    select(
        options.labels(),
        options.index_of(store.get(key)),
        move |store: &mut Store, index| {
            if let Some(value) = options.value_at(index) {
                store.set(key, value);
            }
        },
    )
}

/// A select over a list of names, bound to an `Option<String>` key.
///
/// Separate from [`choice`] because these names come from the system rather than
/// from a schema enum — SSIDs, audio sinks — so the label *is* the value, and
/// "not configured" is a real state that shows as nothing selected.
pub fn name_choice<K>(
    store: &Store,
    key: K,
    names: &'static [&'static str],
) -> SelectView<impl Fn(&mut Store, usize) + Send + Sync + use<K>>
where
    K: Key<Value = Option<String>>,
    K::Section: Section,
{
    let current = store.get(key);
    let selected = current
        .as_deref()
        .and_then(|current| names.iter().position(|name| *name == current));

    select(
        names.iter().copied(),
        selected,
        move |store: &mut Store, index| {
            if let Some(name) = names.get(index) {
                store.set(key, Some((*name).to_owned()));
            }
        },
    )
}

/// A select over a list of names discovered at runtime, bound to an
/// `Option<String>` key whose `None` means "let the system decide".
///
/// The difference from [`name_choice`] is that first row. A Wi-Fi network or an
/// audio sink is either chosen or not chosen, and "not chosen" shows as an empty
/// select; a *microphone* is different, because not choosing one is itself a
/// working answer — the system default, which follows a headset in and out of
/// the socket. So `None` gets a row of its own at the top rather than being the
/// absence of a row, and `automatic` is what that row says.
///
/// The names come from the system rather than from a schema, so unlike
/// [`choice`] this takes them by value: the list is a `Vec<String>` read at
/// startup, and it lives as long as the select's callback does.
pub fn auto_name_choice<K, L>(
    store: &Store,
    key: K,
    automatic: L,
    names: &[String],
) -> SelectView<impl Fn(&mut Store, usize) + Send + Sync + use<K, L>>
where
    L: Into<ArcStr>,
    K: Key<Value = Option<String>>,
    K::Section: Section,
{
    let current = store.get(key);
    // Index 0 is "Automatic", so a named device sits one row further down than
    // its position in `names`. A stored name that matches nothing — the device
    // it named is unplugged — selects nothing at all rather than silently
    // reading as "Automatic", because the two are different states and the user
    // is owed the difference.
    let selected = match current.as_deref() {
        None => Some(0),
        Some(current) => names
            .iter()
            .position(|name| name == current)
            .map(|index| index + 1),
    };

    let rows: Vec<ArcStr> = std::iter::once(automatic.into())
        .chain(names.iter().map(|name| ArcStr::from(name.as_str())))
        .collect();
    // Owned by the callback, which outlives this frame's view.
    let names: Vec<String> = names.to_vec();

    select(rows, selected, move |store: &mut Store, index| {
        let chosen = match index {
            0 => None,
            index => names.get(index - 1).cloned(),
        };
        // A row that isn't there changes nothing — `select` only ever reports
        // indices it was given, so this is belt and braces.
        if index == 0 || chosen.is_some() {
            store.set(key, chosen);
        }
    })
}

/// A shortcut recorder bound to a [`Keybind`] key.
///
/// The whole binding is the key, as everywhere else here: the field records a
/// chord and the store writes it to the section the key names, so a page never
/// says what a shortcut *is* — only which setting it belongs to.
pub fn keybind<K>(
    store: &Store,
    key: K,
) -> KeybindInputView<impl Fn(&mut Store, Keybind) + Send + Sync + use<K>>
where
    K: Key<Value = Keybind>,
    K::Section: Section,
{
    keybind_input(store.get(key), move |store: &mut Store, bind| {
        store.set(key, bind);
    })
}

/// A read-only row value, for state the app reports but doesn't let you edit.
pub fn value_text<T: Into<ArcStr>>(text: T) -> impl WidgetView<Store> + use<T> {
    // The theme's muted tone — the same one a row description uses, which is
    // what makes these read as "informational, not editable".
    label(text.into())
        .text_size(VALUE_TEXT_SIZE)
        .font(INTER)
        .color(theme().text.muted)
}
