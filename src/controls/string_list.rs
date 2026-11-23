use crownos_config::Key;
use crownui::kit::{ButtonOptions, ButtonVariant, TextInputOptions, button};
use crownui::prelude::{AnyView, Cx, Listeners, Semantic, View, keyed, vstack};

use super::field_text;
use crate::config::Section;
use crate::layout::{setting_row, setting_row_content, settings_divider};
use crate::state::Store;

const FIELD_WIDTH: f32 = 320.0;

/// The entries of a list key, each with a Remove button, above a field that
/// adds one when the user commits it.
pub fn string_list<K>(cx: &mut Cx, store: Store, key: K, placeholder: &'static str) -> AnyView
where
    K: Key<Value = Vec<String>> + Copy + 'static,
    K::Section: Section,
{
    let rows = keyed(
        move |runtime| store.get(runtime, key),
        move |_, entries| {
            let rows: Vec<AnyView> = entries
                .into_iter()
                .flat_map(|entry| {
                    let removed = entry.clone();
                    let remove = button("Remove")
                        .variant(ButtonVariant::Ghost)
                        .label(format!("Remove {entry}"))
                        .on_click(move |cx, _| {
                            store.update::<K::Section>(cx, |section| {
                                let mut entries = K::get(section);
                                entries.retain(|kept| *kept != removed);
                                K::set(section, entries);
                            });
                        });
                    [
                        setting_row(entry, remove).boxed(),
                        settings_divider().boxed(),
                    ]
                })
                .collect();
            vstack(rows).boxed()
        },
    );
    let add = field_text(
        cx,
        move |runtime| {
            let _ = store.get(runtime, key).len();
            String::new()
        },
        move |runtime, typed| {
            let typed = typed.trim().to_owned();
            if typed.is_empty() {
                return;
            }
            store.update::<K::Section>(runtime, |section| {
                let mut entries = K::get(section);
                if !entries.contains(&typed) {
                    entries.push(typed);
                }
                K::set(section, entries);
            });
        },
    )
    .placeholder(placeholder)
    .width(FIELD_WIDTH);
    vstack((rows, setting_row_content(add))).boxed()
}
