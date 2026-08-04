//! The Wi-Fi page.

use blinc_icons::icons;
use crownconfig::schema::Wifi;
use crownuikit::layouts::settings::{
    setting_row, setting_row_desc, setting_row_icon, settings_card_titled, settings_divider,
};

use crate::config::ConfigStore;
use crate::controls::{name_choice, switch, value_text};
use crate::pages::{PageDescriptor, PageView, page};

/// Placeholder SSIDs until there's a real network backend to enumerate.
static NETWORKS: &[&str] = &["CrownNet", "CrownNet 5G", "Workshop", "Phone Hotspot"];

pub const PAGE: PageDescriptor = PageDescriptor {
    title: "Wi-Fi",
    icon: icons::WIFI,
    build,
};

fn build(config: &ConfigStore) -> PageView {
    let wifi: &Wifi = config.section();
    let enabled = wifi.enabled;

    page(
        PAGE.title,
        (
            settings_card_titled(
                "Network",
                (
                    setting_row_icon(
                        icons::WIFI,
                        "Wi-Fi",
                        switch(enabled, |wifi: &mut Wifi, on| wifi.enabled = on),
                    ),
                    settings_divider(),
                    setting_row_desc(
                        "Network",
                        "Known networks in range",
                        name_choice(NETWORKS, wifi.network.as_ref(), |wifi: &mut Wifi, ssid| {
                            wifi.network = Some(ssid);
                        })
                        .disabled(!enabled),
                    ),
                ),
            ),
            settings_card_titled(
                "Status",
                (setting_row("Connection", value_text(status(wifi))),),
            ),
        ),
    )
}

/// The connection line: the radio being off outranks whatever SSID is stored,
/// since that network isn't joined while the radio is down.
fn status(wifi: &Wifi) -> String {
    match (&wifi.network, wifi.enabled) {
        (_, false) => "Off".to_owned(),
        (Some(ssid), true) => format!("Connected to {ssid}"),
        (None, true) => "Not connected".to_owned(),
    }
}
