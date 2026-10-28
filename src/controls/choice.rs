use crownos_config::Key;
use crownui::kit::{SelectKind, select};
use crownui::prelude::{Cx, Element, Runtime, Signal};

use super::Options;
use crate::config::Section;
use crate::state::Store;
use crate::util::bridge::{bridge, derived};

/// A select over an [`Options`] table, bound to a `Copy` key.
pub fn choice<K>(
    cx: &mut Cx,
    store: Store,
    key: K,
    options: &'static Options<K::Value>,
) -> Element<SelectKind>
where
    K: Key,
    K::Value: Copy,
    K::Section: Section,
{
    field_choice(
        cx,
        options,
        move |runtime| store.get(runtime, key),
        move |runtime, value| store.set(runtime, key, value),
    )
}

/// A select over an [`Options`] table for a value that is only part of a key:
/// `read` picks it out, `write` puts a new one back.
pub fn field_choice<T: Copy + PartialEq + 'static>(
    cx: &mut Cx,
    options: &'static Options<T>,
    mut read: impl FnMut(&mut Runtime) -> T + 'static,
    mut write: impl FnMut(&mut Runtime, T) + 'static,
) -> Element<SelectKind> {
    let labels = cx.signal(options.labels());
    let selected = bridge(
        cx,
        move |runtime| options.index_of(read(runtime)),
        move |runtime, index: Option<usize>| {
            if let Some(value) = index.and_then(|index| options.value_at(index)) {
                write(runtime, value);
            }
        },
    );
    select(labels, selected)
}

/// A select over a fixed list of names, bound to an `Option<String>` key:
/// the label *is* the value, and "not configured" shows as nothing selected.
pub fn name_choice<K>(
    cx: &mut Cx,
    store: Store,
    key: K,
    names: &'static [&'static str],
) -> Element<SelectKind>
where
    K: Key<Value = Option<String>>,
    K::Section: Section,
{
    let labels = cx.signal(names.iter().map(|name| (*name).to_owned()).collect());
    let selected = bridge(
        cx,
        move |runtime| {
            let current = store.get(runtime, key);
            current
                .as_deref()
                .and_then(|current| names.iter().position(|name| *name == current))
        },
        move |runtime, index: Option<usize>| {
            if let Some(name) = index.and_then(|index| names.get(index)) {
                store.set(runtime, key, Some((*name).to_owned()));
            }
        },
    );
    select(labels, selected)
}

/// A select over names discovered at runtime, bound to an `Option<String>`
/// key whose `None` means "let the system decide".
///
/// Not choosing a microphone is itself a working answer — the system default,
/// which follows a headset in and out of the socket — so `None` gets a row of
/// its own at the top, labelled by `automatic`. A stored name that matches
/// nothing (the device is unplugged) selects nothing at all rather than
/// silently reading as "Automatic": the two are different states.
pub fn auto_name_choice<K>(
    cx: &mut Cx,
    store: Store,
    key: K,
    names: Signal<Vec<String>>,
    mut automatic: impl FnMut(&mut Runtime) -> String + 'static,
) -> Element<SelectKind>
where
    K: Key<Value = Option<String>>,
    K::Section: Section,
{
    let rows = derived(cx, move |runtime| {
        let automatic = automatic(runtime);
        names.with(runtime, |names| {
            std::iter::once(automatic)
                .chain(names.iter().cloned())
                .collect()
        })
    });
    let selected = bridge(
        cx,
        move |runtime| {
            let current = store.get(runtime, key);
            names.with(runtime, |names| automatic_index(current.as_deref(), names))
        },
        move |runtime, index: Option<usize>| {
            let chosen = match index {
                Some(0) => None,
                Some(index) => match names.with(runtime, |names| names.get(index - 1).cloned()) {
                    Some(name) => Some(name),
                    None => return,
                },
                None => return,
            };
            store.set(runtime, key, chosen);
        },
    );
    select(rows, selected)
}

/// Where the current choice sits in an "Automatic"-first list.
fn automatic_index(current: Option<&str>, names: &[String]) -> Option<usize> {
    match current {
        None => Some(0),
        Some(current) => names
            .iter()
            .position(|name| name == current)
            .map(|index| index + 1),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn automatic_leads_and_a_missing_device_selects_nothing() {
        let names = vec!["Yeti".to_owned(), "Webcam".to_owned()];
        assert_eq!(automatic_index(None, &names), Some(0));
        assert_eq!(automatic_index(Some("Webcam"), &names), Some(2));
        assert_eq!(automatic_index(Some("Unplugged"), &names), None);
    }
}
