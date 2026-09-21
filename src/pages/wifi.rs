//! The Wi-Fi page: the one page in this app that draws a fact rather than a
//! preference.
//!
//! Every other page here is a form. It reads `store.get(some::Key)`, hands the
//! value to a control, and the control writes it back — and because the value
//! only ever changes when the user changes it, the page can be a straight line
//! from key to widget. Wi-Fi is not like that. The radio can go down because
//! someone pressed a function key, a network can appear because someone walked
//! into a café, and a join can fail three seconds after the click that started
//! it. So this page is written the other way round: it renders
//! [`WifiState`] — whatever the backend last said — and every affordance on it
//! pushes a [`WifiCommand`] and then waits to be told what happened.
//!
//! That inversion is what makes the code below shaped the way it is.
//!
//! ## One function per state, not one function with the states in it
//!
//! There are five mutually exclusive things this page can be, and [`build`]
//! picks between them before composing anything:
//!
//! * NetworkManager is unreachable ⇒ one quiet card saying so;
//! * nothing has arrived yet ⇒ a spinner and "Looking for networks…" — this is
//!   also what the page-building test renders, since `Store::defaults()` has
//!   never heard from a worker;
//! * there is no wireless hardware ⇒ one quiet card saying that instead;
//! * the radio is off ⇒ the radio card alone, because a list of networks you
//!   cannot join is a list of disappointments;
//! * otherwise ⇒ the full stack: radio, the current network, known networks,
//!   other networks.
//!
//! ## Everything a row needs is already in the snapshot
//!
//! [`WifiSnapshot`] arrives sorted, partitioned and reduced to booleans. So no
//! function in this module filters, sorts, compares strengths or works out
//! whether something is the current network — it reads fields and picks
//! widgets. The only derivations here are cosmetic: which of the four wifi
//! glyphs a strength maps to, and how to word a percentage.
//!
//! ## Where the transient bits live
//!
//! A xilem view is rebuilt from scratch on every message, so it can hold
//! nothing. Three pieces of "the user is part-way through something" therefore
//! live in [`WifiState`] and are read back here: `connecting` (which row shows
//! a spinner), `prompt` (which row has grown a password field), and
//! `details_open` (whether the current network's card is expanded). Each of
//! them is set by a callback in this module and cleared either by a callback
//! here or by the worker's own event folding.
//!
//! ## Composition
//!
//! Rows come from [`crate::layout`], which owns this app's spacing and type
//! scale, so nothing here names a padding or a text size. The two shapes that
//! module cannot know about — a network row's trailing cluster of icons and
//! buttons, and a card that holds a sentence rather than a setting — are built
//! out of [`setting_row_icon`]'s and
//! [`setting_row_content`](crate::layout::setting_row_content)'s hospitality
//! rather than out of new constants.

use blinc_icons::icons;
use crownuikit::config::theme;
use crownuikit::widgets::{
    ButtonVariant, Tone, badge, button, icon, popup_menu, popup_menu_item, popup_menu_separator,
    spinner, status, text_input, toggle,
};
use xilem::WidgetView;
use xilem::masonry::core::ArcStr;
use xilem::masonry::properties::types::AsUnit;
use xilem::view::{CrossAxisAlignment, FlexSpacer, flex_col, flex_row};

use crate::controls::value_text;
use crate::layout::{
    setting_row, setting_row_content, setting_row_desc, setting_row_icon, settings_card,
    settings_card_titled, settings_divider,
};
use crate::net::wifi::{
    ActiveWifi, KnownNetwork, LinkState, OtherNetwork, PSK_LENGTH, PasswordPrompt, WifiCommand,
    WifiSnapshot, WifiState,
};
use crate::pages::{PageDescriptor, PageView, page};
use crate::state::Store;

// --- MARK: Metrics ---

/// An icon body with no shapes in it.
///
/// [`setting_row_icon`] reserves its icon column whether or not the glyph draws
/// anything, which is exactly what an unjoined network's row wants: no
/// checkmark, but its SSID starting on the same pixel as the joined network's
/// above it. The alternative — a second row constructor that takes a leading
/// spacer — would put this app's icon column width in two places.
const BLANK_ICON: &str = "";

/// Trailing lock and signal glyphs. A step under the row's own leading icon,
/// because they annotate the name rather than introduce it.
const TRAILING_ICON: f64 = 15.0;
/// Gap between the things stacked on the right-hand end of a network row.
const TRAILING_GAP: f64 = 10.0;
/// A spinner sized to sit on a row's baseline without changing its height.
const ROW_SPINNER: f64 = 14.0;
/// Width of the inline password field. Wide enough for a passphrase to be
/// visible as a shape, narrow enough to leave Cancel and Join on the same row.
const PASSWORD_WIDTH: f64 = 200.0;

