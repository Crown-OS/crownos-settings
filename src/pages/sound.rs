//! The Sound page.

use crate::layout::{
    setting_row, setting_row_desc, setting_row_icon, settings_card_titled, settings_divider,
};
use blinc_icons::icons;
use crownos_config::schema::sound;

use crate::controls::{name_choice, percent_slider, switch};
use crate::pages::{PageDescriptor, PageView, page};
use crate::state::Store;

/// Placeholder sinks until there's a real audio backend to enumerate.
static OUTPUT_DEVICES: &[&str] = &[
    "Built-in Speakers",
    "HDMI Output",
    "USB Headset",
    "Bluetooth Headphones",
];

pub const PAGE: PageDescriptor = PageDescriptor {
    title: "Sound",
    icon: icons::VOLUME_2,
    build,
};

fn build(store: &Store) -> PageView {
    // Muting greys out the output slider without touching the stored level.
    let muted = store.get(sound::Muted);

    page(
        &PAGE,
        (
            settings_card_titled(
                "Output",
                (
                    setting_row_icon(
                        icons::VOLUME_2,
                        "Output volume",
                        percent_slider(store, sound::OutputVolume).disabled(muted),
                    ),
                    settings_divider(),
                    setting_row_desc(
                        "Mute",
                        "Silence output without losing the volume level",
                        switch(store, sound::Muted),
                    ),
                    settings_divider(),
                    setting_row(
                        "Output device",
                        name_choice(store, sound::OutputDevice, OUTPUT_DEVICES),
                    ),
                ),
            ),
            settings_card_titled(
                "Input",
                (setting_row_icon(
                    icons::MIC,
                    "Input volume",
                    percent_slider(store, sound::InputVolume),
                ),),
            ),
        ),
    )
}
