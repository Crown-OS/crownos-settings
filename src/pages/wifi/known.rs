//! The saved networks that are in range.

use crownui::kit::{
    ButtonVariant, MenuEntry, icons, popup_menu, popup_menu_item, popup_menu_separator, spinner,
};
use crownui::prelude::{Cx, Listeners, Semantic, Signal, Styled, View, for_each, keyed, vstack};

use super::parts::{ROW_SPINNER, lock_icon, row_button, send, signal_icon};
use crate::layout::{
    Listed, hairline_unless_first, inline, listed, live_glyph, nothing, setting_row_icon,
    settings_card_titled, when,
};
use crate::net::wifi::{KnownNetwork, WifiCommand, WifiState};
use crate::util::bridge::derived;

/// No checkmark, but the SSID still starts on the same pixel as the joined
/// network's above it.
const BLANK_ICON: &str = "";

type Row = Signal<Listed<KnownNetwork>>;

pub fn card(cx: &mut Cx, wifi: Signal<WifiState>) -> impl View + use<> {
    let known = derived(cx, move |runtime| {
        wifi.with(runtime, |wifi| {
            listed(
                wifi.snapshot
                    .iter()
                    .flat_map(|snapshot| snapshot.known.iter().cloned()),
            )
        })
    });
    settings_card_titled(
        "Known Networks",
        (for_each(
            known,
            |row: &Listed<KnownNetwork>| row.item.ssid.clone(),
            move |cx, row| known_row(cx, wifi, row),
        ),),
    )
}

/// One saved network: a checkmark if it is the one we are on, its name, and
/// everything that can be done to it.
///
/// The join affordance is a Ghost button rather than a click anywhere on the
/// row: Ghost exists for repeated actions inside a list, so the row keeps the
/// typography of every other row and only the verb lights up under the
/// pointer.
fn known_row(cx: &mut Cx, wifi: Signal<WifiState>, row: Row) -> impl View + use<> {
    let ssid = row.with_untracked(cx, |row| row.item.ssid.clone());
    let (joining_ssid, named) = (ssid.clone(), format!("Connect to {ssid}"));

    let action = keyed(
        move |runtime| {
            let joining = wifi.with(runtime, |wifi| {
                wifi.connecting.as_deref() == Some(joining_ssid.as_str())
            });
            (joining, row.with(runtime, |row| row.item.joined))
        },
        move |_, (joining, joined)| match (joining, joined) {
            (true, _) => spinner().size(ROW_SPINNER).boxed(),
            (false, false) => row_button("Connect", ButtonVariant::Ghost)
                .label(named.clone())
                .on_click(move |cx, _| {
                    // No password: NetworkManager already holds this
                    // profile's secret, and offering a blank one would
                    // overwrite it.
                    let ssid = row.with_untracked(cx, |row| row.item.ssid.clone());
                    send(
                        cx,
                        wifi,
                        WifiCommand::Connect {
                            ssid,
                            password: None,
                        },
                    );
                })
                .boxed(),
            (false, true) => nothing().boxed(),
        },
    );

    vstack((
        hairline_unless_first(row),
        setting_row_icon(
            live_glyph(move |runtime| {
                if row.with(runtime, |row| row.item.joined) {
                    icons::CHECK
                } else {
                    BLANK_ICON
                }
            }),
            ssid.clone(),
            inline((
                when(
                    move |runtime| row.with(runtime, |row| row.item.secured),
                    move |_| lock_icon(move |runtime| row.with(runtime, |row| row.item.weak)),
                ),
                signal_icon(move |runtime| row.with(runtime, |row| row.item.strength)),
                action,
                known_menu(cx, wifi, row, ssid),
            )),
        ),
    ))
}

/// macOS's ⋯ menu for a saved network, minus "Copy Password" — `nmrs` exposes
/// no way to read a secret back out of NetworkManager.
///
/// Auto-join and forget are both `uuid` operations, and a group whose saved
/// profile the snapshot could not pin down carries an empty one; forgetting is
/// still possible in that case *if* the network is the one joined, because
/// that path goes by name. Every entry reads the row at the moment it is
/// chosen, so a list re-sorting under an open menu cannot redirect it.
fn known_menu(cx: &mut Cx, wifi: Signal<WifiState>, row: Row, ssid: String) -> impl View + use<> {
    let auto_join = derived(cx, move |runtime| {
        row.with(runtime, |row| row.item.auto_join)
    });
    let anonymous = derived(cx, move |runtime| {
        row.with(runtime, |row| row.item.uuid.is_empty())
    });
    let not_joined = derived(cx, move |runtime| row.with(runtime, |row| !row.item.joined));
    let unforgettable = derived(cx, move |runtime| {
        row.with(runtime, |row| row.item.uuid.is_empty() && !row.item.joined)
    });

    let entries: [MenuEntry; 5] = [
        popup_menu_item("Auto-Join", move |cx| {
            let (uuid, enabled) =
                row.with_untracked(cx, |row| (row.item.uuid.clone(), !row.item.auto_join));
            send(cx, wifi, WifiCommand::SetAutoJoin { uuid, enabled });
        })
        .checked(auto_join)
        .disabled(anonymous),
        popup_menu_separator(),
        // The details section belongs to the current connection, so this is
        // "show me this network's settings" only while this network is it.
        popup_menu_item("Network Settings…", move |cx| {
            wifi.update(cx, |wifi| wifi.details_open = true);
        })
        .disabled(not_joined),
        popup_menu_separator(),
        popup_menu_item("Forget This Network…", move |cx| {
            let (ssid, uuid) =
                row.with_untracked(cx, |row| (row.item.ssid.clone(), row.item.uuid.clone()));
            send(cx, wifi, WifiCommand::Forget { ssid, uuid });
        })
        .destructive()
        .disabled(unforgettable),
    ];
    popup_menu(icons::ELLIPSIS, format!("Options for {ssid}"), entries)
}