/// Signal strength percentages at which the wifi glyph gains another arc.
///
/// Four glyphs, so three thresholds; `WIFI_ZERO` is what is left below the
/// lowest. The numbers are the obvious quarters — NetworkManager's percentage
/// is a smoothed estimate and does not deserve a more opinionated scale.
const SIGNAL_TIERS: [(u8, &str); 3] = [
    (75, icons::WIFI),
    (50, icons::WIFI_HIGH),
    (25, icons::WIFI_LOW),
];

pub const PAGE: PageDescriptor = PageDescriptor {
    title: "Wi-Fi",
    icon: icons::WIFI,
    build,
};

// --- MARK: Page ---

/// Pick which of the page's five states applies, then compose it.
///
/// The order of the tests is the order of authority: an unreachable
/// NetworkManager outranks a stale snapshot, and no snapshot at all outranks
/// everything below. [`WifiState`] keeps those two mutually exclusive already —
/// an `Unavailable` event clears the snapshot and a `Snapshot` event clears the
/// unavailability — so this is a restatement of the backend's rule, not a
/// second one.
fn build(store: &Store) -> PageView {
    let wifi = &store.wifi;

    if let Some(reason) = wifi.unavailable.as_deref() {
        return page(&PAGE, (offline_card(reason.to_owned()),));
    }
    let Some(snapshot) = wifi.snapshot.as_ref() else {
        return page(&PAGE, with_error(wifi, searching_card().boxed()));
    };
    page(&PAGE, cards(wifi, snapshot))
}

/// A card, followed by the last failure if there is one.
///
/// The error card cannot be a privilege of the state that has a snapshot. A
/// `publish` that fails before the first one ever lands leaves the page in
/// exactly the state that most needs an explanation — spinning under "Looking
/// for networks…" — and a machine with no adapter can still be told that
/// turning the radio on did not work. So every state that can hold an error
/// draws it, in the same place, with the same dismiss affordance.
fn with_error(wifi: &WifiState, card: PageView) -> Vec<PageView> {
    let mut cards = vec![card];
    if let Some(message) = wifi.error.as_deref() {
        cards.push(error_card(message.to_owned()).boxed());
    }
    cards
}

/// Every card the page shows, in order, for a machine we have heard from.
///
/// Boxed rather than tupled because the list is conditional: which cards exist
/// depends on the radio, on whether anything is joined, and on whether anything
/// is in range.
fn cards(wifi: &WifiState, snapshot: &WifiSnapshot) -> Vec<PageView> {
    if !snapshot.present {
        return with_error(
            wifi,
            quiet_card(muted_icon(icons::WIFI_OFF), "No Wi-Fi adapter found").boxed(),
        );
    }

    // Above the networks and below the radio: the error is almost always about
    // something in the list underneath, and putting it there means it cannot be
    // scrolled past on a machine that can see thirty access points.
    let mut cards = with_error(wifi, radio_card(snapshot).boxed());

    if !snapshot.radio_enabled {
        return cards;
    }
    if let Some(active) = snapshot.connection.as_ref() {
        cards.push(current_card(wifi, active).boxed());
    }
    if !snapshot.known.is_empty() {
        cards.push(known_card(wifi, &snapshot.known).boxed());
    }
    cards.push(others_card(wifi, &snapshot.others).boxed());
    cards
}

// --- MARK: Radio ---

/// The switch itself, plus the one thing that can make it a lie.
///
/// The toggle is bound to NetworkManager's software switch and not to
/// `wifi.ron`: the config file is a mirror the worker keeps up to date, and
/// writing it from here would mean the page believed itself over the hardware.
/// When rfkill has the radio down the switch cannot do anything, so it is
/// disabled and the card says why rather than silently ignoring presses.
fn radio_card(snapshot: &WifiSnapshot) -> impl WidgetView<Store> + use<> {
    let blocked = snapshot.hardware_blocked;
    let glyph = if snapshot.radio_enabled {
        icons::WIFI
    } else {
        icons::WIFI_OFF
    };

    let radio = toggle("", snapshot.radio_enabled, |store: &mut Store, on| {
        store.wifi.send(WifiCommand::SetRadio(on));
    })
    .disabled(blocked);

    let mut rows: Vec<PageView> = vec![setting_row_icon(glyph, "Wi-Fi", radio).boxed()];
    if blocked {
        rows.push(settings_divider().boxed());
        rows.push(
            setting_row_content(status("Turned off by a hardware switch").tone(Tone::Warning))
                .boxed(),
        );
    }
    settings_card_titled("Wi-Fi", rows)
}

