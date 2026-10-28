//! The Wi-Fi page: the one page in this app that draws a fact rather than a
//! preference.
//!
//! Every other page here is a form: a key, a control, and the control writes
//! it back. Wi-Fi is not like that. The radio can go down because someone
//! pressed a function key, a network can appear because someone walked into a
//! café, and a join can fail three seconds after the click that started it. So
//! this page renders [`WifiState`] — whatever the backend last said — and every
//! affordance on it pushes a [`WifiCommand`](crate::net::wifi::WifiCommand)
//! and then waits to be told what happened.
//!
//! ## One screen at a time
//!
//! There are four mutually exclusive things the page can be, picked in order
//! of authority: NetworkManager is unreachable; nothing has arrived yet; there
//! is no wireless hardware; or the radio card, followed — while the radio is on
//! — by the current network, known networks and other networks. Each card is
//! built when its screen arrives and is otherwise bound to the snapshot, so a
//! signal-strength refresh repaints a glyph, not a page.
//!
//! ## Everything a row needs is already in the snapshot
//!
//! [`WifiSnapshot`](crate::net::wifi::WifiSnapshot) arrives sorted,
//! partitioned and reduced to booleans, so nothing here filters, sorts or
//! compares strengths. The only derivations are cosmetic: which wifi glyph a
//! strength maps to, and how to word a percentage.

mod current;
mod known;
mod others;
mod parts;
mod radio;

use crownui::kit::{icons, spinner};
use crownui::prelude::{AnyView, Cx, Styled};

use crate::layout::{optional, when};
use crate::net::wifi::WifiState;
use crate::pages::{PageDescriptor, page};
use crate::state::Store;
use parts::{ROW_SPINNER, error_card, muted_icon, quiet_card};

pub const PAGE: PageDescriptor = PageDescriptor {
    title: "Wi-Fi",
    slug: "network",
    icon: icons::WIFI,
    build,
};

/// Which of the page's states applies.
///
/// The order is the order of authority: an unreachable NetworkManager outranks
/// a stale snapshot, and no snapshot at all outranks everything below.
#[derive(Debug, Clone, PartialEq)]
enum Screen {
    Offline(String),
    Searching,
    NoAdapter,
    Ready,
}

fn screen(wifi: &WifiState) -> Screen {
    if let Some(reason) = wifi.unavailable.as_deref() {
        return Screen::Offline(reason.to_owned());
    }
    match wifi.snapshot.as_ref() {
        None => Screen::Searching,
        Some(snapshot) if !snapshot.present => Screen::NoAdapter,
        Some(_) => Screen::Ready,
    }
}

/// Whether the radio card has company: networks only mean something with the
/// radio on, and a list of networks you cannot join is a list of
/// disappointments.
fn listening(wifi: &WifiState) -> bool {
    screen(wifi) == Screen::Ready
        && wifi
            .snapshot
            .as_ref()
            .is_some_and(|snapshot| snapshot.radio_enabled)
}

fn build(_cx: &mut Cx, store: Store) -> AnyView {
    let wifi = store.wifi;
    let on_screen = move |wanted: Screen| {
        move |runtime: &mut _| wifi.with(runtime, |wifi| screen(wifi) == wanted)
    };

    page(
        &PAGE,
        (
            // NetworkManager's own words, verbatim: they name the D-Bus
            // failure, and paraphrasing them would cost the only diagnostic.
            optional(
                move |runtime| match wifi.with(runtime, screen) {
                    Screen::Offline(reason) => Some(reason),
                    _ => None,
                },
                |_, reason| quiet_card(muted_icon(icons::WIFI_OFF), reason),
            ),
            when(on_screen(Screen::Searching), |_| {
                quiet_card(
                    spinner().size(ROW_SPINNER),
                    "Looking for networks…".to_owned(),
                )
            }),
            when(on_screen(Screen::NoAdapter), |_| {
                quiet_card(
                    muted_icon(icons::WIFI_OFF),
                    "No Wi-Fi adapter found".to_owned(),
                )
            }),
            when(on_screen(Screen::Ready), move |cx| radio::card(cx, wifi)),
            // Above the networks and below the radio: the error is almost
            // always about something in the list underneath, and putting it
            // here means it cannot be scrolled past. An unreachable
            // NetworkManager clears it, so it never sits under the offline
            // card.
            optional(
                move |runtime| wifi.with(runtime, |wifi| wifi.error.clone()),
                move |_, message| error_card(wifi, message),
            ),
            optional(
                move |runtime| {
                    wifi.with(runtime, |state| {
                        listening(state)
                            .then(|| state.snapshot.as_ref()?.connection.as_ref())
                            .flatten()
                            .map(|active| active.ssid.clone())
                    })
                },
                move |cx, ssid| current::card(cx, wifi, ssid),
            ),
            when(
                move |runtime| {
                    wifi.with(runtime, |state| {
                        listening(state)
                            && state
                                .snapshot
                                .as_ref()
                                .is_some_and(|snapshot| !snapshot.known.is_empty())
                    })
                },
                move |cx| known::card(cx, wifi),
            ),
            when(
                move |runtime| wifi.with(runtime, listening),
                move |cx| others::card(cx, wifi),
            ),
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::wifi::WifiSnapshot;

    #[test]
    fn the_screen_follows_the_order_of_authority() {
        let mut state = WifiState::default();
        assert_eq!(screen(&state), Screen::Searching, "nothing heard yet");

        state.snapshot = Some(WifiSnapshot::default());
        assert_eq!(screen(&state), Screen::NoAdapter);

        state.snapshot = Some(WifiSnapshot {
            present: true,
            ..WifiSnapshot::default()
        });
        assert_eq!(screen(&state), Screen::Ready);
        assert!(!listening(&state), "the radio is off");

        state.unavailable = Some("gone".to_owned());
        assert_eq!(screen(&state), Screen::Offline("gone".to_owned()));
    }
}
