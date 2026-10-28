//! The network this machine is on, and — once asked — what that connection is.
//!
//! Details expand inline instead of opening a sheet. macOS uses a modal here;
//! this app has no modal layer, and inventing one so that four read-only rows
//! have somewhere to live would be the wrong trade.

use crownui::kit::{
    ButtonOptions, ButtonVariant, StatusOptions, Tone, badge, button, spinner, status, use_theme,
};
use crownui::prelude::{
    Cx, Listeners, Runtime, Signal, Styled, View, bind, component, keyed, vstack,
};

use super::parts::{ROW_SPINNER, row_button, send, signal_glyph, with_active};
use crate::layout::{
    inline, live_glyph, setting_row, setting_row_desc, setting_row_icon, settings_card_titled,
    settings_divider, value_text, when,
};
use crate::net::wifi::{ActiveWifi, LinkState, WifiCommand, WifiState};
use crate::util::theme::tint;

pub fn card(_cx: &mut Cx, wifi: Signal<WifiState>, ssid: String) -> impl View + use<> {
    let details_open = move |runtime: &mut Runtime| wifi.with(runtime, |wifi| wifi.details_open);

    let trailing = inline((
        // A badge rather than a status dot: this names a fact about the
        // network, not how the connection attempt is doing.
        when(
            move |runtime| active(runtime, wifi, |active| active.weak_security),
            |_| weak_badge(),
        ),
        keyed(
            move |runtime| with_active(runtime, wifi, |active| active.state),
            |_, state| match state {
                Some(LinkState::Connecting) => inline((
                    spinner().size(ROW_SPINNER),
                    status("Connecting…").tone(Tone::Neutral),
                ))
                .boxed(),
                Some(LinkState::Connected) => status("Connected").tone(Tone::Success).boxed(),
                None => vstack(()).boxed(),
            },
        ),
        button(bind(move |runtime| {
            if details_open(runtime) {
                "Hide Details"
            } else {
                "Details…"
            }
            .to_owned()
        }))
        .variant(ButtonVariant::Secondary)
        .control_size(crownui::kit::ButtonSize::Small)
        .on_click(move |cx, _| wifi.update(cx, |wifi| wifi.details_open = !wifi.details_open)),
    ));

    settings_card_titled(
        "Current Network",
        (
            setting_row_icon(
                live_glyph(move |runtime| {
                    signal_glyph(active(runtime, wifi, |active| active.strength.unwrap_or(0)))
                }),
                ssid,
                trailing,
            ),
            when(details_open, move |_| details(wifi)),
        ),
    )
}

fn active<R: Default>(
    runtime: &mut Runtime,
    wifi: Signal<WifiState>,
    read: impl FnOnce(&ActiveWifi) -> R,
) -> R {
    with_active(runtime, wifi, read).unwrap_or_default()
}

fn weak_badge() -> impl View {
    component(|cx| {
        let theme = use_theme(cx);
        badge("Weak Security", tint(theme, |theme| theme.status.warning))
    })
}

/// What "Details…" reveals: three facts and the one action that undoes them.
///
/// Every value is printed rather than edited, because none of them is this
/// app's to change. Disconnect closes the disclosure on its way out, so the
/// card does not briefly show an expanded body for a connection that no longer
/// exists.
fn details(wifi: Signal<WifiState>) -> impl View {
    let fact = move |read: fn(&ActiveWifi) -> String| {
        value_text(bind(move |runtime| active(runtime, wifi, read)))
    };
    vstack((
        settings_divider(),
        setting_row(
            "IP Address",
            fact(|active| {
                active
                    .ip4_address
                    .clone()
                    .unwrap_or_else(|| "Not assigned".to_owned())
            }),
        ),
        settings_divider(),
        setting_row(
            "Signal",
            fact(|active| match active.strength {
                Some(strength) => format!("{strength}%"),
                None => "Unknown".to_owned(),
            }),
        ),
        settings_divider(),
        setting_row(
            "Security",
            fact(|active| {
                if active.weak_security {
                    "Weak"
                } else {
                    "Standard"
                }
                .to_owned()
            }),
        ),
        settings_divider(),
        setting_row_desc(
            "Disconnect",
            "Leave this network without forgetting it",
            row_button("Disconnect", ButtonVariant::Destructive).on_click(move |cx, _| {
                wifi.update(cx, |wifi| wifi.details_open = false);
                send(cx, wifi, WifiCommand::Disconnect);
            }),
        ),
    ))
}