// --- MARK: Current network ---

/// The network this machine is on, and — once asked — what that connection is.
///
/// Details expand inline instead of opening a sheet. macOS uses a modal here;
/// this app has no modal layer, and inventing one so that four read-only rows
/// have somewhere to live would be the wrong trade.
fn current_card(wifi: &WifiState, active: &ActiveWifi) -> impl WidgetView<Store> + use<> {
    let open = wifi.details_open;

    let mut trailing: Vec<PageView> = Vec::new();
    if active.weak_security {
        // A badge rather than a `status()` dot: this is naming a fact about
        // the network (its security is weak), not reporting how the
        // connection attempt is doing the way the states below it are.
        trailing.push(badge("Weak Security", theme().status.warning).boxed());
    }
    match active.state {
        LinkState::Connecting => {
            trailing.push(spinner().size(ROW_SPINNER).boxed());
            trailing.push(status("Connecting…").tone(Tone::Neutral).boxed());
        }
        LinkState::Connected => {
            trailing.push(status("Connected").tone(Tone::Success).boxed());
        }
    }
    trailing.push(
        button(
            if open { "Hide Details" } else { "Details…" },
            |store: &mut Store| store.wifi.details_open = !store.wifi.details_open,
        )
        .variant(ButtonVariant::Secondary)
        .small()
        .boxed(),
    );

    let mut rows: Vec<PageView> = vec![
        setting_row_icon(
            signal_glyph(active.strength.unwrap_or(0)),
            active.ssid.clone(),
            cluster(trailing),
        )
        .boxed(),
    ];
    if open {
        rows.extend(detail_rows(active));
    }

    settings_card_titled("Current Network", rows)
}

/// What "Details…" reveals: three facts and the one action that undoes them.
///
/// Every value is printed rather than edited, because none of them is this
/// app's to change — the address comes from DHCP and the signal from the air.
/// Disconnect closes the disclosure on its way out, so the card does not
/// briefly show an expanded body for a connection that no longer exists.
fn detail_rows(active: &ActiveWifi) -> Vec<PageView> {
    let address = active
        .ip4_address
        .clone()
        .unwrap_or_else(|| "Not assigned".to_owned());
    let signal = match active.strength {
        Some(strength) => format!("{strength}%"),
        None => "Unknown".to_owned(),
    };
    let security = if active.weak_security {
        "Weak"
    } else {
        "Standard"
    };

    vec![
        settings_divider().boxed(),
        setting_row("IP Address", value_text(address)).boxed(),
        settings_divider().boxed(),
        setting_row("Signal", value_text(signal)).boxed(),
        settings_divider().boxed(),
        setting_row("Security", value_text(security)).boxed(),
        settings_divider().boxed(),
        setting_row_desc(
            "Disconnect",
            "Leave this network without forgetting it",
            button("Disconnect", |store: &mut Store| {
                store.wifi.details_open = false;
                store.wifi.send(WifiCommand::Disconnect);
            })
            .variant(ButtonVariant::Destructive)
            .small(),
        )
        .boxed(),
    ]
}

// --- MARK: Known networks ---

/// The saved networks that are in range.
fn known_card(wifi: &WifiState, known: &[KnownNetwork]) -> impl WidgetView<Store> + use<> {
    let rows = known.iter().map(|network| known_row(wifi, network));
    settings_card_titled("Known Networks", separated(rows))
}

