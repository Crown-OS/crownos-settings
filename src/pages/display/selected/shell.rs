//! Which monitor is primary, and which shell surfaces each monitor shows.

use crownos_config::schema::{Compositor, OutputSetting, ShellRole};
use crownui::kit::toggle;
use crownui::prelude::{Cx, Runtime, Semantic, View, bind, vstack};

use super::super::output_names::OutputNames;
use crate::layout::{setting_row_desc, settings_divider};
use crate::state::Store;
use crate::util::bridge::bridge;

const ROLES: [(ShellRole, &str, &str); 3] = [
    (ShellRole::Bar, "Show bar", "The top bar on this monitor"),
    (ShellRole::Dock, "Show dock", "The app dock on this monitor"),
    (
        ShellRole::Notifications,
        "Show notifications",
        "Notification pop-ups on this monitor",
    ),
];

pub fn rows(cx: &mut Cx, store: Store, name: String, names: OutputNames) -> impl View + use<> {
    let primary = primary_row(cx, store, name.clone(), names.clone());
    let [bar, dock, notifications] =
        ROLES.map(|role| role_row(cx, store, name.clone(), names.clone(), role));
    vstack((
        settings_divider(),
        primary,
        settings_divider(),
        bar,
        settings_divider(),
        dock,
        settings_divider(),
        notifications,
    ))
}

fn is_primary(runtime: &mut Runtime, store: Store, name: &str) -> bool {
    store.with_primary_monitor(runtime, |primary| {
        primary.is_some_and(|monitor| monitor.name == name)
    })
}

/// Works like a radio button: the primary monitor's switch is on and locked,
/// and switching another one on moves the primary there.
fn primary_row(cx: &mut Cx, store: Store, name: String, names: OutputNames) -> impl View + use<> {
    let reading = name.clone();
    let on = bridge(
        cx,
        move |runtime| is_primary(runtime, store, &reading),
        move |runtime, on| {
            if on {
                store.update(runtime, |compositor: &mut Compositor| {
                    compositor.primary_output = Some(names.identity().to_owned());
                });
            }
        },
    );
    let locked = bind(move |runtime| is_primary(runtime, store, &name));
    setting_row_desc(
        "Use as primary",
        "The dock and notifications show here unless set otherwise",
        toggle(on).label("Use as primary").disabled(locked),
    )
}

fn role_row(
    cx: &mut Cx,
    store: Store,
    name: String,
    names: OutputNames,
    (role, title, description): (ShellRole, &'static str, &'static str),
) -> impl View + use<> {
    let reading = names.clone();
    let writing = name.clone();
    let on = bridge(
        cx,
        move |runtime| {
            let primary = is_primary(runtime, store, &name);
            store.section(runtime, |compositor: &Compositor| {
                reading
                    .setting(compositor)
                    .map_or(role.default_shown(primary), |setting| {
                        setting.shows(role, primary)
                    })
            })
        },
        move |runtime, on| {
            let primary = is_primary(runtime, store, &writing);
            store.update(runtime, |compositor: &mut Compositor| {
                show_role(compositor, &names, role, on, primary);
            });
        },
    );
    setting_row_desc(title, description, toggle(on).label(title))
}

/// What the default gives is stored as no choice, so the monitor follows the
/// default again if its primary status changes.
fn show_role(
    compositor: &mut Compositor,
    names: &OutputNames,
    role: ShellRole,
    shown: bool,
    primary: bool,
) {
    let choice = (shown != role.default_shown(primary)).then_some(shown);
    names.set_override(compositor, |setting| role_choice(setting, role), choice);
}

fn role_choice(setting: &mut OutputSetting, role: ShellRole) -> &mut Option<bool> {
    match role {
        ShellRole::Bar => &mut setting.bar,
        ShellRole::Dock => &mut setting.dock,
        ShellRole::Notifications => &mut setting.notifications,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pages::display::output_names::names;

    const IDENTITY: &str = "Dell U2720Q 7XK2";

    #[test]
    fn showing_what_the_default_shows_writes_no_choice() {
        let monitor = names("DP-2", IDENTITY);
        let mut compositor = Compositor::default();

        show_role(&mut compositor, &monitor, ShellRole::Bar, true, false);
        assert!(
            compositor.outputs.is_empty(),
            "no entry only to say nothing"
        );

        show_role(&mut compositor, &monitor, ShellRole::Bar, false, false);
        assert_eq!(
            monitor.setting(&compositor).and_then(|setting| setting.bar),
            Some(false)
        );

        show_role(&mut compositor, &monitor, ShellRole::Bar, true, false);
        assert_eq!(
            monitor.setting(&compositor).map(|setting| setting.bar),
            Some(None),
            "switching the bar back on clears the override"
        );
    }

    #[test]
    fn the_dock_default_follows_the_primary() {
        let monitor = names("DP-2", IDENTITY);
        let mut compositor = Compositor::default();

        show_role(&mut compositor, &monitor, ShellRole::Dock, true, false);
        assert_eq!(
            monitor
                .setting(&compositor)
                .and_then(|setting| setting.dock),
            Some(true),
            "a dock on a secondary monitor is an override"
        );

        show_role(&mut compositor, &monitor, ShellRole::Dock, true, true);
        assert_eq!(
            monitor.setting(&compositor).map(|setting| setting.dock),
            Some(None),
            "the primary shows the dock anyway"
        );
    }
}
