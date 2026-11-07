//! The Accounts page: the online accounts the home widgets read — Spotify for
//! the music widget, Google for the calendar.
//!
//! Each provider signs in with an OAuth client the user registers with it, so
//! nothing here is tied to a client CrownOS ships. Signing in opens the
//! browser and waits for it on a loopback listener; the tokens it earns are
//! kept in `~/.config/crownos/accounts/`, readable by the user alone.

mod card;
mod sign_in;
mod state;

use crownos_accounts::Provider;
use crownui::kit::icons;
use crownui::prelude::{AnyView, Cx, View};

use crate::pages::{PageDescriptor, page};
use crate::state::Store;

pub const PAGE: PageDescriptor = PageDescriptor {
    title: "Accounts",
    slug: "accounts",
    icon: icons::CIRCLE_USER,
    keywords: &["spotify", "google", "calendar", "music", "sign in", "oauth"],
    build,
};

fn build(cx: &mut Cx, _store: Store) -> AnyView {
    let cards: Vec<AnyView> = Provider::ALL
        .into_iter()
        .map(|provider| card::account_card(cx, provider).boxed())
        .collect();
    page(&PAGE, cards)
}
