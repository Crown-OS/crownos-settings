//! How long Android stays up without a window, and which folders it sees.
//! All three write `android.ron`, which crown-androidd follows live.

use crownos_config::schema::android;
use crownui::kit::icons;
use crownui::prelude::{Cx, Semantic, View};

use crate::controls::{Options, choice, switch};
use crate::layout::{glyph, setting_row_icon_desc, settings_card_titled, settings_divider};
use crate::state::Store;

/// `idle_minutes` that keeps Android running until the session ends.
const NEVER: u32 = 0;

static IDLE_STOPS: Options<u32> = Options::new(&[
    ("After 1 minute", 1),
    ("After 5 minutes", 5),
    ("After 15 minutes", 15),
    ("After 30 minutes", 30),
    ("Never", NEVER),
]);

const SHARE_DOWNLOADS: &str = "Share Downloads";
const SHARE_PICTURES: &str = "Share Pictures";

pub fn card(cx: &mut Cx, store: Store) -> impl View + use<> {
    settings_card_titled(
        "While Android runs",
        (
            setting_row_icon_desc(
                glyph(icons::TIMER),
                "Stop when unused",
                "Android apps can't ring alarms or notify you while Android is stopped",
                choice(cx, store, android::IdleMinutes, &IDLE_STOPS),
            ),
            settings_divider(),
            setting_row_icon_desc(
                glyph(icons::DOWNLOAD),
                SHARE_DOWNLOADS,
                "Android apps can open and save files in Downloads",
                switch(cx, store, android::ShareDownloads).label(SHARE_DOWNLOADS),
            ),
            settings_divider(),
            setting_row_icon_desc(
                glyph(icons::IMAGE),
                SHARE_PICTURES,
                "Android apps can see the photos in Pictures",
                switch(cx, store, android::SharePictures).label(SHARE_PICTURES),
            ),
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crownos_config::schema::Android;

    #[test]
    fn the_idle_choices_end_in_never_and_offer_the_default() {
        let minutes: Vec<u32> = IDLE_STOPS.values().collect();
        assert_eq!(minutes, [1, 5, 15, 30, NEVER]);
        assert!(minutes.contains(&Android::default().idle_minutes));
    }
}
