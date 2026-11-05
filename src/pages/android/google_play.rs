//! Google Play's one-time step: Android images built outside Google's
//! certification only sign in once their services id is registered by hand.

use crownui::kit::{ButtonOptions, ButtonVariant, icons};
use crownui::prelude::{Listeners, Runtime, Semantic, Signal, View, bind, spacer};

use super::parts::row_button;
use crate::layout::{
    glyph, setting_row_content, setting_row_icon_desc, settings_card_titled, settings_divider,
    spread, value_text,
};
use crate::net::android::AndroidState;
use crate::util::open_url::open_url;

pub const REGISTRATION_URL: &str = "https://www.google.com/android/uncertified";
pub const PLAIN_TEXT: &str = "text/plain;charset=utf-8";
const NOT_YET: &str = "Shown once Android has started";

fn registration_id(runtime: &mut Runtime, android: Signal<AndroidState>) -> Option<String> {
    android.with(runtime, |state| state.registration_id.clone())
}

pub fn card(android: Signal<AndroidState>) -> impl View {
    let copy = row_button("Copy ID", ButtonVariant::Secondary)
        .leading_icon(icons::COPY)
        .disabled(bind(move |runtime| {
            registration_id(runtime, android).is_none()
        }))
        .on_click(move |cx, _| {
            let id = android.with_untracked(cx, |state| state.registration_id.clone());
            if let Some(id) = id {
                cx.set_clipboard(vec![(PLAIN_TEXT.to_owned(), id.into_bytes())]);
            }
        });
    let open = row_button("Open registration page", ButtonVariant::Secondary)
        .leading_icon(icons::EXTERNAL_LINK)
        .on_click(|_, _| open_url(REGISTRATION_URL));
    settings_card_titled(
        "Google Play",
        (
            setting_row_icon_desc(
                glyph(icons::SHIELD_CHECK),
                "Register this device",
                "Google Play signs in once this ID is registered with your Google account",
                value_text(bind(move |runtime| {
                    registration_id(runtime, android).unwrap_or_else(|| NOT_YET.to_owned())
                })),
            ),
            settings_divider(),
            setting_row_content(spread((spacer(), copy, open))),
        ),
    )
}
