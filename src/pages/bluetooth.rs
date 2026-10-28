//! The Bluetooth page.

use crownos_config::schema::bluetooth;
use crownui::kit::icons;
use crownui::prelude::{AnyView, Cx, bind};

use crate::controls::switch;
use crate::layout::{
    glyph, setting_row_desc, setting_row_icon, settings_card_titled, settings_divider, value_text,
};
use crate::pages::{PageDescriptor, page};
use crate::state::Store;

pub const PAGE: PageDescriptor = PageDescriptor {
    title: "Bluetooth",
    slug: "bluetooth",
    icon: icons::BLUETOOTH,
    build,
};

fn build(cx: &mut Cx, store: Store) -> AnyView {
    // Reported by the Discoverable row below, not only by its own switch.
    let discoverable = bind(move |runtime| {
        let enabled = store.get(runtime, bluetooth::Enabled);
        if enabled { "Yes" } else { "No" }.to_owned()
    });

    page(
        &PAGE,
        (settings_card_titled(
            "Bluetooth",
            (
                setting_row_icon(
                    glyph(icons::BLUETOOTH),
                    "Bluetooth",
                    switch(cx, store, bluetooth::Enabled),
                ),
                settings_divider(),
                setting_row_desc(
                    "Discoverable",
                    "Nearby devices can find this computer while Bluetooth is on",
                    value_text(discoverable),
                ),
            ),
        ),),
    )
}
