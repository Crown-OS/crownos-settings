//! The Display page.

use blinc_icons::icons;
use crownconfig::schema::{Display, DisplayScale};
use crownuikit::layouts::settings::{
    setting_row, setting_row_desc, setting_row_icon, settings_card_titled, settings_divider,
};

use crate::config::ConfigStore;
use crate::controls::{Options, choice, percent_slider, switch};
use crate::pages::{PageDescriptor, PageView, page};

static SCALES: Options<DisplayScale> = Options::new(&[
    ("100%", DisplayScale::S100),
    ("125%", DisplayScale::S125),
    ("150%", DisplayScale::S150),
    ("200%", DisplayScale::S200),
]);

pub const PAGE: PageDescriptor = PageDescriptor {
    title: "Display",
    icon: icons::MONITOR,
    build,
};

fn build(config: &ConfigStore) -> PageView {
    let display: &Display = config.section();
    let night_light = display.night_light;

    page(
        PAGE.title,
        (
            settings_card_titled(
                "Brightness",
                (setting_row_icon(
                    icons::SUN,
                    "Brightness",
                    percent_slider(display.brightness, |display: &mut Display, level| {
                        display.brightness = level;
                    }),
                ),),
            ),
            settings_card_titled(
                "Night Light",
                (
                    setting_row_desc(
                        "Night Light",
                        "Shift colors warmer after sunset",
                        switch(night_light, |display: &mut Display, on| {
                            display.night_light = on;
                        }),
                    ),
                    settings_divider(),
                    setting_row(
                        "Warmth",
                        percent_slider(
                            display.night_light_warmth,
                            |display: &mut Display, warmth| {
                                display.night_light_warmth = warmth;
                            },
                        )
                        .disabled(!night_light),
                    ),
                ),
            ),
            settings_card_titled(
                "Scaling",
                (setting_row_desc(
                    "UI scale",
                    "Size of text and interface elements",
                    choice(&SCALES, display.scale, |display: &mut Display, scale| {
                        display.scale = scale;
                    }),
                ),),
            ),
        ),
    )
}
