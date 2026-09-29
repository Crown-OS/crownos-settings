//! One paired device: what it is, what it may do, and forgetting it.

use blinc_icons::icons;
use crownuikit::config::theme;
use crownuikit::widgets::{ButtonVariant, Tone, badge, button, select, status, toggle};
use xilem::WidgetView;
use xilem::view::FlexSpacer;

use super::describe::{
    EDGES, battery_text, class_name, connection_summary, edge_index, feature_copy,
};
use super::parts::inline;
use crate::controls::value_text;
use crate::layout::{
    setting_row, setting_row_content, setting_row_desc, setting_row_icon_desc, settings_card,
    settings_card_titled, settings_divider,
};
use crate::net::crownconnect::{CrossDeviceState, Device, Feature};
use crate::pages::PageView;
use crate::state::Store;

pub fn cards(state: &CrossDeviceState, device: &Device) -> Vec<PageView> {
    let mut cards = vec![
        back_button().boxed(),
        summary_card(device).boxed(),
        features_card(device).boxed(),
    ];
    if let Some(controls) = controls_card(device) {
        cards.push(controls);
    }
    cards.push(forget_card(device, state.confirming_forget).boxed());
    cards
}

fn back_button() -> impl WidgetView<Store> {
    button("All Devices", |store: &mut Store| {
        store.cross_device.close()
    })
    .variant(ButtonVariant::Ghost)
    .small()
    .leading_icon(icons::CHEVRON_LEFT)
}

fn summary_card(device: &Device) -> impl WidgetView<Store> + use<> {
    let info = &device.info;
    let battery = info.battery.map(|battery| {
        (
            settings_divider(),
            setting_row("Battery", value_text(battery_text(battery))),
        )
    });
    settings_card_titled(
        info.name.clone(),
        (
            setting_row("Status", value_text(connection_summary(info))),
            settings_divider(),
            setting_row("Type", value_text(class_name(info.class))),
            battery,
        ),
    )
}

fn features_card(device: &Device) -> impl WidgetView<Store> + use<> {
    let mut rows: Vec<PageView> = Vec::with_capacity(Feature::ALL.len() * 2);
    for feature in Feature::ALL {
        if !rows.is_empty() {
            rows.push(settings_divider().boxed());
        }
        rows.push(feature_row(device, feature));
    }
    settings_card_titled("Features", rows)
}

fn feature_row(device: &Device, feature: Feature) -> PageView {
    let copy = feature_copy(feature);
    let id = device.info.id;
    let allowed = device.info.features.contains(feature);
    let running = device
        .info
        .active
        .contains(feature)
        .then(|| badge("In use", theme().status.success));
    let switch = toggle("", allowed, move |store: &mut Store, enabled| {
        store.cross_device.set_feature(id, feature, enabled);
    });
    setting_row_icon_desc(
        copy.glyph,
        copy.title,
        copy.description,
        inline((running, switch)),
    )
    .boxed()
}

/// The controls a feature unlocks, when any of them is allowed.
fn controls_card(device: &Device) -> Option<PageView> {
    let info = &device.info;
    let id = info.id;
    let offline = !info.connected;

    let hotspot = info.features.contains(Feature::Hotspot).then(|| {
        setting_row_desc(
            "Mobile hotspot",
            "Share the device's mobile data with this computer",
            toggle(
                "",
                device.hotspot.unwrap_or(false),
                move |store: &mut Store, enabled| {
                    store.cross_device.set_hotspot(id, enabled);
                },
            )
            .disabled(offline),
        )
        .boxed()
    });
    let edge = info.features.contains(Feature::Unicursor).then(|| {
        setting_row_desc(
            "Shared cursor edge",
            "Push the pointer past this side of the screen to reach the device",
            select(
                EDGES.map(|(name, _)| name),
                device.unicursor_edge.and_then(edge_index),
                move |store: &mut Store, index| {
                    if let Some(&(_, edge)) = EDGES.get(index) {
                        store.cross_device.set_unicursor_edge(id, edge);
                    }
                },
            )
            .disabled(offline),
        )
        .boxed()
    });

    let rows: Vec<PageView> = match (hotspot, edge) {
        (None, None) => return None,
        (Some(one), None) | (None, Some(one)) => vec![one],
        (Some(hotspot), Some(edge)) => vec![hotspot, settings_divider().boxed(), edge],
    };
    Some(settings_card_titled("Controls", rows).boxed())
}

fn forget_card(device: &Device, confirming: bool) -> impl WidgetView<Store> + use<> {
    let id = device.info.id;
    let row: PageView = if confirming {
        setting_row_content(inline((
            status(format!("Forget {}?", device.info.name)).tone(Tone::Danger),
            FlexSpacer::Flex(1.0),
            button("Cancel", |store: &mut Store| {
                store.cross_device.ask_forget(false);
            })
            .variant(ButtonVariant::Ghost)
            .small(),
            button("Forget", move |store: &mut Store| {
                store.cross_device.forget(id);
            })
            .variant(ButtonVariant::Destructive)
            .small(),
        )))
        .boxed()
    } else {
        setting_row_desc(
            "Forget this device",
            "Deletes its keys; it has to pair again to reconnect",
            button("Forget…", |store: &mut Store| {
                store.cross_device.ask_forget(true);
            })
            .variant(ButtonVariant::Destructive)
            .small(),
        )
        .boxed()
    };
    settings_card((row,))
}
