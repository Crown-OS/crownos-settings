//! The App Access page: what an app may do with screen context, app by app,
//! and what happens when an app asks for the first time.

use crownos_config::schema::context_access::{AskOnFirstUse, HostDefault, SandboxedDefault};
use crownos_config::schema::{
    AppKey, ContextAccess, Expiry, FirstUsePolicy, Scope, ScopeSet, allowed_scopes,
};
use crownui::kit::{
    ButtonOptions, ButtonVariant, CalloutOptions, TextInputOptions, Tone, button, callout, icons,
    select, toggle,
};
use crownui::prelude::{
    AnyView, Cx, Listeners, Runtime, Semantic, Signal, Styled, View, hstack, keyed, vstack,
};

use crate::controls::{Options, choice, field_text, switch};
use crate::layout::{
    body_text, card_stack, dismissible_failure, nothing, paragraph, setting_row,
    setting_row_content, setting_row_desc, settings_card, settings_card_titled, settings_divider,
};
use crate::pages::{PageDescriptor, page};
use crate::state::Store;
use crate::util::bridge::bridge;
use crate::util::desktop_apps::installed_desktop_apps;
use crate::util::strict_section::problem_with;

const PATH_WIDTH: f32 = 320.0;
const CONTROL_GAP: f32 = 8.0;

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
    let failure = cx.signal(None);
    page(
        &PAGE,
        (
            broken_file_notice(),
            settings_card(setting_row_content(paragraph(
                "For apps that are not sandboxed, this is a consent choice. Such apps can \
                 still read the screen in other ways. Sandboxed apps are enforced.",
            ))),
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
            add_app_card(cx, store, failure),
            dismissible_failure(failure),
            granted_apps(store),
            refused_apps(store),
        ),
    )
}

/// A warning while `context_access.ron` does not parse; nothing is saved then.
fn broken_file_notice() -> AnyView {
    match problem_with::<ContextAccess>(ContextAccess::SECTION) {
        Some(problem) => callout(Tone::Warning, "context_access.ron does not parse")
            .body(format!(
                "Fix the file by hand; changes here are not saved until then. {problem}"
            ))
            .icon(icons::TRIANGLE_ALERT)
            .boxed(),
        None => nothing().boxed(),
    }
}

/// Gives an app the window list before it asks: an installed app picked by
/// name, or a program that only root can change, by path.
fn add_app_card(cx: &mut Cx, store: Store, failure: Signal<Option<String>>) -> AnyView {
    let apps = installed_desktop_apps();
    let names = cx.signal(apps.iter().map(|app| app.name.clone()).collect::<Vec<_>>());
    let picked = cx.signal(None);
    let add_picked = button("Add")
        .label("Add the picked app")
        .on_click(move |cx, _| {
            let Some(app) = picked.get(cx).and_then(|index: usize| apps.get(index)) else {
                return;
            };
            add_grant(cx, store, &app.key);
        });
    let by_path = field_text(
        cx,
        move |runtime| {
            let _ = store.with_section(runtime, |access: &ContextAccess| access.grants.len());
            String::new()
        },
        move |runtime, typed| match program_key(typed.trim()) {
            Ok(key) => {
                add_grant(runtime, store, &key);
                failure.set(runtime, None);
            }
            Err(problem) => failure.set(runtime, Some(problem)),
        },
    )
    .placeholder("/usr/bin/program")
    .width(PATH_WIDTH);
    settings_card_titled(
        "Add an app",
        (
            setting_row(
                "Installed app",
                hstack((select(names, picked), add_picked)).gap(CONTROL_GAP),
            ),
            settings_divider(),
            setting_row_desc("Program", "A program only root can change", by_path),
        ),
    )
    .boxed()
}

/// The key of a program the user typed: an absolute path to a file that
/// only root can change.
fn program_key(typed: &str) -> Result<AppKey, String> {
    let path = std::path::Path::new(typed);
    if !path.is_absolute() || !path.is_file() {
        return Err(format!("{typed} is not a program"));
    }
    let key = AppKey::HostExecutable {
        path: path.to_path_buf(),
    };
    let home = std::env::var_os("HOME").map(std::path::PathBuf::from);
    if key.is_session_only(home.as_deref()) {
        return Err(format!(
            "{typed} can be changed without root, so it can only be allowed when it asks"
        ));
    }
    Ok(key)
}

/// Gives `key` the window list, unless it already has a grant, and lets it
/// out of the refused list.
fn add_grant(runtime: &mut Runtime, store: Store, key: &AppKey) {
    store.update::<ContextAccess>(runtime, |access| add_app(access, key));
}

/// Gives a new app the window list through [`ContextAccess::grant`], so a
/// program the user can change gets it for this session only. An app with a
/// grant keeps it.
fn add_app(access: &mut ContextAccess, key: &AppKey) {
    if !access.grants.contains_key(&key.to_string()) {
        access.grant(key, ScopeSet::EMPTY.with(Scope::Metadata), Expiry::Never);
    }
}

/// Why `scope` cannot be given to the app at `key`, if it cannot.
fn not_allowed_reason(key: &str, scope: Scope) -> Option<&'static str> {
    let key = key.parse::<AppKey>().ok()?;
    if allowed_scopes(&key).contains(scope) {
        return None;
    }
    Some(if scope == Scope::Engine {
        "Only crown-contextd runs the engine"
    } else {
        "Not for programs that can be changed without root"
    })
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
        let switch = toggle(scope_switch(cx, store, key, scope));
        rows.push(match not_allowed_reason(key, scope) {
            Some(reason) => setting_row_desc(label, reason, switch.disabled(true)).boxed(),
            None => setting_row(label, switch).boxed(),
        });
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
    key.parse::<AppKey>()
        .map_or_else(|_| key.to_owned(), |key| key.display_name())
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
    fn an_added_app_follows_the_session_only_rules() {
        let script = AppKey::Interpreted {
            interpreter: "/usr/bin/python3".into(),
            script: "/srv/bot.py".into(),
        };
        let mut access = ContextAccess::default();
        access.denied.push(script.to_string());
        add_app(&mut access, &script);
        let grant = &access.grants[&script.to_string()];
        assert_eq!(grant.scopes, [Scope::Metadata]);
        assert!(matches!(grant.expires, Expiry::Session(_)));
        assert!(access.denied.is_empty());

        if let Some(grant) = access.grants.get_mut(&script.to_string()) {
            grant.scopes.push(Scope::Text);
        }
        add_app(&mut access, &script);
        assert_eq!(access.grants[&script.to_string()].scopes.len(), 2, "kept");
    }

    #[test]
    fn programs_the_user_can_change_cannot_be_added_by_path() {
        assert!(program_key("relative/path").is_err());
        assert!(program_key("/no/such/program").is_err());
        let temporary = std::env::temp_dir().join(format!("crownsettings-{}", std::process::id()));
        let _ = std::fs::write(&temporary, b"");
        assert!(program_key(&temporary.display().to_string()).is_err());
        let _ = std::fs::remove_file(&temporary);
    }

    #[test]
    fn scopes_a_key_cannot_hold_say_why() {
        let script = "interp:/usr/bin/python3:/srv/bot.py";
        assert_eq!(
            not_allowed_reason(script, Scope::Raw),
            Some("Not for programs that can be changed without root")
        );
        assert_eq!(not_allowed_reason(script, Scope::Text), None);
        assert_eq!(
            not_allowed_reason("sandbox:flatpak/org.example.Notes", Scope::Engine),
            Some("Only crown-contextd runs the engine")
        );
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
