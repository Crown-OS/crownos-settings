//! The Android apps page, driven headlessly over a store set as the daemon
//! and the disk would leave it.

use crowntest::{Role, TestApp, role, text};
use crownui::prelude::component;

use super::fixtures::populate_android;
use super::root::shell;
use crate::net::android::{AndroidService, AndroidState, RuntimeState};
use crate::pages;
use crate::state::Store;

const WIDTH: f32 = 1040.0;
const HEIGHT: f32 = 720.0;
const REGISTRATION_ID: &str = "3f8a1c2b9d7e6054";

fn android_page() -> usize {
    pages::ordinal_of("android").unwrap_or_else(|| panic!("the android slug is registered"))
}

/// The page over a store `populated` as a running Android with Google Play,
/// then `edit`ed.
fn window(populated: bool, edit: fn(&mut AndroidState)) -> TestApp {
    let mut app = TestApp::with_size(
        component(move |cx| {
            let store = Store::defaults(cx);
            if populated {
                populate_android(cx, store);
            }
            store.android.update(cx, edit);
            shell(cx, store, android_page())
        }),
        WIDTH,
        HEIGHT,
    );
    app.settle();
    app
}

#[test]
fn the_page_opens_from_its_slug() {
    let mut app = window(false, |_| {});
    assert!(app.exists(role(Role::ScrollView).name("Android apps")));
    assert!(app.exists(text("Checking…")));
}

#[test]
fn without_android_only_the_status_shows() {
    let mut app = window(false, |android| {
        android.service = AndroidService::NotInstalled;
    });
    assert!(app.exists(text("Not installed")));
    assert!(!app.exists(role(Role::Button).name("Stop")));
    assert!(!app.exists(role(Role::Button).name("Remove…")));
    assert!(!app.exists(text("Register this device")));
}

#[test]
fn a_running_android_shows_its_windows_tier_and_storage() {
    let mut app = window(true, |_| {});
    assert!(app.exists(text("Running · 2 windows")));
    assert!(!app.find(role(Role::Button).name("Stop")).states.disabled);
    assert!(app.exists(text("Venus")));
    assert!(app.exists(text("2026.11.0")));
    assert!(app.exists(text("1.4 GB")));
    assert!(app.exists(text("212 MB")));
    assert!(app.exists(text(REGISTRATION_ID)));
}

#[test]
fn the_idle_choice_and_folder_switches_read_the_android_section() {
    let mut app = window(true, |_| {});
    assert!(
        app.exists(role(Role::ComboBox).name("After 5 minutes")),
        "{}",
        app.semantics_snapshot()
    );
    assert_eq!(
        app.find(role(Role::Switch).name("Share Downloads"))
            .states
            .checked,
        Some(true)
    );
    assert_eq!(
        app.find(role(Role::Switch).name("Share Pictures"))
            .states
            .checked,
        Some(false)
    );
}

#[test]
fn a_stopped_android_cannot_be_stopped_again() {
    let mut app = window(true, |android| {
        if let AndroidService::Answered(status) = &mut android.service {
            status.state = RuntimeState::Stopped;
            status.windows = 0;
        }
    });
    assert!(app.exists(text("Stopped")));
    assert!(app.find(role(Role::Button).name("Stop")).states.disabled);
}

#[test]
fn the_registration_id_copies_as_plain_text() {
    let mut app = window(true, |_| {});
    app.click(role(Role::Button).name("Copy ID")).settle();
    let copied = app
        .clipboard()
        .iter()
        .any(|(_, data)| data.as_slice() == REGISTRATION_ID.as_bytes());
    assert!(copied);
}

#[test]
fn without_google_play_there_is_nothing_to_register() {
    let mut app = window(true, |android| {
        if let Some(config) = &mut android.installed {
            config.google_play = false;
        }
    });
    assert!(!app.exists(text("Register this device")));
    assert!(app.exists(text("Running · 2 windows")));
}

#[test]
fn removing_asks_first_and_cancelling_changes_nothing() {
    let mut app = window(true, |_| {});
    assert!(!app.exists(role(Role::Dialog)));
    app.click(role(Role::Button).name("Remove…")).settle();
    assert!(app.exists(text("Remove Android apps?")));
    app.click(role(Role::Button).name("Cancel")).settle();
    assert!(!app.exists(text("Remove Android apps?")));
    assert!(!app.find(role(Role::Button).name("Reset…")).states.disabled);
}

#[test]
fn confirming_a_reset_runs_it_and_holds_both_buttons() {
    let mut app = window(true, |_| {});
    app.click(role(Role::Button).name("Reset…")).settle();
    assert!(app.exists(text("Reset Android data?")));
    app.click(role(Role::Button).name("Reset")).settle();
    assert!(!app.exists(text("Reset Android data?")));
    assert!(app.exists(text("Resetting…")));
    assert!(app.find(role(Role::Button).name("Remove…")).states.disabled);
}

#[test]
fn a_failed_removal_is_shown_until_dismissed() {
    let mut app = window(true, |android| {
        android.error = Some("Permission was not given.".to_owned());
    });
    assert!(app.exists(text("Permission was not given.")));
    app.click(role(Role::Button).name("Dismiss")).settle();
    assert!(!app.exists(text("Permission was not given.")));
}
