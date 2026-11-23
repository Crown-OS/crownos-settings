//! The App Access page: what an app may do with screen context, app by app,
//! and what happens when an app asks for the first time.

use crownos_config::schema::context_access::{AskOnFirstUse, HostDefault, SandboxedDefault};
use crownos_config::schema::{AppKey, ContextAccess, Expiry, FirstUsePolicy, Scope};
use crownui::kit::{ButtonOptions, ButtonVariant, button, icons, toggle};
use crownui::prelude::{AnyView, Cx, Listeners, Semantic, View, keyed, vstack};

use crate::controls::{Options, choice, switch};
use crate::layout::{
    body_text, card_stack, setting_row, setting_row_content, setting_row_desc,
    settings_card_titled, settings_divider,
};
use crate::pages::{PageDescriptor, page};
use crate::state::Store;
use crate::util::bridge::bridge;

static FIRST_USE: Options<FirstUsePolicy> = Options::new(&[
    ("Ask", FirstUsePolicy::Ask),
    ("Don't allow", FirstUsePolicy::Deny),
]);

/// Every scope with what the user calls it, in the order the page lists them.
const SCOPES: [(Scope, &str); 10] = [
    (Scope::Metadata, "See the window list"),
    (Scope::Text, "See window titles and text"),
    (Scope::Raw, "See screen pixels"),
    (Scope::Pause, "Pause observation"),
    (Scope::MemoryWrite, "Add to memory"),
    (Scope::MemoryDelete, "Delete from memory"),
    (Scope::WaylandWindowControl, "Move and close windows"),
    (Scope::WaylandAgentCursor, "Drive an agent cursor"),
    (Scope::PolicyAdmin, "Change these privacy settings"),
    (Scope::Engine, "Run the context engine"),
];

pub const PAGE: PageDescriptor = PageDescriptor {
    title: "App Access",
    slug: "app-access",
    icon: icons::SHIELD_USER,
    keywords: &[
        "privacy",
        "permissions",
        "grants",
        "revoke",
        "screen context",
        "agents",
        "ai",
    ],
    build,
};

fn build(cx: &mut Cx, store: Store) -> AnyView {
    page(
        &PAGE,
        (
            settings_card_titled(
                "When an app asks",
                (
                    setting_row_desc(
                        "Ask the first time",
                        "Off, an app without access is refused",
                        switch(cx, store, AskOnFirstUse),
                    ),
                    settings_divider(),
                    setting_row(
                        "Apps on this computer",
                        choice(cx, store, HostDefault, &FIRST_USE),
                    ),
                    settings_divider(),
                    setting_row(
                        "Sandboxed apps",
                        choice(cx, store, SandboxedDefault, &FIRST_USE),
                    ),
                ),
            ),
            granted_apps(store),
            refused_apps(store),
        ),
    )
}

/// One card per app with a grant, rebuilt when an app comes or goes.
fn granted_apps(store: Store) -> impl View {
    keyed(
        move |runtime| {
            store.with_section(runtime, |access: &ContextAccess| {
                access.grants.keys().cloned().collect::<Vec<_>>()
            })
        },
        move |cx, keys| {
            if keys.is_empty() {
                return settings_card_titled(
                    "Apps with access",
                    setting_row_content(body_text("No app has access yet.")),
                )
                .boxed();
            }
            card_stack(
                keys.iter()
                    .map(|key| app_card(cx, store, key))
                    .collect::<Vec<_>>(),
            )
            .boxed()
        },
    )
}

fn app_card(cx: &mut Cx, store: Store, key: &str) -> AnyView {
    let mut rows: Vec<AnyView> = vec![
        setting_row_desc(
            app_name(key),
            lifetime(store, key, cx),
            revoke_button(store, key),
        )
        .boxed(),
    ];
    for (scope, label) in SCOPES {
        rows.push(settings_divider().boxed());
        rows.push(setting_row(label, toggle(scope_switch(cx, store, key, scope))).boxed());
    }
    settings_card_titled(&app_name(key), rows).boxed()
}

/// Whether the app at `key` holds `scope`, written back to its grant.
fn scope_switch(
    cx: &mut Cx,
    store: Store,
    key: &str,
    scope: Scope,
) -> crownui::prelude::Signal<bool> {
    let (read_key, write_key) = (key.to_owned(), key.to_owned());
    bridge(
        cx,
        move |runtime| {
            store.with_section(runtime, |access: &ContextAccess| {
                access
                    .grants
                    .get(&read_key)
                    .is_some_and(|grant| grant.scopes.contains(&scope))
            })
        },
        move |runtime, on| {
            store.update::<ContextAccess>(runtime, |access| {
                if let Some(grant) = access.grants.get_mut(&write_key) {
                    grant.scopes.retain(|held| *held != scope);
                    if on {
                        grant.scopes.push(scope);
                    }
                }
            });
        },
    )
}

