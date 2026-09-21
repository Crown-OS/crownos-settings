//! The Notifications page.

use crate::layout::{setting_row_desc, setting_row_icon, settings_card_titled, settings_divider};
use blinc_icons::icons;
use crownos_config::schema::notification;

use crate::controls::switch;
use crate::pages::{PageDescriptor, PageView, page};
use crate::state::Store;

pub const PAGE: PageDescriptor = PageDescriptor {
    title: "Notifications",
    icon: icons::BELL,
    build,
};

fn build(store: &Store) -> PageView {
    // The master switch gates the other two rows.
    let enabled = store.get(notification::Enabled);

    page(
        &PAGE,
        (settings_card_titled(
            "Notifications",
            (
                setting_row_icon(
                    icons::BELL,
                    "Allow notifications",
                    switch(store, notification::Enabled),
                ),
                settings_divider(),
                setting_row_desc(
                    "Do Not Disturb",
                    "Keep notifications in the tray without banners or sounds",
                    switch(store, notification::DoNotDisturb).disabled(!enabled),
                ),
                settings_divider(),
                setting_row_desc(
                    "Show previews",
                    "Include message content in banners",
                    switch(store, notification::ShowPreviews).disabled(!enabled),
                ),
            ),
        ),),
    )
}
