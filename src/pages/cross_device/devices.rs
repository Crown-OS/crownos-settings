//! The paired devices, one row each, and the way to add another.

use blinc_icons::icons;
use crownuikit::config::theme;
use crownuikit::widgets::{ButtonVariant, button, icon};
use xilem::WidgetView;

use super::describe::{battery_glyph, battery_text, class_glyph, connection_summary};
use super::parts::inline;
use crate::controls::value_text;
use crate::layout::{
    separated, setting_row_content, setting_row_icon_desc, settings_card_titled, settings_divider,
};
use crate::net::crownconnect::{CrossDeviceState, Device};
use crate::pages::PageView;
use crate::state::Store;

const BATTERY_GLYPH: f64 = 15.0;

pub fn card(state: &CrossDeviceState) -> impl WidgetView<Store> + use<> {
    let mut rows = separated(state.trusted().map(device_row));
    rows.push(settings_divider().boxed());
    rows.push(
        setting_row_content(
            button("Add Another Device…", |store: &mut Store| {
                store.cross_device.begin_pairing();
            })
            .variant(ButtonVariant::Secondary)
            .small()
            .leading_icon(icons::PLUS),
        )
        .boxed(),
    );
    settings_card_titled("My Devices", rows)
}

fn device_row(device: &Device) -> PageView {
    let info = &device.info;
    let id = info.id;

    let battery = info.battery.map(|battery| {
        inline((
            icon(battery_glyph(battery))
                .size(BATTERY_GLYPH)
                .color(theme().text.icon_muted),
            value_text(battery_text(battery)),
        ))
    });
    let open = button("Options…", move |store: &mut Store| {
        store.cross_device.open(id);
    })
    .variant(ButtonVariant::Ghost)
    .small();

    setting_row_icon_desc(
        class_glyph(info.class),
        info.name.clone(),
        connection_summary(info),
        inline((battery, open)),
    )
    .boxed()
}
