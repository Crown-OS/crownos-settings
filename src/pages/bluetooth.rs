//! The Bluetooth page.

use crate::layout::{setting_row_desc, setting_row_icon, settings_card_titled, settings_divider};
use blinc_icons::icons;
use crownos_config::schema::bluetooth;

use crate::controls::{switch, value_text};
use crate::pages::{PageDescriptor, PageView, page};
use crate::state::Store;

pub const PAGE: PageDescriptor = PageDescriptor {
    title: "Bluetooth",
    icon: icons::BLUETOOTH,
    build,
};

fn build(store: &Store) -> PageView {
    // Reported by the Discoverable row below, not only by its own switch.
    let enabled = store.get(bluetooth::Enabled);

    page(
        &PAGE,
        (settings_card_titled(
            "Bluetooth",
            (
                setting_row_icon(
                    icons::BLUETOOTH,
                    "Bluetooth",
                    switch(store, bluetooth::Enabled),
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
