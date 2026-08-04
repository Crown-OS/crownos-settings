//! The Bluetooth page.

use blinc_icons::icons;
use crownconfig::schema::Bluetooth;
use crownuikit::layouts::settings::{
    setting_row_desc, setting_row_icon, settings_card_titled, settings_divider,
};

use crate::config::ConfigStore;
use crate::controls::{switch, value_text};
use crate::pages::{PageDescriptor, PageView, page};

pub const PAGE: PageDescriptor = PageDescriptor {
    title: "Bluetooth",
    icon: icons::BLUETOOTH,
    build,
};

fn build(config: &ConfigStore) -> PageView {
    let bluetooth: &Bluetooth = config.section();
    let enabled = bluetooth.enabled;

    page(
        PAGE.title,
        (settings_card_titled(
            "Bluetooth",
            (
                setting_row_icon(
                    icons::BLUETOOTH,
                    "Bluetooth",
                    switch(enabled, |bluetooth: &mut Bluetooth, on| {
                        bluetooth.enabled = on;
                    }),
                ),
                settings_divider(),
                setting_row_desc(
                    "Discoverable",
                    "Nearby devices can find this computer while Bluetooth is on",
                    value_text(if enabled { "Yes" } else { "No" }),
                ),
            ),
        ),),
    )
}
