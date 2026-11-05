//! How Android draws, which CrownOS layer it runs, and the space it takes.

use crownui::kit::icons;
use crownui::prelude::{Runtime, Signal, View, bind};

use crate::layout::{
    glyph, setting_row_icon, setting_row_icon_desc, settings_card_titled, settings_divider,
    value_text,
};
use crate::net::android::{AndroidConfig, AndroidState, AndroidTier, Storage};
use crate::util::byte_size::byte_size;

/// The tier's name, and what it means for how Android apps run.
fn tier_label(tier: Option<AndroidTier>) -> (&'static str, &'static str) {
    match tier {
        Some(AndroidTier::Native) => ("Native", "Drawn by this computer's graphics driver"),
        Some(AndroidTier::Venus) => (
            "Venus",
            "Drawn on the NVIDIA graphics card through a render server",
        ),
        Some(AndroidTier::Software) => (
            "Software",
            "Drawn by the processor, so games and video run slower",
        ),
        None => (
            "Not chosen yet",
            "Setup picks the fastest way this computer can draw Android",
        ),
    }
}

fn installed<R>(
    runtime: &mut Runtime,
    android: Signal<AndroidState>,
    read: impl FnOnce(Option<&AndroidConfig>) -> R,
) -> R {
    android.with(runtime, |state| read(state.installed.as_ref()))
}

fn tier(config: Option<&AndroidConfig>) -> Option<AndroidTier> {
    config.and_then(|config| config.tier)
}

pub fn card(android: Signal<AndroidState>) -> impl View {
    settings_card_titled(
        "About",
        (
            setting_row_icon_desc(
                glyph(icons::CPU),
                "Graphics",
                bind(move |runtime| {
                    installed(runtime, android, |config| {
                        tier_label(tier(config)).1.to_owned()
                    })
                }),
                value_text(bind(move |runtime| {
                    installed(runtime, android, |config| {
                        tier_label(tier(config)).0.to_owned()
                    })
                })),
            ),
            settings_divider(),
            setting_row_icon(
                glyph(icons::LAYERS),
                "CrownOS layer",
                value_text(bind(move |runtime| {
                    installed(runtime, android, |config| {
                        config
                            .and_then(|config| config.bundle.clone())
                            .unwrap_or_else(|| "Not installed yet".to_owned())
                    })
                })),
            ),
        ),
    )
}

fn storage(runtime: &mut Runtime, android: Signal<AndroidState>) -> Storage {
    android.with(runtime, |state| state.storage)
}

fn size_or(bytes: Option<u64>, absent: &str) -> String {
    bytes.map_or_else(|| absent.to_owned(), byte_size)
}

pub fn storage_card(android: Signal<AndroidState>) -> impl View {
    settings_card_titled(
        "Storage",
        (
            setting_row_icon(
                glyph(icons::HARD_DRIVE),
                "Android system",
                value_text(bind(move |runtime| {
                    size_or(storage(runtime, android).images_bytes, "Not downloaded yet")
                })),
            ),
            settings_divider(),
            setting_row_icon_desc(
                glyph(icons::DATABASE),
                "Apps and data",
                "Some app data is readable only by Android, so this can be more",
                value_text(bind(move |runtime| {
                    size_or(storage(runtime, android).data_bytes, "None yet")
                })),
            ),
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_tier_explains_itself_and_an_unprobed_one_says_so() {
        assert_eq!(tier_label(Some(AndroidTier::Native)).0, "Native");
        assert_eq!(tier_label(Some(AndroidTier::Venus)).0, "Venus");
        assert_eq!(tier_label(Some(AndroidTier::Software)).0, "Software");
        assert_eq!(tier_label(None).0, "Not chosen yet");
        let explanations = [
            AndroidTier::Native,
            AndroidTier::Venus,
            AndroidTier::Software,
        ]
        .map(|tier| tier_label(Some(tier)).1);
        assert!(explanations.iter().all(|line| !line.is_empty()));
    }

    #[test]
    fn a_size_nobody_could_read_says_why() {
        assert_eq!(size_or(Some(1_400_000_000), "None yet"), "1.4 GB");
        assert_eq!(size_or(None, "None yet"), "None yet");
    }
}
