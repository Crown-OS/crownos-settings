//! The radio switch itself, plus the one thing that can make it a lie.
//!
//! The toggle is bound to NetworkManager's software switch and not to
//! `wifi.ron`: the config file is a mirror the worker keeps up to date, and
//! writing it from here would mean the page believed itself over the hardware.
//! When rfkill has the radio down the switch cannot do anything, so it is
//! disabled and the card says why rather than silently ignoring presses.

use crownui::kit::{StatusOptions, Tone, icons, status, toggle};
use crownui::prelude::{Cx, Runtime, Semantic, Signal, View, bind, vstack};

use super::parts::send;
use crate::layout::{
    live_glyph, setting_row_content, setting_row_icon, settings_card_titled, settings_divider, when,
};
use crate::net::wifi::{WifiCommand, WifiSnapshot, WifiState};
use crate::util::bridge::bridge;

fn snapshot<R: Default>(
    runtime: &mut Runtime,
    wifi: Signal<WifiState>,
    read: impl FnOnce(&WifiSnapshot) -> R,
) -> R {
    wifi.with(runtime, |wifi| wifi.snapshot.as_ref().map(read))
        .unwrap_or_default()
}

pub fn card(cx: &mut Cx, wifi: Signal<WifiState>) -> impl View + use<> {
    let radio = bridge(
        cx,
        move |runtime| snapshot(runtime, wifi, |snapshot| snapshot.radio_enabled),
        move |runtime, on| send(runtime, wifi, WifiCommand::SetRadio(on)),
    );
    let blocked =
        move |runtime: &mut Runtime| snapshot(runtime, wifi, |snapshot| snapshot.hardware_blocked);

    settings_card_titled(
        "Wi-Fi",
        (
            setting_row_icon(
                live_glyph(move |runtime| {
                    if snapshot(runtime, wifi, |snapshot| snapshot.radio_enabled) {
                        icons::WIFI
                    } else {
                        icons::WIFI_OFF
                    }
                }),
                "Wi-Fi",
                toggle(radio).disabled(bind(blocked)),
            ),
            when(blocked, |_| {
                vstack((
                    settings_divider(),
                    setting_row_content(
                        status("Turned off by a hardware switch").tone(Tone::Warning),
                    ),
                ))
            }),
        ),
    )
}