/// One saved network: a checkmark if it is the one we are on, its name, and
/// everything that can be done to it.
///
/// The join affordance is a Ghost button rather than a click anywhere on the
/// row. The kit has no clickable-row primitive, and Ghost exists for precisely
/// this — "repeated actions inside a list, where a chip per row would be visual
/// noise" — so the row keeps the same typography as every other row in the app
/// and only the verb lights up under the pointer.
fn known_row(wifi: &WifiState, network: &KnownNetwork) -> PageView {
    let joining = wifi.connecting.as_deref() == Some(network.ssid.as_str());

    let mut trailing: Vec<PageView> = Vec::new();
    if network.secured {
        trailing.push(lock_icon(network.weak).boxed());
    }
    trailing.push(signal_icon(network.strength).boxed());

    if joining {
        trailing.push(spinner().size(ROW_SPINNER).boxed());
    } else if !network.joined {
        let ssid = network.ssid.clone();
        trailing.push(
            button("Connect", move |store: &mut Store| {
                // No password: NetworkManager already holds this profile's
                // secret, and offering a blank one would overwrite it.
                store.wifi.send(WifiCommand::Connect {
                    ssid: ssid.clone(),
                    password: None,
                });
            })
            .variant(ButtonVariant::Ghost)
            .small()
            .boxed(),
        );
    }
    trailing.push(known_menu(network).boxed());

    setting_row_icon(
        if network.joined {
            icons::CHECK
        } else {
            BLANK_ICON
        },
        network.ssid.clone(),
        cluster(trailing),
    )
    .boxed()
}

/// macOS's ⋯ menu for a saved network, minus "Copy Password" — `nmrs` exposes
/// no way to read a secret back out of NetworkManager, and a menu entry that
/// cannot work is worse than one that isn't there.
///
/// Two entries are conditional on identifiers rather than on taste. Auto-join
/// and forget are both `uuid` operations, and a group whose saved profile the
/// snapshot could not pin down carries an empty one; forgetting is still
/// possible in that case *if* the network is the one joined, because that path
/// goes by name. So each entry is disabled exactly when the call behind it has
/// nothing to address.
fn known_menu(network: &KnownNetwork) -> impl WidgetView<Store> + use<> {
    let auto_join = network.auto_join;
    let joined = network.joined;
    let anonymous = network.uuid.is_empty();

    let auto_join_uuid = network.uuid.clone();
    let forget_uuid = network.uuid.clone();
    let forget_ssid = network.ssid.clone();

    popup_menu(
        icons::ELLIPSIS,
        [
            popup_menu_item("Auto-Join", move |store: &mut Store| {
                store.wifi.send(WifiCommand::SetAutoJoin {
                    uuid: auto_join_uuid.clone(),
                    enabled: !auto_join,
                });
            })
            .checked(auto_join)
            .disabled(anonymous),
            popup_menu_separator(),
            // The details section belongs to the current connection, so this is
            // "show me this network's settings" only while this network is it.
            popup_menu_item("Network Settings…", |store: &mut Store| {
                store.wifi.details_open = true;
            })
            .disabled(!joined),
            popup_menu_separator(),
            popup_menu_item("Forget This Network…", move |store: &mut Store| {
                store.wifi.send(WifiCommand::Forget {
                    ssid: forget_ssid.clone(),
                    uuid: forget_uuid.clone(),
                });
            })
            .destructive()
            .disabled(anonymous && !joined),
        ],
    )
    // The identity is what keeps the destructive entry honest: rows re-sort by
    // strength while a menu can be open, and a menu without one would dispatch
    // into whichever network had drifted under it. The SSID names the subject;
    // the row's position never does.
    .identity(network.ssid.clone())
}

// --- MARK: Other networks ---

/// Everything visible that this machine has never saved.
///
/// The card is drawn even when it is empty, because "we looked and there is
/// nothing" is information — silently omitting it would read as a page that
/// hadn't finished loading.
fn others_card(wifi: &WifiState, others: &[OtherNetwork]) -> impl WidgetView<Store> + use<> {
    let rows = if others.is_empty() {
        vec![setting_row_content(status("No other networks found").tone(Tone::Neutral)).boxed()]
    } else {
        separated(others.iter().map(|network| other_row(wifi, network)))
    };
    settings_card_titled("Other Networks", rows)
}

/// One unknown network, which may have grown a password field.
///
/// The prompt is part of the row rather than a card of its own so that the
/// field appears attached to the name it belongs to; with thirty access points
/// in range, a password box anywhere else is a password box for nobody in
/// particular.
fn other_row(wifi: &WifiState, network: &OtherNetwork) -> PageView {
    let joining = wifi.connecting.as_deref() == Some(network.ssid.as_str());
    let prompt = wifi
        .prompt
        .as_ref()
        .filter(|open| open.ssid == network.ssid);

    let mut trailing: Vec<PageView> = Vec::new();
    if network.secured {
        trailing.push(lock_icon(network.weak).boxed());
    }
    trailing.push(signal_icon(network.strength).boxed());

    if joining {
        trailing.push(spinner().size(ROW_SPINNER).boxed());
    } else if prompt.is_none() {
        let ssid = network.ssid.clone();
        let secured = network.secured;
        trailing.push(
            button("Join", move |store: &mut Store| {
                if secured {
                    // One step, not two: the click opens the field, and the
                    // field's own Join is what reaches the worker.
                    store.wifi.prompt = Some(PasswordPrompt {
                        ssid: ssid.clone(),
                        password: String::new(),
                    });
                } else {
                    store.wifi.send(WifiCommand::Connect {
                        ssid: ssid.clone(),
                        password: None,
                    });
                }
            })
            .variant(ButtonVariant::Ghost)
            .small()
            .boxed(),
        );
    }

    let header = setting_row(network.ssid.clone(), cluster(trailing));
    match prompt {
        Some(open) => flex_col((header, password_row(open)))
            .gap(0.0.px())
            .cross_axis_alignment(CrossAxisAlignment::Fill)
            .boxed(),
        None => header.boxed(),
    }
}

