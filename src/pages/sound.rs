//! The Sound page.

use blinc_icons::icons;
use crownconfig::schema::Sound;
use crownuikit::layouts::settings::{
    setting_row, setting_row_desc, setting_row_icon, settings_card_titled, settings_divider,
};

use crate::config::ConfigStore;
use crate::controls::{name_choice, percent_slider, switch};
use crate::pages::{PageDescriptor, PageView, page};

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

fn build(config: &ConfigStore) -> PageView {
    let sound: &Sound = config.section();
    let muted = sound.muted;

    page(
        PAGE.title,
        (
            settings_card_titled(
                "Output",
                (
                    setting_row_icon(
                        icons::VOLUME_2,
                        "Output volume",
                        percent_slider(sound.output_volume, |sound: &mut Sound, level| {
                            sound.output_volume = level;
                        })
                        .disabled(muted),
                    ),
                    settings_divider(),
                    setting_row_desc(
                        "Mute",
                        "Silence output without losing the volume level",
                        switch(muted, |sound: &mut Sound, on| sound.muted = on),
                    ),
                    settings_divider(),
                    setting_row(
                        "Output device",
                        name_choice(
                            OUTPUT_DEVICES,
                            sound.output_device.as_ref(),
                            |sound: &mut Sound, device| sound.output_device = Some(device),
                        ),
                    ),
                ),
            ),
            settings_card_titled(
                "Input",
                (setting_row_icon(
                    icons::MIC,
                    "Input volume",
                    percent_slider(sound.input_volume, |sound: &mut Sound, level| {
                        sound.input_volume = level;
                    }),
                ),),
            ),
        ),
    )
}
