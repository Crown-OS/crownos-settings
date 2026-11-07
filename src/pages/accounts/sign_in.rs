//! Running a sign-in: the browser opens, the wait happens off the UI thread.

use std::time::{Duration, Instant};

use crownos_accounts::SignIn;
use crownui::prelude::{Cx, Signal};

use super::state::{AccountCard, Phase, describe};
use crate::util::browser::open_in_browser;

/// How long the loopback listener waits for the browser to come back.
const SIGN_IN_WINDOW: Duration = Duration::from_secs(5 * 60);

pub fn sign_in(cx: &mut Cx, card: Signal<AccountCard>) {
    let (provider, client) = card.with(cx, |card| (card.provider, card.account.client.clone()));
    let opened = SignIn::start(provider, client)
        .map_err(describe)
        .and_then(|sign_in| {
            open_in_browser(sign_in.url())
                .map(|()| sign_in)
                .map_err(describe)
        });
    match opened {
        Ok(sign_in) => wait_for_browser(cx, card, sign_in),
        Err(message) => card.update(cx, |card| card.fail(message)),
    }
}

fn wait_for_browser(cx: &mut Cx, card: Signal<AccountCard>, sign_in: SignIn) {
    card.update(cx, |card| card.phase = Phase::WaitingForBrowser);
    let deadline = Instant::now() + SIGN_IN_WINDOW;
    let finished = tokio::task::spawn_blocking(move || sign_in.finish(deadline));
    cx.spawn(finished, move |cx, outcome| {
        card.update(cx, |card| card.finish_sign_in(outcome));
    });
}
