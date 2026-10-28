//! Everything visible that this machine has never saved.
//!
//! The card is drawn even when it is empty, because "we looked and there is
//! nothing" is information — silently omitting it would read as a page that
//! hadn't finished loading.

use crownui::kit::{
    ButtonVariant, StatusOptions, TextInputOptions, Tone, spinner, status, text_input,
};
use crownui::prelude::{
    Cx, Listeners, Runtime, Semantic, Signal, Styled, View, bind, for_each, keyed, vstack,
};

use super::parts::{ROW_SPINNER, lock_icon, row_button, send, signal_icon};
use crate::layout::{
    Listed, hairline_unless_first, inline, listed, nothing, setting_row, setting_row_content,
    settings_card_titled, when,
};
use crate::net::wifi::{OtherNetwork, PSK_LENGTH, WifiCommand, WifiState};
use crate::util::bridge::derived;

/// Wide enough for a passphrase to be visible as a shape, narrow enough to
/// leave Cancel and Join on the same row.
const PASSWORD_WIDTH: f32 = 200.0;

type Row = Signal<Listed<OtherNetwork>>;

pub fn card(cx: &mut Cx, wifi: Signal<WifiState>) -> impl View + use<> {
    let others = derived(cx, move |runtime| {
        wifi.with(runtime, |wifi| {
            listed(
                wifi.snapshot
                    .iter()
                    .flat_map(|snapshot| snapshot.others.iter().cloned()),
            )
        })
    });
    settings_card_titled(
        "Other Networks",
        (
            when(
                move |runtime| others.with(runtime, Vec::is_empty),
                |_| setting_row_content(status("No other networks found").tone(Tone::Neutral)),
            ),
            for_each(
                others,
                |row: &Listed<OtherNetwork>| row.item.ssid.clone(),
                move |cx, row| other_row(cx, wifi, row),
            ),
        ),
    )
}

/// One unknown network, which may have grown a password field.
///
/// The prompt is part of the row rather than a card of its own so that the
/// field appears attached to the name it belongs to; with thirty access points
/// in range, a password box anywhere else is a password box for nobody in
/// particular.
fn other_row(cx: &mut Cx, wifi: Signal<WifiState>, row: Row) -> impl View + use<> {
    let ssid = row.with_untracked(cx, |row| row.item.ssid.clone());
    let (watched, prompted, named) = (ssid.clone(), ssid.clone(), format!("Join {ssid}"));
    let prompt_open = move |runtime: &mut Runtime| {
        wifi.with(runtime, |wifi| {
            wifi.prompt.as_deref() == Some(prompted.as_str())
        })
    };
    let prompt_closed = prompt_open.clone();

    let action = keyed(
        move |runtime| {
            let joining = wifi.with(runtime, |wifi| {
                wifi.connecting.as_deref() == Some(watched.as_str())
            });
            (joining, prompt_closed.clone()(runtime))
        },
        move |_, (joining, prompting)| match (joining, prompting) {
            (true, _) => spinner().size(ROW_SPINNER).boxed(),
            (false, false) => row_button("Join", ButtonVariant::Ghost)
                .label(named.clone())
                .on_click(move |cx, _| {
                    let (ssid, secured) =
                        row.with_untracked(cx, |row| (row.item.ssid.clone(), row.item.secured));
                    if secured {
                        // One step, not two: the click opens the field, and
                        // the field's own Join is what reaches the worker.
                        wifi.update(cx, |wifi| wifi.prompt = Some(ssid));
                    } else {
                        send(
                            cx,
                            wifi,
                            WifiCommand::Connect {
                                ssid,
                                password: None,
                            },
                        );
                    }
                })
                .boxed(),
            (false, true) => nothing().boxed(),
        },
    );

    let password_ssid = ssid.clone();
    vstack((
        hairline_unless_first(row),
        setting_row(
            ssid,
            inline((
                when(
                    move |runtime| row.with(runtime, |row| row.item.secured),
                    move |_| lock_icon(move |runtime| row.with(runtime, |row| row.item.weak)),
                ),
                signal_icon(move |runtime| row.with(runtime, |row| row.item.strength)),
                action,
            )),
        ),
        when(prompt_open, move |cx| {
            password_row(cx, wifi, password_ssid.clone())
        }),
    ))
}

/// The inline password field for one open prompt.
///
/// Validation is [`PSK_LENGTH`] and nothing else: NetworkManager reports a
/// wrong-length key as an addressing error, which is unreadable, so Join
/// simply does not arm until the length is one NetworkManager will accept —
/// in bytes, because bytes are what it counts. Enter does what Join does.
fn password_row(cx: &mut Cx, wifi: Signal<WifiState>, ssid: String) -> impl View + use<> {
    let password = cx.signal(String::new());
    let submitted = ssid.clone();

    let field = text_input(password)
        .secret()
        .placeholder("Password")
        .width(PASSWORD_WIDTH)
        .label(format!("Password for {ssid}"))
        .on_submit(move |cx, _| join(cx, wifi, &submitted, password));

    let cancel = row_button("Cancel", ButtonVariant::Ghost)
        .on_click(move |cx, _| wifi.update(cx, |wifi| wifi.prompt = None));

    let confirm = row_button("Join", ButtonVariant::Primary)
        .disabled(bind(move |runtime| {
            !PSK_LENGTH.contains(&password.with(runtime, String::len))
        }))
        .on_click(move |cx, _| join(cx, wifi, &ssid, password));

    setting_row("Password", inline((field, cancel, confirm)))
}

/// Submit a typed key, if it is one NetworkManager could accept.
///
/// The prompt closes here as well as in the worker's event folding: the round
/// trip is long enough to see, and a field that stays filled after Join looks
/// like a click that missed.
fn join(runtime: &mut Runtime, wifi: Signal<WifiState>, ssid: &str, password: Signal<String>) {
    let password = password.with(runtime, Clone::clone);
    if !PSK_LENGTH.contains(&password.len()) {
        return;
    }
    wifi.update(runtime, |wifi| wifi.prompt = None);
    send(
        runtime,
        wifi,
        WifiCommand::Connect {
            ssid: ssid.to_owned(),
            password: Some(password),
        },
    );
}