/// The inline password field for one open prompt.
///
/// Validation is [`PSK_LENGTH`] and nothing else: NetworkManager reports a
/// wrong-length key as an addressing error, which is unreadable, so the button
/// simply does not arm until the length is one NetworkManager will accept. The
/// length is in bytes, because bytes are what NetworkManager counts — see
/// [`PSK_LENGTH`]. Enter does the same thing the button does, and [`join`]
/// re-checks the length because a submit can arrive from a field the button
/// never saw.
fn password_row(prompt: &PasswordPrompt) -> impl WidgetView<Store> + use<> {
    let ready = PSK_LENGTH.contains(&prompt.password.len());
    let submitted = prompt.ssid.clone();
    let pressed = prompt.ssid.clone();
    let typed = prompt.password.clone();

    let field = text_input(prompt.password.clone(), |store: &mut Store, value| {
        // The prompt may have closed under us — a snapshot can land between the
        // keystroke and its message — so this writes only if it is still open.
        if let Some(open) = store.wifi.prompt.as_mut() {
            open.password = value;
        }
    })
    .secret(true)
    .placeholder("Password")
    .width(PASSWORD_WIDTH)
    .on_submit(move |store: &mut Store, value| join(store, &submitted, value));

    let cancel = button("Cancel", |store: &mut Store| store.wifi.prompt = None)
        .variant(ButtonVariant::Ghost)
        .small();

    let confirm = button("Join", move |store: &mut Store| {
        join(store, &pressed, typed.clone());
    })
    .variant(ButtonVariant::Primary)
    .small()
    .disabled(!ready);

    setting_row("Password", cluster_of((field, cancel, confirm)))
}

/// Submit a typed key, if it is one NetworkManager could accept.
///
/// Closing the prompt here as well as in the worker's event folding is
/// deliberate: the round trip to the worker and back is long enough to see, and
/// a field that stays filled after Join looks like a click that missed.
fn join(store: &mut Store, ssid: &str, password: String) {
    if !PSK_LENGTH.contains(&password.len()) {
        return;
    }
    store.wifi.prompt = None;
    store.wifi.send(WifiCommand::Connect {
        ssid: ssid.to_owned(),
        password: Some(password),
    });
}

// --- MARK: Quiet states ---

/// The dismissible last failure.
///
/// A card of its own rather than a line inside another one, because the error
/// outlives whichever row caused it — the network it names may already have
/// gone out of range by the time it is read.
fn error_card(message: String) -> impl WidgetView<Store> + use<> {
    settings_card((setting_row_content(
        flex_row((
            status(message).tone(Tone::Danger),
            FlexSpacer::Flex(1.0),
            button("Dismiss", |store: &mut Store| store.wifi.dismiss_error())
                .variant(ButtonVariant::Ghost)
                .small(),
        ))
        .gap(TRAILING_GAP.px())
        .cross_axis_alignment(CrossAxisAlignment::Center),
    ),))
}

/// Nothing has arrived from the worker yet.
///
/// Also the state `Store::defaults()` is in, which is what the page-building
/// test renders — so this path is the one guaranteed to be exercised on a
/// machine with no NetworkManager at all.
fn searching_card() -> impl WidgetView<Store> + use<> {
    quiet_card(spinner().size(ROW_SPINNER), "Looking for networks…")
}

/// NetworkManager could not be reached. Its own words, verbatim: they name the
/// D-Bus failure, and paraphrasing them would cost the only diagnostic there is.
fn offline_card(reason: String) -> impl WidgetView<Store> + use<> {
    quiet_card(muted_icon(icons::WIFI_OFF), reason)
}

