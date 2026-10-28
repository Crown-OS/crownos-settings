//! The Notifications page.

use crownos_config::schema::notification;
use crownui::kit::icons;
use crownui::prelude::{AnyView, Cx, Semantic, bind};

use crate::controls::switch;
use crate::layout::{
    glyph, setting_row_desc, setting_row_icon, settings_card_titled, settings_divider,
};
use crate::pages::{PageDescriptor, page};
use crate::state::Store;

pub const PAGE: PageDescriptor = PageDescriptor {
    title: "Notifications",
    slug: "notifications",
    icon: icons::BELL,
    build,
};

fn build(cx: &mut Cx, store: Store) -> AnyView {
    // The master switch gates the other two rows.
    let disabled = move || bind(move |runtime| !store.get(runtime, notification::Enabled));

    page(
        &PAGE,
        (settings_card_titled(
            "Notifications",
            (
                setting_row_icon(
                    glyph(icons::BELL),
                    "Allow notifications",
                    switch(cx, store, notification::Enabled),
                ),
                settings_divider(),
                setting_row_desc(
                    "Do Not Disturb",
                    "Keep notifications in the tray without banners or sounds",
                    switch(cx, store, notification::DoNotDisturb).disabled(disabled()),
                ),
                settings_divider(),
                setting_row_desc(
                    "Show previews",
                    "Include message content in banners",
                    switch(cx, store, notification::ShowPreviews).disabled(disabled()),
                ),
            ),
        ),),
    )
}