/// Takes the grant away and remembers the refusal, so the app is not asked
/// again until the user allows it below.
fn revoke_button(store: Store, key: &str) -> impl View + use<> {
    let revoked = key.to_owned();
    button("Revoke")
        .variant(ButtonVariant::Destructive)
        .label(format!("Revoke access for {}", app_name(key)))
        .on_click(move |cx, _| {
            store.update::<ContextAccess>(cx, |access| refuse(access, &revoked));
        })
}

fn refuse(access: &mut ContextAccess, key: &str) {
    access.grants.remove(key);
    if !access.denied.iter().any(|denied| denied == key) {
        access.denied.push(key.to_owned());
    }
}

/// The apps the user refused, each with a way to let it ask again.
fn refused_apps(store: Store) -> impl View {
    keyed(
        move |runtime| store.with_section(runtime, |access: &ContextAccess| access.denied.clone()),
        move |_, denied| {
            if denied.is_empty() {
                return vstack(()).boxed();
            }
            let rows: Vec<AnyView> = denied
                .iter()
                .enumerate()
                .flat_map(|(index, key)| {
                    let forgiven = key.clone();
                    let unblock = button("Ask again")
                        .variant(ButtonVariant::Ghost)
                        .label(format!("Let {} ask again", app_name(key)))
                        .on_click(move |cx, _| {
                            store.update::<ContextAccess>(cx, |access| {
                                access.denied.retain(|denied| *denied != forgiven);
                            });
                        });
                    let divider = (index > 0).then(|| settings_divider().boxed());
                    divider.into_iter().chain([setting_row_desc(
                        app_name(key),
                        key.clone(),
                        unblock,
                    )
                    .boxed()])
                })
                .collect();
            settings_card_titled("Refused apps", rows).boxed()
        },
    )
}

/// How long a grant lasts, in the words of the row under the app's name.
fn lifetime(store: Store, key: &str, cx: &mut Cx) -> String {
    let key = key.to_owned();
    cx.untrack(|runtime| {
        store.with_section(runtime, |access: &ContextAccess| {
            match access.grants.get(&key).map(|grant| &grant.expires) {
                Some(Expiry::Session(_)) => format!("{key} · this session only"),
                Some(Expiry::At(_)) => format!("{key} · for a limited time"),
                Some(Expiry::Never) | None => key.clone(),
            }
        })
    })
}

/// The name the user knows an app by: its app id, program or script.
fn app_name(key: &str) -> String {
    let file_name = |path: &std::path::Path| {
        path.file_name().map_or_else(
            || key.to_owned(),
            |name| name.to_string_lossy().into_owned(),
        )
    };
    match key.parse::<AppKey>() {
        Ok(AppKey::Sandboxed { app_id, .. }) => app_id,
        Ok(AppKey::Spawned { component }) => component,
        Ok(AppKey::HostExecutable { path }) => file_name(&path),
        Ok(AppKey::Interpreted { script, .. }) => file_name(&script),
        Err(_) => key.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apps_are_named_by_what_the_user_knows() {
        assert_eq!(
            app_name("exe:/usr/bin/crownos-computer-use"),
            "crownos-computer-use"
        );
        assert_eq!(
            app_name("sandbox:flatpak/org.example.Notes"),
            "org.example.Notes"
        );
        assert_eq!(app_name("interp:/usr/bin/python3:/srv/bot.py"), "bot.py");
        assert_eq!(app_name("not a key"), "not a key");
    }

    #[test]
    fn revoking_moves_the_app_to_the_refused_list() {
        let key = "exe:/usr/bin/agent";
        let mut access = ContextAccess::default();
        access.grants.insert(key.to_owned(), Default::default());
        refuse(&mut access, key);
        refuse(&mut access, key);
        assert!(access.grants.is_empty());
        assert_eq!(access.denied, [key]);
    }

    #[test]
    fn every_scope_is_listed_once() {
        for scope in Scope::ALL {
            assert_eq!(
                SCOPES.iter().filter(|(listed, _)| *listed == scope).count(),
                1,
                "{scope}"
            );
        }
    }
}
