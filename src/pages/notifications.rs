//! The Notifications page.

use blinc_icons::icons;
use crownconfig::schema::Notifications;
use crownuikit::layouts::settings::{
    setting_row_desc, setting_row_icon, settings_card_titled, settings_divider,
};

use crate::config::ConfigStore;
use crate::controls::switch;
use crate::pages::{PageDescriptor, PageView, page};

pub const PAGE: PageDescriptor = PageDescriptor {
    title: "Notifications",
    icon: icons::BELL,
    build,
};

fn build(config: &ConfigStore) -> PageView {
    let notifications: &Notifications = config.section();
    // The master switch gates the other two rows.
    let enabled = notifications.enabled;

    page(
        PAGE.title,
        (settings_card_titled(
            "Notifications",
            (
                setting_row_icon(
                    icons::BELL,
                    "Allow notifications",
                    switch(enabled, |notifications: &mut Notifications, on| {
                        notifications.enabled = on;
                    }),
                ),
                settings_divider(),
                setting_row_desc(
                    "Do Not Disturb",
                    "Keep notifications in the tray without banners or sounds",
                    switch(
                        notifications.do_not_disturb,
                        |notifications: &mut Notifications, on| {
                            notifications.do_not_disturb = on;
                        },
                    )
                    .disabled(!enabled),
                ),
                settings_divider(),
                setting_row_desc(
                    "Show previews",
                    "Include message content in banners",
                    switch(
                        notifications.show_previews,
                        |notifications: &mut Notifications, on| {
                            notifications.show_previews = on;
                        },
                    )
                    .disabled(!enabled),
                ),
            ),
        ),),
    )
}
