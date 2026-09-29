//! The pairing flow: a QR invitation, a code to compare, and the outcome.

use blinc_icons::icons;
use crownuikit::config::theme;
use crownuikit::util::INTER;
use crownuikit::widgets::{ButtonVariant, Tone, button, callout, spinner, status};
use xilem::masonry::properties::ObjectFit;
use xilem::masonry::properties::types::AsUnit;
use xilem::style::Style;
use xilem::view::{
    CrossAxisAlignment, FlexExt, FlexSpacer, flex_col, flex_row, image, label, sized_box,
};
use xilem::{FontWeight, WidgetView};

use super::describe::{class_glyph, countdown_text, grouped_code};
use super::parts::{INLINE_GAP, ROW_SPINNER, headline, inline, paragraph};
use crate::controls::value_text;
use crate::layout::{
    separated, setting_row_content, setting_row_icon, settings_card_titled, settings_divider,
};
use crate::net::crownconnect::{
    CrossDeviceState, PairingInvite, PairingOutcome, PairingPrompt, PairingSession,
};
use crate::pages::PageView;
use crate::state::Store;

const TITLE: &str = "Connect a Device";
/// The QR code's largest side; it is drawn at the biggest whole number of
/// pixels per module that fits, so no module is ever resampled unevenly.
const QR_SIDE: f64 = 196.0;
const QR_GAP: f64 = 24.0;
const CODE_SIZE: f32 = 34.0;

pub fn cards(state: &CrossDeviceState, session: &PairingSession) -> Vec<PageView> {
    if let Some(outcome) = session.outcome.as_ref() {
        return vec![outcome_view(outcome)];
    }
    if let Some(prompt) = session.prompt.as_ref() {
        return vec![compare_card(prompt).boxed()];
    }
    let Some(invite) = session.invite.as_ref() else {
        return vec![waiting_card().boxed()];
    };
    let mut cards = vec![invite_card(invite, state.now_unix_ms).boxed()];
    if state.nearby().next().is_some() {
        cards.push(nearby_card(state).boxed());
    }
    cards
}

fn waiting_card() -> impl WidgetView<Store> {
    settings_card_titled(
        TITLE,
        (setting_row_content(inline((
            spinner().size(ROW_SPINNER),
            value_text("Preparing a pairing code…"),
            FlexSpacer::Flex(1.0),
            cancel_button(),
        ))),),
    )
}

fn invite_card(invite: &PairingInvite, now_unix_ms: u64) -> impl WidgetView<Store> + use<> {
    let seconds_left = invite.seconds_left(now_unix_ms);
    let expired = seconds_left == 0;

    let (code, clock) = if expired {
        (None, status("This code has expired").tone(Tone::Warning))
    } else {
        (
            Some(qr_view(invite)),
            status(format!("Expires in {}", countdown_text(seconds_left))).tone(Tone::Neutral),
        )
    };

    let steps = flex_col((
        headline("Scan with your device"),
        paragraph("Open CrownConnect on your phone or tablet and scan this code."),
        clock,
    ))
    .gap(INLINE_GAP.px())
    .cross_axis_alignment(CrossAxisAlignment::Start);

    let renew = expired.then(|| {
        button("New Code", |store: &mut Store| {
            store.cross_device.begin_pairing();
        })
        .variant(ButtonVariant::Primary)
        .small()
    });

    settings_card_titled(
        TITLE,
        (
            setting_row_content(
                flex_row((code, steps.flex(1.0)))
                    .gap(QR_GAP.px())
                    .cross_axis_alignment(CrossAxisAlignment::Start),
            ),
            settings_divider(),
            setting_row_icon(
                icons::RADIO,
                "Or pair with a nearby device: choose this computer in CrownConnect",
                value_text(""),
            ),
            settings_divider(),
            setting_row_content(inline((FlexSpacer::Flex(1.0), cancel_button(), renew))),
        ),
    )
}

/// The invitation at a whole number of pixels per module, nearest-sampled.
fn qr_view(invite: &PairingInvite) -> impl WidgetView<Store> + use<> {
    let modules = f64::from(invite.qr.image.width.max(1));
    let side = (QR_SIDE / modules).floor().max(1.0) * modules;
    sized_box(image(invite.qr.clone()).fit(ObjectFit::Contain))
        .width(side.px())
        .height(side.px())
}

fn nearby_card(state: &CrossDeviceState) -> impl WidgetView<Store> + use<> {
    let rows = state.nearby().map(|device| {
        setting_row_icon(
            class_glyph(device.info.class),
            device.info.name.clone(),
            value_text("Waiting for it to ask"),
        )
        .boxed()
    });
    settings_card_titled("Nearby", separated(rows))
}

fn compare_card(prompt: &PairingPrompt) -> impl WidgetView<Store> + use<> {
    let code = label(grouped_code(&prompt.code).into_owned())
        .text_size(CODE_SIZE)
        .weight(FontWeight::BOLD)
        .font(INTER)
        .color(theme().text.primary);

    let body = flex_col((
        headline(format!("Pair with {}?", prompt.name)),
        paragraph("Check that the same code is showing on the device."),
        code,
    ))
    .gap(INLINE_GAP.px())
    .cross_axis_alignment(CrossAxisAlignment::Start);

    let answer: PageView = if prompt.answered {
        inline((
            spinner().size(ROW_SPINNER),
            value_text("Waiting for the device…"),
        ))
        .boxed()
    } else {
        inline((
            button("Decline", |store: &mut Store| {
                store.cross_device.answer_pairing(false);
            })
            .variant(ButtonVariant::Secondary)
            .small(),
            button("Pair", |store: &mut Store| {
                store.cross_device.answer_pairing(true);
            })
            .variant(ButtonVariant::Primary)
            .small(),
        ))
        .boxed()
    };

    settings_card_titled(
        TITLE,
        (
            setting_row_content(body),
            settings_divider(),
            setting_row_content(inline((FlexSpacer::Flex(1.0), answer))),
        ),
    )
}

fn outcome_view(outcome: &PairingOutcome) -> PageView {
    let (notice, retry) = match outcome {
        PairingOutcome::Paired { name } => (
            callout(Tone::Success, format!("Paired with {name}"))
                .body("It connects on its own whenever it is nearby.")
                .view(),
            false,
        ),
        PairingOutcome::Refused => (
            callout(Tone::Danger, "Pairing did not finish")
                .body("The code was declined, or the device stopped answering.")
                .view(),
            true,
        ),
    };
    let actions = inline((
        retry.then(|| {
            button("Try Again", |store: &mut Store| {
                store.cross_device.begin_pairing();
            })
            .variant(ButtonVariant::Secondary)
        }),
        button("Done", |store: &mut Store| store.cross_device.end_pairing())
            .variant(ButtonVariant::Primary),
    ));
    flex_col((notice, actions))
        .gap(INLINE_GAP.px())
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .boxed()
}

fn cancel_button() -> impl WidgetView<Store> {
    button("Cancel", |store: &mut Store| {
        store.cross_device.end_pairing()
    })
    .variant(ButtonVariant::Ghost)
    .small()
}
