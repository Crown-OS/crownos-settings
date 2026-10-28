//! The pairing flow: a QR invitation, a code to compare, and the outcome.

use crownui::kit::{
    ButtonOptions, ButtonVariant, CalloutOptions, INTER_FAMILY, StatusOptions, Tone, button,
    callout, icons, spinner, status, use_theme,
};
use crownui::prelude::{
    AnyView, BLACK, CornerRadii, Cx, Rect, Signal, Styled, View, WHITE, bind, canvas, component,
    for_each, hstack, keyed, spacer, text, vstack,
};

use super::describe::{class_glyph, countdown_text, grouped_code};
use super::parts::{ROW_SPINNER, acting, row_button};
use crate::layout::{
    INLINE_GAP, Listed, card_stack, glyph, hairline_unless_first, headline, inline, listed,
    nothing, paragraph, setting_row_content, setting_row_icon, settings_card_titled,
    settings_divider, spread, value_text, when,
};
use crate::net::crownconnect::{CrossDeviceState, Device, PairingOutcome, PairingSession};
use crate::util::bridge::derived;
use crate::util::qr::QrMatrix;
use crate::util::theme::tint;

const TITLE: &str = "Connect a Device";
/// The QR code's largest side; it is drawn at the biggest whole number of
/// pixels per module that fits, so no module is ever resampled unevenly.
const QR_SIDE: f32 = 196.0;
const QR_GAP: f32 = 24.0;
const CODE_SIZE: f32 = 34.0;

/// Where the pairing flow is, as far as the cards' structure goes.
#[derive(Debug, Clone, PartialEq)]
pub enum Stage {
    Outcome(PairingOutcome),
    Compare { name: String, code: String },
    Waiting,
    Invite { qr: QrMatrix, expired: bool },
}

impl Stage {
    pub fn of(session: &PairingSession, now_unix_ms: u64) -> Self {
        if let Some(outcome) = session.outcome.as_ref() {
            return Self::Outcome(outcome.clone());
        }
        if let Some(prompt) = session.prompt.as_ref() {
            return Self::Compare {
                name: prompt.name.clone(),
                code: prompt.code.clone(),
            };
        }
        match session.invite.as_ref() {
            None => Self::Waiting,
            Some(invite) => Self::Invite {
                qr: invite.qr.clone(),
                expired: invite.seconds_left(now_unix_ms) == 0,
            },
        }
    }
}

pub fn cards(state: Signal<CrossDeviceState>, stage: Stage) -> AnyView {
    match stage {
        Stage::Outcome(outcome) => outcome_view(state, outcome).boxed(),
        Stage::Compare { name, code } => compare_card(state, name, code).boxed(),
        Stage::Waiting => waiting_card(state).boxed(),
        Stage::Invite { qr, expired } => card_stack((
            invite_card(state, qr, expired),
            when(
                move |runtime| state.with(runtime, |state| state.nearby().next().is_some()),
                move |cx| nearby_card(cx, state),
            ),
        ))
        .boxed(),
    }
}

fn cancel_button(state: Signal<CrossDeviceState>) -> impl View {
    acting(
        row_button("Cancel", ButtonVariant::Ghost),
        state,
        CrossDeviceState::end_pairing,
    )
}

fn waiting_card(state: Signal<CrossDeviceState>) -> impl View {
    settings_card_titled(
        TITLE,
        (setting_row_content(spread((
            spinner().size(ROW_SPINNER),
            value_text("Preparing a pairing code…"),
            spacer(),
            cancel_button(state),
        ))),),
    )
}

fn invite_card(state: Signal<CrossDeviceState>, qr: QrMatrix, expired: bool) -> impl View {
    let clock = if expired {
        status("This code has expired").tone(Tone::Warning)
    } else {
        status(bind(move |runtime| {
            let seconds = state.with(runtime, |state| {
                state
                    .pairing
                    .as_ref()
                    .and_then(|session| session.invite.as_ref())
                    .map_or(0, |invite| invite.seconds_left(state.now_unix_ms))
            });
            format!("Expires in {}", countdown_text(seconds))
        }))
        .tone(Tone::Neutral)
    };

    let steps = vstack((
        headline("Scan with your device"),
        paragraph("Open CrownConnect on your phone or tablet and scan this code."),
        clock,
    ))
    .gap(INLINE_GAP)
    .items_start()
    .flex_1();

    let code = if expired {
        nothing().boxed()
    } else {
        qr_view(qr).boxed()
    };
    let renew = if expired {
        acting(
            row_button("New Code", ButtonVariant::Primary),
            state,
            CrossDeviceState::begin_pairing,
        )
        .boxed()
    } else {
        nothing().boxed()
    };

    settings_card_titled(
        TITLE,
        (
            setting_row_content(
                hstack((code, steps))
                    .gap(QR_GAP)
                    .items_start()
                    .self_stretch(),
            ),
            settings_divider(),
            setting_row_icon(
                glyph(icons::RADIO),
                "Or pair with a nearby device: choose this computer in CrownConnect",
                value_text(""),
            ),
            settings_divider(),
            setting_row_content(spread((spacer(), cancel_button(state), renew))),
        ),
    )
}

