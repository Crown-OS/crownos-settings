//! The actions that applications registered with the compositor, one row each.
//!
//! The compositor writes the list to `app_actions.ron`. Each chord the user
//! records here goes to `keybinds.app_keybinds`, and the compositor gives it
//! to the action at once.

use crownos_config::schema::{AppAction, AppActions, Keybinds};
use crownui::kit::{
    ButtonOptions, ButtonSize, ButtonVariant, StatusOptions, Tone, button, keybind_input, status,
};
use crownui::prelude::{Cx, Listeners, Runtime, Semantic, Signal, View, bind, for_each, vstack};

use super::user_chords::{forget_user_chord, set_user_chord, user_chord, wanted_chord};
use crate::layout::{
    Listed, hairline_unless_first, inline, listed, setting_row_content, setting_row_desc,
    settings_card_titled, settings_divider, when,
};
use crate::state::Store;
use crate::util::bridge::{bridge, derived};

type Row = Signal<Listed<AppAction>>;

pub fn applications_card(cx: &mut Cx, store: Store) -> impl View + use<> {
    let actions = derived(cx, move |runtime| {
        store.section(runtime, |app_actions: &AppActions| {
            listed(app_actions.actions.iter().cloned())
        })
    });
    settings_card_titled(
        "Applications",
        (
            when(
                move |runtime| actions.with(runtime, Vec::is_empty),
                |_| {
                    setting_row_content(
                        status("No application has asked for a shortcut").tone(Tone::Neutral),
                    )
                },
            ),
            for_each(
                actions,
                |row: &Listed<AppAction>| (row.item.app_id.clone(), row.item.name.clone()),
                move |cx, row| action_row(cx, store, row),
            ),
        ),
    )
}

fn action_row(cx: &mut Cx, store: Store, row: Row) -> impl View + use<> {
    let (app_id, name, title) = row.with_untracked(cx, |row| {
        let action = &row.item;
        let title = if action.description.is_empty() {
            action.name.clone()
        } else {
            action.description.clone()
        };
        (action.app_id.clone(), action.name.clone(), title)
    });
    let wanted = move |runtime: &mut Runtime| {
        let action = row.with(runtime, |row| row.item.clone());
        store.section(runtime, |keybinds: &Keybinds| {
            wanted_chord(&keybinds.app_keybinds, &action)
        })
    };
    let has_user_chord = move |runtime: &mut Runtime| {
        let action = row.with(runtime, |row| row.item.clone());
        store.section(runtime, |keybinds: &Keybinds| {
            user_chord(&keybinds.app_keybinds, &action).is_some()
        })
    };
    let refused = move |runtime: &mut Runtime| row.with(runtime, |row| row.item.is_refused());

    let (recorded_app_id, recorded_name) = (app_id.clone(), name.clone());
    let chord = bridge(cx, wanted, move |runtime, keys| {
        store.update::<Keybinds>(runtime, |keybinds| {
            set_user_chord(
                &mut keybinds.app_keybinds,
                &recorded_app_id,
                &recorded_name,
                keys,
            );
        });
    });
    let reset_label = format!("Reset the shortcut of {title}");
    let reset = move |_: &mut Cx| reset_button(store, &app_id, &name, &reset_label);

    vstack((
        hairline_unless_first(row),
        setting_row_desc(
            title,
            bind(move |runtime| row.with(runtime, |row| subtitle(&row.item))),
            inline((when(has_user_chord, reset), keybind_input(chord))),
        ),
        when(refused, |_| {
            vstack((
                settings_divider(),
                setting_row_content(
                    status("This shortcut is used by the desktop or another app, or is not allowed, so the action has none")
                        .tone(Tone::Warning),
                ),
            ))
        }),
    ))
}

fn reset_button(store: Store, app_id: &str, name: &str, label: &str) -> impl View + use<> {
    let (app_id, name) = (app_id.to_owned(), name.to_owned());
    button("Reset")
        .variant(ButtonVariant::Ghost)
        .control_size(ButtonSize::Small)
        .label(label.to_owned())
        .on_click(move |cx, _| {
            store.update::<Keybinds>(cx, |keybinds| {
                forget_user_chord(&mut keybinds.app_keybinds, &app_id, &name);
            });
        })
}

fn subtitle(action: &AppAction) -> String {
    if action.running {
        action.app_id.clone()
    } else {
        format!("{} · not running", action.app_id)
    }
}
