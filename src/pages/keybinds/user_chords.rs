//! The chords the user gives to application actions, kept as rows in
//! `keybinds.app_keybinds`.
//!
//! The compositor uses the first row of an action, so each edit keeps one row
//! for each action.

use crownos_config::Keybind;
use crownos_config::schema::{AppAction, AppKeybind};

/// The row the user wrote for `action`, if there is one.
pub fn user_chord(user_chords: &[AppKeybind], action: &AppAction) -> Option<Keybind> {
    user_chords
        .iter()
        .find(|row| action.is_action(&row.app_id, &row.name))
        .map(|row| row.keys)
}

/// The chord the user wants `action` to have: their own, else the one the
/// application asked for.
pub fn wanted_chord(user_chords: &[AppKeybind], action: &AppAction) -> Keybind {
    user_chord(user_chords, action).unwrap_or(action.preferred_keys)
}

/// Gives the action `name` of `app_id` the chord `keys`.
pub fn set_user_chord(user_chords: &mut Vec<AppKeybind>, app_id: &str, name: &str, keys: Keybind) {
    forget_user_chord(user_chords, app_id, name);
    user_chords.push(AppKeybind {
        app_id: app_id.to_owned(),
        name: name.to_owned(),
        keys,
    });
}

/// Removes the chord of the user, so the action gets the one its
/// application asks for.
pub fn forget_user_chord(user_chords: &mut Vec<AppKeybind>, app_id: &str, name: &str) {
    user_chords.retain(|row| row.app_id != app_id || row.name != name);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chord(keys: &str) -> Keybind {
        keys.parse()
            .unwrap_or_else(|error| panic!("`{keys}`: {error}"))
    }

    fn action(preferred_keys: &str, assigned_keys: &str, running: bool) -> AppAction {
        AppAction {
            app_id: "recorder".to_owned(),
            name: "toggle".to_owned(),
            description: "Start or stop a recording".to_owned(),
            preferred_keys: chord(preferred_keys),
            wanted_keys: chord(preferred_keys),
            assigned_keys: chord(assigned_keys),
            running,
        }
    }

    fn row(app_id: &str, name: &str, keys: &str) -> AppKeybind {
        AppKeybind {
            app_id: app_id.to_owned(),
            name: name.to_owned(),
            keys: chord(keys),
        }
    }

    #[test]
    fn without_a_user_row_the_action_wants_the_chord_of_its_application() {
        let action = action("Super+Alt+R", "Super+Alt+R", true);
        assert_eq!(wanted_chord(&[], &action), chord("Super+Alt+R"));
        assert_eq!(
            wanted_chord(&[row("recorder", "toggle", "Ctrl+F9")], &action),
            chord("Ctrl+F9")
        );
    }

    #[test]
    fn setting_a_chord_keeps_one_row_for_the_action_and_the_others_alone() {
        let mut rows = vec![
            row("recorder", "toggle", "Ctrl+F9"),
            row("camera", "shutter", "Super+Alt+C"),
            row("recorder", "toggle", "Ctrl+F10"),
        ];
        set_user_chord(&mut rows, "recorder", "toggle", chord("Super+Alt+T"));
        assert_eq!(
            rows,
            [
                row("camera", "shutter", "Super+Alt+C"),
                row("recorder", "toggle", "Super+Alt+T"),
            ]
        );
    }

    #[test]
    fn forgetting_a_chord_removes_only_that_action() {
        let mut rows = vec![
            row("recorder", "toggle", "Ctrl+F9"),
            row("recorder", "pause", "Ctrl+F10"),
        ];
        forget_user_chord(&mut rows, "recorder", "toggle");
        assert_eq!(rows, [row("recorder", "pause", "Ctrl+F10")]);
    }
}