/// The invitation at a whole number of pixels per module, painted as one
/// rectangle per run of dark modules — crisp at any scale, and no texture.
fn qr_view(qr: QrMatrix) -> impl View {
    let modules = qr.side().max(1) as f32;
    let module = (QR_SIDE / modules).floor().max(1.0);
    let side = module * modules;
    canvas(move |_, _, list| {
        list.fill(
            Rect::from_xywh(0.0, 0.0, side, side),
            CornerRadii::ZERO,
            WHITE,
        );
        for run in qr.dark_runs() {
            list.fill(
                Rect::from_xywh(
                    run.start as f32 * module,
                    run.row as f32 * module,
                    run.length as f32 * module,
                    module,
                ),
                CornerRadii::ZERO,
                BLACK,
            );
        }
    })
    .size(side)
    .shrink(0.0)
}

fn nearby_card(cx: &mut Cx, state: Signal<CrossDeviceState>) -> impl View + use<> {
    let nearby = derived(cx, move |runtime| {
        state.with(runtime, |state| listed(state.nearby().cloned()))
    });
    settings_card_titled(
        "Nearby",
        (for_each(
            nearby,
            |row: &Listed<Device>| row.item.info.id,
            |cx, row| {
                let name = row.with_untracked(cx, |row| row.item.info.name.clone());
                vstack((
                    hairline_unless_first(row),
                    setting_row_icon(
                        keyed(
                            move |runtime| {
                                row.with(runtime, |row| class_glyph(row.item.info.class))
                            },
                            |_, body| glyph(body),
                        ),
                        name,
                        value_text("Waiting for it to ask"),
                    ),
                ))
            },
        ),),
    )
}

fn compare_card(state: Signal<CrossDeviceState>, name: String, code: String) -> impl View {
    let answered = move |runtime: &mut _| {
        state.with(runtime, |state| {
            state
                .pairing
                .as_ref()
                .and_then(|session| session.prompt.as_ref())
                .is_some_and(|prompt| prompt.answered)
        })
    };

    let body = vstack((
        headline(format!("Pair with {name}?")),
        paragraph("Check that the same code is showing on the device."),
        code_label(grouped_code(&code).into_owned()),
    ))
    .gap(INLINE_GAP)
    .items_start();

    let answer = keyed(answered, move |_, answered| {
        if answered {
            inline((
                spinner().size(ROW_SPINNER),
                value_text("Waiting for the device…"),
            ))
            .boxed()
        } else {
            inline((
                acting(
                    row_button("Decline", ButtonVariant::Secondary),
                    state,
                    |state| {
                        state.answer_pairing(false);
                    },
                ),
                acting(row_button("Pair", ButtonVariant::Primary), state, |state| {
                    state.answer_pairing(true);
                }),
            ))
            .boxed()
        }
    });

    settings_card_titled(
        TITLE,
        (
            setting_row_content(body),
            settings_divider(),
            setting_row_content(spread((spacer(), answer))),
        ),
    )
}

/// The six digits both screens show, large enough to compare at a glance.
fn code_label(code: String) -> impl View {
    component(move |cx| {
        let theme = use_theme(cx);
        text(code)
            .text_size(CODE_SIZE)
            .font_bold()
            .font_family(INTER_FAMILY)
            .text_color(tint(theme, |theme| theme.text.primary))
    })
}

fn outcome_view(state: Signal<CrossDeviceState>, outcome: PairingOutcome) -> impl View {
    let (notice, retry) = match outcome {
        PairingOutcome::Paired { name } => (
            callout(Tone::Success, format!("Paired with {name}"))
                .body("It connects on its own whenever it is nearby."),
            false,
        ),
        PairingOutcome::Refused => (
            callout(Tone::Danger, "Pairing did not finish")
                .body("The code was declined, or the device stopped answering."),
            true,
        ),
    };
    let try_again = if retry {
        acting(
            button("Try Again").variant(ButtonVariant::Secondary),
            state,
            CrossDeviceState::begin_pairing,
        )
        .boxed()
    } else {
        nothing().boxed()
    };
    let done = acting(
        button("Done").variant(ButtonVariant::Primary),
        state,
        CrossDeviceState::end_pairing,
    );
    vstack((notice, inline((try_again, done))))
        .gap(INLINE_GAP)
        .items_start()
}
