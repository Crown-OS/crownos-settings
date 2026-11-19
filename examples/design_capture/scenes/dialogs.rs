//! One sheet or dialog for each page that has one: joining a secured Wi-Fi
//! network, the Cross-device pairing code, the lock screen's Clock & colour
//! sheet, a plugin's details, the display confirmation, and the page search.

use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Result;
use capture::Carrier;
use crowntest::{Role, role};

use super::{LOCK_SCREEN, open_page, ordinal, page_view};
use crate::fixtures::{CPU_GRAPH, SECURED_NETWORK, logs_of, populate_all};
use crate::net::crownlink::{PairingInvite, PairingSession};
use crate::pages;
use crate::session::Window;
use crate::util::qr::QrMatrix;

const GROUP: &str = "Dialogs & sheets";
const INVITATION: &str = "crownlink://pair?k=Q2Jx7mVt4WcR9pLs2eHn6uYd&h=crown-laptop";
const INVITE_LIFETIME_MS: u64 = 94_000;
/// Past the lock screen preview, to its toolbar and the sheet under it.
const SHEET_SCROLL: f32 = 540.0;

pub fn capture(carrier: &mut Carrier) -> Result<()> {
    let mut window = Window::open(ordinal("Wi-Fi"), populate_all)?;
    if window.click(role(Role::Button).name(format!("Join {SECURED_NETWORK}"))) {
        window.click(role(Role::TextField).name(format!("Password for {SECURED_NETWORK}")));
        window.press("harbor22");
        carrier.capture_state(
            &mut window.app,
            GROUP,
            "Wi-Fi · join with password",
            "Joining a secured network grows a password field in its row",
        )?;
    }

    let mut window = Window::open(ordinal("Cross-device"), |cx, store| {
        populate_all(cx, store);
        let now = now_ms();
        store.cross_device.update(cx, |state| {
            state.now_unix_ms = now;
            state.pairing = Some(PairingSession {
                invite: QrMatrix::encode(INVITATION).ok().map(|qr| PairingInvite {
                    qr,
                    expires_unix_ms: now + INVITE_LIFETIME_MS,
                }),
                ..PairingSession::default()
            });
        });
    })?;
    carrier.capture_state(
        &mut window.app,
        GROUP,
        "Cross-device · pairing code",
        "Connect a device: the invitation to scan, counting down",
    )?;

    let mut window = open_page(LOCK_SCREEN, populate_all)?;
    window.scroll(page_view(LOCK_SCREEN), SHEET_SCROLL);
    carrier.capture_state(
        &mut window.app,
        GROUP,
        "Lock Screen · Clock & colour",
        "The sheet under the preview: layouts, fonts, weight and glass",
    )?;

    let mut window = Window::open(pages::plugins_ordinal(), |cx, store| {
        populate_all(cx, store);
        store.plugins.update(cx, |plugins| {
            plugins.selected = Some(CPU_GRAPH.to_owned());
            plugins.logs = logs_of(CPU_GRAPH);
        });
    })?;
    carrier.capture_state(
        &mut window.app,
        GROUP,
        "Plugins · details",
        "One plugin opened: what it is, its status, its actions and permissions",
    )?;

    let mut window = Window::open(ordinal("Display"), |cx, store| {
        populate_all(cx, store);
        store.outputs.update(cx, |outputs| {
            outputs.pending_revert = Some(outputs.drafts.clone());
        });
    })?;
    carrier.capture_state(
        &mut window.app,
        GROUP,
        "Display · keep these settings?",
        "After Apply: keep or go back before the countdown ends",
    )?;

    let mut window = Window::open(pages::DEFAULT_PAGE, populate_all)?;
    if window.click(role(Role::SearchField).name("Search settings")) {
        window.press("lo");
        carrier.capture_state(
            &mut window.app,
            "Menus & popovers",
            "Page search",
            "Typing in the title bar search offers every page that matches",
        )?;
    }
    Ok(())
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_millis() as u64)
}
