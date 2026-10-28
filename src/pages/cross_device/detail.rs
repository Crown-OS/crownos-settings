//! One paired device: what it is, what it may do, and forgetting it.

use crownui::kit::{
    ButtonOptions, ButtonVariant, StatusOptions, Tone, badge, icons, select, status, toggle,
    use_theme,
};
use crownui::prelude::{
    AnyView, Cx, Runtime, Semantic, Signal, Styled, View, bind, component, keyed, spacer, vstack,
};

use super::describe::{
    EDGES, battery_text, class_name, connection_summary, edge_index, feature_copy,
};
use super::parts::{acting, row_button};
use crate::layout::{
    card_stack, glyph, inline, nothing, setting_row, setting_row_content, setting_row_desc,
    setting_row_icon_desc, settings_card, settings_card_titled, settings_divider, spread,
    value_text, when,
};
use crate::net::crownconnect::{CrossDeviceState, Device, DeviceId, Feature};
use crate::util::bridge::bridge;
use crate::util::theme::tint;

/// Reads the open device, or `R`'s default once it has gone.
fn device<R: Default>(
    runtime: &mut Runtime,
    state: Signal<CrossDeviceState>,
    id: DeviceId,
    read: impl FnOnce(&Device) -> R,
) -> R {
    state
        .with(runtime, |state| state.device(id).map(read))
        .unwrap_or_default()
}

pub fn cards(cx: &mut Cx, state: Signal<CrossDeviceState>, id: DeviceId, name: String) -> AnyView {
    card_stack((
        acting(
            row_button("All Devices", ButtonVariant::Ghost).leading_icon(icons::CHEVRON_LEFT),
            state,
            CrossDeviceState::close,
        )
        .self_start(),
        summary_card(state, id, &name),
        features_card(cx, state, id),
        controls_card(state, id),
        forget_card(state, id, name),
    ))
    .boxed()
}

fn summary_card(state: Signal<CrossDeviceState>, id: DeviceId, name: &str) -> impl View + use<> {
    let fact = move |read: fn(&Device) -> String| {
        value_text(bind(move |runtime| device(runtime, state, id, read)))
    };
    settings_card_titled(
        name,
        (
            setting_row(
                "Status",
                fact(|device| connection_summary(&device.info).to_owned()),
            ),
            settings_divider(),
            setting_row(
                "Type",
                fact(|device| class_name(device.info.class).to_owned()),
            ),
            when(
                move |runtime| device(runtime, state, id, |device| device.info.battery.is_some()),
                move |_| {
                    vstack((
                        settings_divider(),
                        setting_row(
                            "Battery",
                            fact(|device| {
                                device.info.battery.map(battery_text).unwrap_or_default()
                            }),
                        ),
                    ))
                },
            ),
        ),
    )
}

fn features_card(cx: &mut Cx, state: Signal<CrossDeviceState>, id: DeviceId) -> impl View + use<> {
    let rows: Vec<_> = Feature::ALL
        .into_iter()
        .enumerate()
        .map(|(index, feature)| {
            let hairline = if index == 0 {
                nothing().boxed()
            } else {
                settings_divider().boxed()
            };
            vstack((hairline, feature_row(cx, state, id, feature)))
        })
        .collect();
    settings_card_titled("Features", rows)
}

fn feature_row(
    cx: &mut Cx,
    state: Signal<CrossDeviceState>,
    id: DeviceId,
    feature: Feature,
) -> impl View + use<> {
    let copy = feature_copy(feature);
    let allowed = bridge(
        cx,
        move |runtime| {
            device(runtime, state, id, |device| {
                device.info.features.contains(feature)
            })
        },
        move |runtime, enabled| {
            state.update(runtime, |state| state.set_feature(id, feature, enabled))
        },
    );
    let running = when(
        move |runtime| {
            device(runtime, state, id, |device| {
                device.info.active.contains(feature)
            })
        },
        |_| {
            component(|cx| {
                let theme = use_theme(cx);
                badge("In use", tint(theme, |theme| theme.status.success))
            })
        },
    );
    setting_row_icon_desc(
        glyph(copy.glyph),
        copy.title,
        copy.description,
        inline((running, toggle(allowed))),
    )
}

/// The controls a feature unlocks, when any of them is allowed.
fn controls_card(state: Signal<CrossDeviceState>, id: DeviceId) -> impl View {
    keyed(
        move |runtime| {
            device(runtime, state, id, |device| {
                (
                    device.info.features.contains(Feature::Hotspot),
                    device.info.features.contains(Feature::Unicursor),
                )
            })
        },
        move |cx, (hotspot, edge)| {
            if !hotspot && !edge {
                return nothing().boxed();
            }
            let offline = move || {
                bind(move |runtime| !device(runtime, state, id, |device| device.info.connected))
            };
            let hotspot_row = hotspot.then(|| {
                let on = bridge(
                    cx,
                    move |runtime| {
                        device(runtime, state, id, |device| device.hotspot.unwrap_or(false))
                    },
                    move |runtime, enabled| {
                        state.update(runtime, |state| state.set_hotspot(id, enabled))
                    },
                );
                setting_row_desc(
                    "Mobile hotspot",
                    "Share the device's mobile data with this computer",
                    toggle(on).disabled(offline()),
                )
            });
            let edge_row = edge.then(|| {
                let names = cx.signal(EDGES.iter().map(|(name, _)| (*name).to_owned()).collect());
                let selected = bridge(
                    cx,
                    move |runtime| {
                        device(runtime, state, id, |device| {
                            device.unicursor_edge.and_then(edge_index)
                        })
                    },
                    move |runtime, index: Option<usize>| {
                        if let Some(&(_, edge)) = index.and_then(|index| EDGES.get(index)) {
                            state.update(runtime, |state| state.set_unicursor_edge(id, edge));
                        }
                    },
                );
                setting_row_desc(
                    "Shared cursor edge",
                    "Push the pointer past this side of the screen to reach the device",
                    select(names, selected).disabled(offline()),
                )
            });
            let between = (hotspot && edge).then(settings_divider);
            settings_card_titled("Controls", (hotspot_row, between, edge_row)).boxed()
        },
    )
}

fn forget_card(state: Signal<CrossDeviceState>, id: DeviceId, name: String) -> impl View {
    keyed(
        move |runtime| state.with(runtime, |state| state.confirming_forget),
        move |_, confirming| {
            if confirming {
                settings_card((setting_row_content(spread((
                    status(format!("Forget {name}?")).tone(Tone::Danger),
                    spacer(),
                    acting(row_button("Cancel", ButtonVariant::Ghost), state, |state| {
                        state.ask_forget(false);
                    }),
                    acting(
                        row_button("Forget", ButtonVariant::Destructive),
                        state,
                        move |state| {
                            state.forget(id);
                        },
                    ),
                ))),))
                .boxed()
            } else {
                settings_card((setting_row_desc(
                    "Forget this device",
                    "Deletes its keys; it has to pair again to reconnect",
                    acting(
                        row_button("Forget…", ButtonVariant::Destructive),
                        state,
                        |state| {
                            state.ask_forget(true);
                        },
                    ),
                ),))
                .boxed()
            }
        },
    )
}