/// A card that says one thing and offers nothing to press.
fn quiet_card<V, M>(leading: V, message: M) -> impl WidgetView<Store> + use<V, M>
where
    V: WidgetView<Store>,
    M: Into<ArcStr>,
{
    settings_card_titled(
        "Wi-Fi",
        (setting_row_content(
            flex_row((leading, value_text(message)))
                .gap(TRAILING_GAP.px())
                .cross_axis_alignment(CrossAxisAlignment::Center),
        ),),
    )
}

// --- MARK: Row parts ---

/// Put a hairline between every pair of rows, and nowhere else.
///
/// [`settings_card`] takes its dividers as ordinary children — the same
/// arrangement the kit's sidebar uses — which is right for a card written out
/// by hand and wrong for one built from a list. This is the list case.
fn separated(rows: impl IntoIterator<Item = PageView>) -> Vec<PageView> {
    let mut out: Vec<PageView> = Vec::new();
    for row in rows {
        if !out.is_empty() {
            out.push(settings_divider().boxed());
        }
        out.push(row);
    }
    out
}

/// The right-hand end of a network row, from a list built up conditionally.
fn cluster(children: Vec<PageView>) -> impl WidgetView<Store> + use<> {
    flex_row(children)
        .gap(TRAILING_GAP.px())
        .cross_axis_alignment(CrossAxisAlignment::Center)
}

/// [`cluster`], for the one place whose contents are known at compile time and
/// would only lose their types by being boxed.
fn cluster_of<Seq>(children: Seq) -> impl WidgetView<Store> + use<Seq>
where
    Seq: xilem::view::FlexSequence<Store> + Send + Sync + 'static,
{
    flex_row(children)
        .gap(TRAILING_GAP.px())
        .cross_axis_alignment(CrossAxisAlignment::Center)
}

/// The padlock a secured network wears, tinted when its security is one of the
/// discouraged ones.
///
/// The warning goes on the lock rather than on the signal glyph because that is
/// what it is about; an open network has no lock and needs none, since "no
/// padlock at all" already says everything "weak" would.
fn lock_icon(weak: bool) -> impl WidgetView<Store> + use<> {
    let color = if weak {
        theme().status.warning
    } else {
        theme().text.icon_muted
    };
    icon(icons::LOCK).size(TRAILING_ICON).color(color)
}

/// The signal glyph for a strength.
fn signal_icon(strength: u8) -> impl WidgetView<Store> + use<> {
    icon(signal_glyph(strength))
        .size(TRAILING_ICON)
        .color(theme().text.icon)
}

/// A muted decoration for the cards that have nothing to report.
fn muted_icon(glyph: &'static str) -> impl WidgetView<Store> + use<> {
    icon(glyph)
        .size(TRAILING_ICON)
        .color(theme().text.icon_muted)
}

/// Which of `blinc_icons`' four wifi glyphs a strength percentage earns.
fn signal_glyph(strength: u8) -> &'static str {
    SIGNAL_TIERS
        .iter()
        .find(|(floor, _)| strength >= *floor)
        .map_or(icons::WIFI_ZERO, |(_, glyph)| *glyph)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The glyph ladder is the page's only real derivation, and an off-by-one
    /// in it is the kind of thing that is invisible until a network sits
    /// exactly on a boundary.
    #[test]
    fn every_strength_lands_on_a_glyph() {
        assert_eq!(signal_glyph(100), icons::WIFI);
        assert_eq!(signal_glyph(75), icons::WIFI);
        assert_eq!(signal_glyph(74), icons::WIFI_HIGH);
        assert_eq!(signal_glyph(50), icons::WIFI_HIGH);
        assert_eq!(signal_glyph(49), icons::WIFI_LOW);
        assert_eq!(signal_glyph(25), icons::WIFI_LOW);
        assert_eq!(signal_glyph(24), icons::WIFI_ZERO);
        assert_eq!(signal_glyph(0), icons::WIFI_ZERO);
    }

    /// `separated` is the one place a card's shape is computed rather than
    /// written out, so the "no trailing hairline" rule is asserted rather than
    /// eyeballed.
    #[test]
    fn hairlines_go_between_rows_and_not_around_them() {
        let row = || value_text("row").boxed();

        assert_eq!(separated([]).len(), 0);
        assert_eq!(separated([row()]).len(), 1, "one row needs no divider");
        assert_eq!(separated([row(), row()]).len(), 3);
        assert_eq!(separated([row(), row(), row()]).len(), 5);
    }
}
