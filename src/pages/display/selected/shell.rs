//! Which monitor is primary, and which shell surfaces each monitor shows.

use crownos_config::schema::{Compositor, OutputSetting, ShellRole};
use crownui::kit::toggle;
use crownui::prelude::{Cx, Runtime, View, vstack};

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

fn primary_row(cx: &mut Cx, store: Store, name: String, names: OutputNames) -> impl View + use<> {
    let on = bridge(
        cx,
        move |runtime| is_primary(runtime, store, &name),
        move |runtime, on| {
            store.update(runtime, |compositor: &mut Compositor| {
                let configured_here = compositor
                    .primary_output
                    .as_deref()
                    .is_some_and(|configured| names.is(configured));
                if on {
                    compositor.primary_output = Some(names.identity().to_owned());
                } else if configured_here {
                    compositor.primary_output = None;
                }
            });
        },
    );
    setting_row_desc(
        "Use as primary",
        "The dock and notifications show here unless set otherwise",
        toggle(on),
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
            store.update(runtime, |compositor: &mut Compositor| {
                *role_choice(names.entry(compositor), role) = Some(on);
            });
        },
    );
    setting_row_desc(title, description, toggle(on))
}

fn role_choice(setting: &mut OutputSetting, role: ShellRole) -> &mut Option<bool> {
    match role {
        ShellRole::Bar => &mut setting.bar,
        ShellRole::Dock => &mut setting.dock,
        ShellRole::Notifications => &mut setting.notifications,
    }
}
