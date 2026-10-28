//! The Sound page.

use crownos_config::schema::sound;
use crownui::kit::icons;
use crownui::prelude::{AnyView, Cx, Semantic, bind};

use crate::controls::{name_choice, percent_slider, switch};
use crate::layout::{
    glyph, setting_row, setting_row_desc, setting_row_icon, settings_card_titled, settings_divider,
};
use crate::pages::{PageDescriptor, page};
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
    slug: "sound",
    icon: icons::VOLUME_2,
    build,
};

fn build(cx: &mut Cx, store: Store) -> AnyView {
    // Muting disables the output slider without touching the stored level.
    let muted = bind(move |runtime| store.get(runtime, sound::Muted));

    page(
        &PAGE,
        (
            settings_card_titled(
                "Output",
                (
                    setting_row_icon(
                        glyph(icons::VOLUME_2),
                        "Output volume",
                        percent_slider(cx, store, sound::OutputVolume).disabled(muted),
                    ),
                    settings_divider(),
                    setting_row_desc(
                        "Mute",
                        "Silence output without losing the volume level",
                        switch(cx, store, sound::Muted),
                    ),
                    settings_divider(),
                    setting_row(
                        "Output device",
                        name_choice(cx, store, sound::OutputDevice, OUTPUT_DEVICES),
                    ),
                ),
            ),
            settings_card_titled(
                "Input",
                (setting_row_icon(
                    glyph(icons::MIC),
                    "Input volume",
                    percent_slider(cx, store, sound::InputVolume),
                ),),
            ),
        ),
    )
}
