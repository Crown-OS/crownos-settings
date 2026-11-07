//! One provider's card: who is connected, the OAuth client, and the actions.

use crownos_accounts::{Provider, REDIRECT_URI};
use crownui::kit::{
    ButtonKind, ButtonOptions, ButtonSize, ButtonVariant, CalloutOptions, TextInputOptions, Tone,
    button, callout, icons, spinner,
};
use crownui::prelude::{
    Cx, Element, IntoProp, Listeners, Runtime, Semantic, Signal, Styled, View, bind, keyed, vstack,
};

use super::sign_in::sign_in;
use super::state::{AccountCard, describe};
use crate::controls::field_text;
use crate::layout::{
    glyph, inline, nothing, optional, setting_row_content, setting_row_desc, setting_row_icon_desc,
    settings_card_titled, settings_divider, value_text, when,
};
use crate::util::browser::open_in_browser;

const FIELD_WIDTH: f32 = 300.0;
const ROW_SPINNER: f32 = 14.0;

fn row_button(label: impl IntoProp<String>, variant: ButtonVariant) -> Element<ButtonKind> {
    button(label)
        .variant(variant)
        .control_size(ButtonSize::Small)
}

pub fn account_card(cx: &mut Cx, provider: Provider) -> impl View + use<> {
    let card = cx.signal(AccountCard::load(provider));
    let signed_in =
        move |runtime: &mut Runtime| card.with(runtime, |card| card.account.is_signed_in());
    settings_card_titled(
        provider.title(),
        (
            setting_row_icon_desc(
                glyph(icons::CIRCLE_USER),
                "Account",
                bind(move |runtime| card.with(runtime, AccountCard::status_line)),
                keyed(signed_in, move |_, signed_in| {
                    if signed_in {
                        sign_out_button(card).boxed()
                    } else {
                        sign_in_button(card).boxed()
                    }
                }),
            ),
            when(
                move |runtime| !signed_in(runtime),
                move |cx| client_rows(cx, card),
            ),
            optional(
                move |runtime| card.with(runtime, AccountCard::error),
                |_, message| {
                    vstack((
                        settings_divider(),
                        setting_row_content(
                            callout(Tone::Danger, "Could not connect")
                                .body(message)
                                .icon(icons::CIRCLE_ALERT),
                        ),
                    ))
                },
            ),
        ),
    )
}

fn sign_in_button(card: Signal<AccountCard>) -> impl View {
    let waiting = move |runtime: &mut Runtime| card.with(runtime, AccountCard::is_waiting);
    inline((
        when(waiting, |_| spinner().size(ROW_SPINNER)),
        row_button("Sign In", ButtonVariant::Primary)
            .leading_icon(icons::LOG_IN)
            .disabled(bind(move |runtime| {
                !card.with(runtime, AccountCard::can_sign_in)
            }))
            .on_click(move |cx, _| sign_in(cx, card)),
    ))
}

fn sign_out_button(card: Signal<AccountCard>) -> impl View {
    row_button("Sign Out", ButtonVariant::Secondary)
        .leading_icon(icons::LOG_OUT)
        .on_click(move |cx, _| card.update(cx, AccountCard::sign_out))
}

/// The OAuth client the user registered with the provider, and where to
/// register it.
fn client_rows(cx: &mut Cx, card: Signal<AccountCard>) -> impl View + use<> {
    let provider = card.with_untracked(cx, |card| card.provider);
    let client_id = field_text(
        cx,
        move |runtime| card.with(runtime, |card| card.account.client.id.clone()),
        move |runtime, typed| {
            card.update(runtime, |card| {
                card.edit_client(|client| typed.trim().clone_into(&mut client.id));
            });
        },
    )
    .placeholder("Client ID")
    .width(FIELD_WIDTH)
    .label("Client ID");
    let secret_row = provider.needs_secret().then(|| {
        let client_secret = field_text(
            cx,
            move |runtime| card.with(runtime, |card| card.account.client.secret.clone()),
            move |runtime, typed| {
                card.update(runtime, |card| {
                    card.edit_client(|client| typed.trim().clone_into(&mut client.secret));
                });
            },
        )
        .placeholder("Client secret")
        .width(FIELD_WIDTH)
        .secret()
        .label("Client secret");
        vstack((
            settings_divider(),
            setting_row_desc("Client secret", "Shown with the client ID", client_secret),
        ))
    });
    vstack((
        settings_divider(),
        setting_row_desc(
            "Developer app",
            provider.console_url(),
            row_button("Open", ButtonVariant::Ghost)
                .leading_icon(icons::EXTERNAL_LINK)
                .on_click(move |cx, _| {
                    if let Err(error) = open_in_browser(provider.console_url()) {
                        card.update(cx, |card| card.fail(describe(error)));
                    }
                }),
        ),
        settings_divider(),
        setting_row_desc("Client ID", "From the app you registered there", client_id),
        secret_row.map_or_else(|| nothing().boxed(), View::boxed),
        settings_divider(),
        setting_row_desc(
            "Redirect URI",
            "Add this to the app's allowed redirect URIs",
            value_text(REDIRECT_URI),
        ),
    ))
}
