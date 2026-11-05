//! Resetting and removing Android, each behind a confirmation dialog.

use crownui::kit::{ButtonVariant, dialog};
use crownui::prelude::{Cx, Listeners, Semantic, Signal, View, bind, spacer, vstack};

use super::parts::row_button;
use crate::layout::{
    headline, paragraph, setting_row_desc, settings_card_titled, settings_divider, spread,
};
use crate::net::android::{AndroidState, Removal};

/// Everything a removal's row and dialog say.
#[derive(Clone, Copy)]
struct Wording {
    title: &'static str,
    description: &'static str,
    working: &'static str,
    button: &'static str,
    question: &'static str,
    consequence: &'static str,
    confirm: &'static str,
}

const fn wording(removal: Removal) -> Wording {
    match removal {
        Removal::ResetData => Wording {
            title: "Reset Android data",
            description: "Deletes every Android app and its data, then sets Android up again",
            working: "Resetting…",
            button: "Reset…",
            question: "Reset Android data?",
            consequence: "Every Android app, its data and the Google account on it are \
                          deleted. Android then downloads again and starts fresh.",
            confirm: "Reset",
        },
        Removal::Uninstall => Wording {
            title: "Remove Android apps",
            description: "Deletes Android and its apps from this computer",
            working: "Removing…",
            button: "Remove…",
            question: "Remove Android apps?",
            consequence: "Android and every app are removed and their windows close. \
                          App data stays in your home folder.",
            confirm: "Remove",
        },
    }
}

pub fn card(cx: &mut Cx, android: Signal<AndroidState>) -> impl View + use<> {
    settings_card_titled(
        "Danger zone",
        (
            removal_row(cx, android, Removal::ResetData),
            settings_divider(),
            removal_row(cx, android, Removal::Uninstall),
        ),
    )
}

fn removal_row(cx: &mut Cx, android: Signal<AndroidState>, removal: Removal) -> impl View + use<> {
    let asking = cx.signal(false);
    let words = wording(removal);
    let ask = row_button(words.button, ButtonVariant::Destructive)
        .disabled(bind(move |runtime| {
            android.with(runtime, |state| state.removing.is_some())
        }))
        .on_click(move |cx, _| asking.set(cx, true));
    let row = setting_row_desc(
        words.title,
        bind(move |runtime| {
            let running = android.with(runtime, |state| state.removing == Some(removal));
            if running {
                words.working
            } else {
                words.description
            }
            .to_owned()
        }),
        ask,
    );
    let confirmation = dialog(asking, move || {
        (
            headline(words.question),
            paragraph(words.consequence),
            spread((
                spacer(),
                row_button("Cancel", ButtonVariant::Secondary)
                    .on_click(move |cx, _| asking.set(cx, false)),
                row_button(words.confirm, ButtonVariant::Destructive).on_click(move |cx, _| {
                    asking.set(cx, false);
                    android.update(cx, |state| state.remove(removal));
                }),
            )),
        )
    });
    vstack((row, confirmation))
}
