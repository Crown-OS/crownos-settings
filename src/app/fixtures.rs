//! A store as if every backend had spoken, for tests that look at the
//! populated pages.

use crownui::prelude::{Cx, Size};

use crate::net::crownconnect::{
    Battery, Device, DeviceClass, DeviceId, DeviceInfo, Feature, FeatureSet, Link, LinkKind,
};
use crate::net::outputs::{Mode, Monitor, Transform};
use crate::net::plugins::{
    CapabilityInfo, Decision, EntityInfo, EntityStatus, Field, Kind, Level, Link as PluginsLink,
    LogLevel, LogLine, SelectOption, SettingValue, SettingsSection,
};
use crate::net::wallpaper::{Thumbnail, WallpaperEntry};
use crate::net::wifi::{ActiveWifi, KnownNetwork, LinkState, OtherNetwork, WifiSnapshot};
use crate::state::Store;

/// Every backend as if it had spoken: networks with a prompt open, a
/// gallery of thumbnails, a monitor picked, a paired phone.
pub(super) fn populate(cx: &mut Cx, store: Store) {
    store.wifi.update(cx, |wifi| {
        wifi.snapshot = Some(WifiSnapshot {
            radio_enabled: true,
            present: true,
            hardware_blocked: false,
            connection: Some(ActiveWifi {
                ssid: "Workshop".to_owned(),
                state: LinkState::Connected,
                strength: Some(80),
                ip4_address: Some("192.168.1.20/24".to_owned()),
                weak_security: true,
            }),
            known: vec![
                KnownNetwork {
                    ssid: "Workshop".to_owned(),
                    uuid: "a".to_owned(),
                    strength: 80,
                    secured: true,
                    weak: true,
                    joined: true,
                    auto_join: true,
                },
                KnownNetwork {
                    ssid: "Attic".to_owned(),
                    uuid: "b".to_owned(),
                    strength: 40,
                    secured: true,
                    weak: false,
                    joined: false,
                    auto_join: false,
                },
            ],
            others: vec![
                OtherNetwork {
                    ssid: "Café".to_owned(),
                    strength: 60,
                    secured: true,
                    weak: false,
                },
                OtherNetwork {
                    ssid: "Guest".to_owned(),
                    strength: 20,
                    secured: false,
                    weak: false,
                },
            ],
        });
    });
    let entries: Vec<WallpaperEntry> = (0..11)
        .map(|index| {
            let (width, height) = if index % 3 == 0 { (64, 36) } else { (36, 64) };
            let rgba = (0..width * height)
                .flat_map(|pixel| {
                    let x = (pixel % width) as u8;
                    [x.wrapping_mul(4), (index * 23) as u8, 200 - x, 255]
                })
                .collect();
            WallpaperEntry {
                path: format!("/w/wallpaper-{index}.png"),
                thumbnail: Some(Thumbnail {
                    image: cx.register_image(width, height, rgba),
                    size: Size::new(width as f32, height as f32),
                }),
            }
        })
        .collect();
    store.wallpaper.update(cx, |wallpaper| {
        wallpaper.current = Some(entries[1].path.clone());
        wallpaper.entries = entries;
        wallpaper.scanned = true;
        wallpaper.online = true;
    });
    let monitor = Monitor {
        name: "DP-1".to_owned(),
        description: "Dell U2723QE".to_owned(),
        make: Some("Dell".to_owned()),
        model: Some("U2723QE".to_owned()),
        serial: None,
        enabled: true,
        modes: vec![Mode {
            width: 3840,
            height: 2160,
            refresh: 60000,
            preferred: true,
        }],
        current_mode: Some(0),
        position: (0, 0),
        scale: 1.5,
        transform: Transform::Normal,
        adaptive_sync: false,
        vrr_capable: true,
    };
    store.outputs.update(cx, |outputs| {
        outputs.drafts = vec![monitor.to_draft()];
        outputs.monitors = vec![monitor];
        outputs.loaded = true;
        outputs.selected = Some("DP-1".to_owned());
    });
    store.cross_device.update(cx, |state| {
        state.link = Link::Online;
        state.devices = vec![Device {
            info: DeviceInfo {
                id: DeviceId([7; 32]),
                name: "Pixel".to_owned(),
                class: DeviceClass::Phone,
                trusted: true,
                connected: true,
                link: LinkKind::Lan,
                battery: Some(Battery {
                    percent: 76,
                    charging: false,
                }),
                features: FeatureSet::EMPTY.with(Feature::Clipboard),
                active: FeatureSet::EMPTY,
            },
            hotspot: None,
            unicursor_edge: None,
        }];
    });
    populate_plugins(cx, store);
}

