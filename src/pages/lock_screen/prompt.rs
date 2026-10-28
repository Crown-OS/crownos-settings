//! The password prompt: who it greets, where it sits, how long it stays.

use crownos_config::schema::Anchor;
use crownos_config::schema::lock::{AuthField, PromptTimeoutSecs, ShowUser};
use crownui::prelude::{Cx, View};

use crate::controls::{Options, choice, field_choice, switch};
use crate::layout::{setting_row, setting_row_desc, settings_card_titled, settings_divider};
use crate::state::Store;

static ANCHORS: Options<Anchor> = Options::new(&[
    ("Top left", Anchor::TopLeft),
    ("Top", Anchor::Top),
    ("Top right", Anchor::TopRight),
    ("Left", Anchor::Left),
    ("Center", Anchor::Center),
    ("Right", Anchor::Right),
    ("Bottom left", Anchor::BottomLeft),
    ("Bottom", Anchor::Bottom),
    ("Bottom right", Anchor::BottomRight),
]);

static TIMEOUTS: Options<u32> = Options::new(&[
    ("Never", 0),
    ("10 seconds", 10),
    ("20 seconds", 20),
    ("30 seconds", 30),
    ("1 minute", 60),
    ("2 minutes", 120),
]);

pub fn prompt_card(cx: &mut Cx, store: Store) -> impl View + use<> {
    settings_card_titled(
        "Password prompt",
        (
            setting_row_desc(
                "Show your name",
                "Your picture and name above the password field",
                switch(cx, store, ShowUser),
            ),
            settings_divider(),
            setting_row(
                "Position",
                field_choice(
                    cx,
                    &ANCHORS,
                    move |runtime| store.get(runtime, AuthField).anchor,
                    move |runtime, anchor| {
                        let mut placement = store.get(runtime, AuthField);
                        placement.anchor = anchor;
                        store.set(runtime, AuthField, placement);
                    },
                ),
            ),
            settings_divider(),
            setting_row_desc(
                "Hide after",
                "Without typing, the prompt gives way to the clock again",
                choice(cx, store, PromptTimeoutSecs, &TIMEOUTS),
            ),
        ),
    )
}