fn entity(id: &str, kind: Kind, name: &str) -> EntityInfo {
    EntityInfo {
        id: id.to_owned(),
        kind,
        name: name.to_owned(),
        version: "1.2.0".to_owned(),
        author: "Crown".to_owned(),
        description: format!("{name}, for testing"),
        dir: String::new(),
        system: false,
        active: true,
        dev: false,
        status: EntityStatus::Running,
        capabilities: Vec::new(),
        settings: None,
    }
}

/// The weather plugin's page: one field of every kind.
fn weather_settings() -> SettingsSection {
    SettingsSection {
        title: "Weather".to_owned(),
        icon: "cloud-sun".to_owned(),
        description: "Forecasts on the bar".to_owned(),
        fields: vec![
            Field::Toggle {
                key: "celsius".to_owned(),
                label: "Use Celsius".to_owned(),
                description: String::new(),
                default: true,
            },
            Field::Slider {
                key: "interval".to_owned(),
                label: "Refresh every".to_owned(),
                description: "Minutes between forecasts".to_owned(),
                min: 5.0,
                max: 60.0,
                step: 5.0,
                unit: " min".to_owned(),
                default: 15.0,
            },
            Field::Select {
                key: "units".to_owned(),
                label: "Wind".to_owned(),
                description: String::new(),
                options: vec![
                    SelectOption {
                        value: "kmh".to_owned(),
                        label: "km/h".to_owned(),
                    },
                    SelectOption {
                        value: "mph".to_owned(),
                        label: "mph".to_owned(),
                    },
                ],
                default: "kmh".to_owned(),
            },
            Field::ColorToken {
                key: "tint".to_owned(),
                label: "Icon color".to_owned(),
                description: String::new(),
                default: "accent.start".to_owned(),
            },
            Field::Text {
                key: "api_key".to_owned(),
                label: "API key".to_owned(),
                description: String::new(),
                secret: true,
                default: String::new(),
            },
            Field::File {
                key: "icons".to_owned(),
                label: "Icon pack".to_owned(),
                description: String::new(),
                filter: vec!["svg".to_owned()],
                default: String::new(),
            },
        ],
    }
}

/// A theme in use, a plugin with settings, and a quarantined dev plugin that
/// asks for every level of capability.
fn populate_plugins(cx: &mut Cx, store: Store) {
    let theme = entity("nord", Kind::Theme, "Nord");
    let weather = EntityInfo {
        settings: Some(weather_settings()),
        ..entity("weather", Kind::Plugin, "Weather")
    };
    let capability = |key: &str, level, decision| CapabilityInfo {
        key: key.to_owned(),
        summary: format!("Use {key}"),
        level,
        decision,
    };
    let broken = EntityInfo {
        dev: true,
        status: EntityStatus::Quarantined("panicked three times".to_owned()),
        capabilities: vec![
            capability("timer", Level::Safe, None),
            capability("net", Level::Sensitive, Some(Decision::Granted)),
            capability("exec", Level::Dangerous, None),
        ],
        ..entity("broken", Kind::Plugin, "Broken")
    };
    store.plugins.update(cx, |plugins| {
        plugins.link = PluginsLink::Online;
        plugins.entities = vec![theme, weather, broken];
        plugins.values.insert(
            "weather".to_owned(),
            [("interval".to_owned(), SettingValue::Number(30.0))].into(),
        );
        plugins.logs = vec![LogLine {
            id: "broken".to_owned(),
            level: LogLevel::Error,
            at: 0,
            message: "attempt to index a nil value".to_owned(),
        }];
    });
}
