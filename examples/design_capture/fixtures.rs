//! A believable machine for every backend to describe: nothing here is read
//! from the system the capture runs on.

use crownos_config::schema::lock::ClockKey;
use crownos_config::schema::{LockBackground, appearance, bluetooth, input, lock};
use crownplugin_model::SelectOption;
use crownui::prelude::Cx;

use crate::net::audio::{Microphone, Microphones};
use crate::net::crownlink::{
    Battery, Device, DeviceClass, DeviceId, DeviceInfo, Edge, Feature, FeatureSet, Link, LinkKind,
};
use crate::net::outputs::{InfoState, Mode, Monitor, Transform};
use crate::net::plugins::{
    CapabilityInfo, Decision, EntityInfo, EntityStatus, Field, Kind, Level, Link as PluginsLink,
    LogLevel, LogLine, SettingValue, SettingsSection,
};
use crate::net::voice::VoiceService;
use crate::net::wifi::{ActiveWifi, KnownNetwork, LinkState, OtherNetwork, WifiSnapshot};
use crate::state::Store;
use crate::wallpapers::{CURRENT_WALLPAPER, wallpaper_path};

pub const PHONE: DeviceId = DeviceId([7; 32]);
const TABLET: DeviceId = DeviceId([9; 32]);
pub const JOINED_NETWORK: &str = "Harbor Lane 5G";
pub const SECURED_NETWORK: &str = "Blue Bottle Café";
const KNOWN_NETWORK: &str = "Studio Loft";

/// The settings every capture shares: the stock wallpaper behind the desktop
/// and the lock, no account name on the lock, and no depth cut to wait for.
pub fn neutral_settings(cx: &mut Cx, store: Store) {
    store.set(cx, appearance::Wallpaper, wallpaper_path(CURRENT_WALLPAPER));
    store.set(cx, lock::ShowUser, false);
    store.set(cx, lock::Background, LockBackground::Desktop);
    store.set(cx, bluetooth::Enabled, true);
    store.set(cx, input::DictationMicrophone, None);
    let mut look = store.get(cx, ClockKey);
    look.depth = false;
    store.set(cx, ClockKey, look);
}

/// Every backend as if it had spoken.
pub fn populate(cx: &mut Cx, store: Store) {
    store
        .wifi
        .update(cx, |wifi| wifi.snapshot = Some(wifi_snapshot()));
    store.outputs.update(cx, |outputs| {
        let monitors = monitors();
        outputs.drafts = monitors.iter().map(Monitor::to_draft).collect();
        outputs.monitors = monitors;
        outputs.loaded = true;
        outputs.selected = Some("DP-1".to_owned());
    });
    store.cross_device.update(cx, |state| {
        state.link = Link::Online;
        state.devices = devices();
    });
    store.plugins.update(cx, |plugins| {
        plugins.link = PluginsLink::Online;
        plugins.entities = entities();
        plugins.values.insert(
            "crown.example.cpu-graph".to_owned(),
            [("interval".to_owned(), SettingValue::Number(3.0))].into(),
        );
    });
    store.microphones.set(cx, microphones());
    store.voice.update(cx, |voice| {
        voice.service = VoiceService::Running {
            backend: Some("cuda: GeForce RTX 4070 Laptop GPU".to_owned()),
        };
    });
}

pub fn wifi_snapshot() -> WifiSnapshot {
    let known = |ssid: &str, uuid: &str, strength, weak, joined, auto_join| KnownNetwork {
        ssid: ssid.to_owned(),
        uuid: uuid.to_owned(),
        strength,
        secured: true,
        weak,
        joined,
        auto_join,
    };
    let other = |ssid: &str, strength, secured, weak| OtherNetwork {
        ssid: ssid.to_owned(),
        strength,
        secured,
        weak,
    };
    WifiSnapshot {
        radio_enabled: true,
        present: true,
        hardware_blocked: false,
        connection: Some(ActiveWifi {
            ssid: JOINED_NETWORK.to_owned(),
            state: LinkState::Connected,
            strength: Some(84),
            ip4_address: Some("192.168.1.42/24".to_owned()),
            weak_security: false,
        }),
        known: vec![
            known(JOINED_NETWORK, "3f1c", 84, false, true, true),
            known(KNOWN_NETWORK, "8a20", 61, false, false, true),
            known("Northside Library", "c4e7", 32, true, false, false),
        ],
        others: vec![
            other(SECURED_NETWORK, 68, true, false),
            other("Pixel 9 Pro Hotspot", 52, true, false),
            other("DIRECT-4F OfficeJet Pro", 38, true, true),
            other("Airport Free Wi-Fi", 24, false, false),
        ],
    }
}

fn monitors() -> Vec<Monitor> {
    let mode = |width, height, refresh, preferred| Mode {
        width,
        height,
        refresh,
        preferred,
    };
    vec![
        Monitor {
            name: "eDP-1".to_owned(),
            description: "Built-in display".to_owned(),
            make: Some("Samsung".to_owned()),
            model: Some("ATNA60YV".to_owned()),
            serial: None,
            enabled: true,
            modes: vec![
                mode(2880, 1800, 120_000, true),
                mode(2880, 1800, 60_000, false),
                mode(1920, 1200, 60_000, false),
            ],
            current_mode: Some(0),
            position: (0, 360),
            scale: 2.0,
            transform: Transform::Normal,
            vrr_capable: true,
            info: InfoState::Unserved,
            brightness: None,
        },
        Monitor {
            name: "DP-1".to_owned(),
            description: "Dell U2723QE".to_owned(),
            make: Some("Dell".to_owned()),
            model: Some("U2723QE".to_owned()),
            serial: None,
            enabled: true,
            modes: vec![
                mode(3840, 2160, 60_000, true),
                mode(3840, 2160, 30_000, false),
                mode(2560, 1440, 60_000, false),
                mode(1920, 1080, 60_000, false),
            ],
            current_mode: Some(0),
            position: (1440, 0),
            scale: 1.5,
            transform: Transform::Normal,
            vrr_capable: false,
            info: InfoState::Unserved,
            brightness: None,
        },
    ]
}

fn devices() -> Vec<Device> {
    vec![
        Device {
            info: DeviceInfo {
                id: PHONE,
                name: "Pixel 9 Pro".to_owned(),
                class: DeviceClass::Phone,
                trusted: true,
                connected: true,
                link: LinkKind::Lan,
                battery: Some(Battery {
                    percent: 76,
                    charging: false,
                }),
                features: FeatureSet::EMPTY
                    .with(Feature::Clipboard)
                    .with(Feature::Notifications)
                    .with(Feature::Files)
                    .with(Feature::Battery)
                    .with(Feature::Hotspot)
                    .with(Feature::Unicursor)
                    .with(Feature::Camera),
                active: FeatureSet::EMPTY
                    .with(Feature::Clipboard)
                    .with(Feature::Notifications),
            },
            hotspot: Some(false),
            unicursor_edge: Some(Edge::Left),
        },
        Device {
            info: DeviceInfo {
                id: TABLET,
                name: "Galaxy Tab S9".to_owned(),
                class: DeviceClass::Tablet,
                trusted: true,
                connected: false,
                link: LinkKind::Lan,
                battery: Some(Battery {
                    percent: 41,
                    charging: true,
                }),
                features: FeatureSet::EMPTY
                    .with(Feature::Monitor)
                    .with(Feature::Unicursor),
                active: FeatureSet::EMPTY,
            },
            hotspot: None,
            unicursor_edge: None,
        },
    ]
}

fn microphones() -> Microphones {
    let microphone = |node_name: &str, description: &str| Microphone {
        node_name: node_name.to_owned(),
        description: description.to_owned(),
    };
    Microphones {
        available: vec![
            microphone(
                "alsa_input.pci-0000_00_1f.3.analog-stereo",
                "Built-in Microphone",
            ),
            microphone("alsa_input.usb-blue-yeti", "Yeti Stereo Microphone"),
        ],
        default: Some("Built-in Microphone".to_owned()),
    }
}

fn entity(id: &str, kind: Kind, name: &str, description: &str) -> EntityInfo {
    EntityInfo {
        id: id.to_owned(),
        kind,
        name: name.to_owned(),
        version: "1.0.0".to_owned(),
        author: "CrownOS".to_owned(),
        description: description.to_owned(),
        dir: String::new(),
        system: false,
        active: true,
        dev: false,
        status: EntityStatus::Running,
        capabilities: Vec::new(),
        settings: None,
    }
}

fn capability(
    key: &str,
    summary: &str,
    level: Level,
    decision: Option<Decision>,
) -> CapabilityInfo {
    CapabilityInfo {
        key: key.to_owned(),
        summary: summary.to_owned(),
        level,
        decision,
    }
}

pub const CPU_GRAPH: &str = "crown.example.cpu-graph";
pub const NOW_PLAYING: &str = "crown.example.now-playing";
pub const MIDNIGHT: &str = "crown.example.midnight";

fn cpu_graph_settings() -> SettingsSection {
    SettingsSection {
        title: "CPU Graph".to_owned(),
        icon: "cpu".to_owned(),
        description: "A live load graph on the bar".to_owned(),
        fields: vec![
            Field::Slider {
                key: "interval".to_owned(),
                label: "Refresh every".to_owned(),
                description: "Seconds between samples".to_owned(),
                min: 1.0,
                max: 10.0,
                step: 1.0,
                unit: " s".to_owned(),
                default: 2.0,
            },
            Field::Toggle {
                key: "show_label".to_owned(),
                label: "Show percentage".to_owned(),
                description: "Next to the graph".to_owned(),
                default: true,
            },
            Field::Select {
                key: "style".to_owned(),
                label: "Graph style".to_owned(),
                description: String::new(),
                options: vec![
                    SelectOption {
                        value: "line".to_owned(),
                        label: "Line".to_owned(),
                    },
                    SelectOption {
                        value: "bars".to_owned(),
                        label: "Bars".to_owned(),
                    },
                ],
                default: "line".to_owned(),
            },
            Field::ColorToken {
                key: "tint".to_owned(),
                label: "Graph color".to_owned(),
                description: String::new(),
                default: "accent.start".to_owned(),
            },
            Field::Text {
                key: "label".to_owned(),
                label: "Label".to_owned(),
                description: String::new(),
                secret: false,
                default: "CPU".to_owned(),
            },
            Field::File {
                key: "icon".to_owned(),
                label: "Icon".to_owned(),
                description: String::new(),
                filter: vec!["svg".to_owned()],
                default: String::new(),
            },
        ],
    }
}

fn entities() -> Vec<EntityInfo> {
    let nord = EntityInfo {
        version: "2.1.0".to_owned(),
        author: "Arctic Studio".to_owned(),
        ..entity(
            "crown.community.nord",
            Kind::Theme,
            "Nord",
            "Frost blues and aurora accents",
        )
    };
    let midnight = EntityInfo {
        active: false,
        status: EntityStatus::Inactive,
        version: "1.2.0".to_owned(),
        ..entity(
            MIDNIGHT,
            Kind::Theme,
            "Midnight",
            "Deep blues with a day and a night wallpaper",
        )
    };
    let cpu_graph = EntityInfo {
        settings: Some(cpu_graph_settings()),
        capabilities: vec![capability(
            "feed.system",
            "Read CPU and memory load",
            Level::Safe,
            None,
        )],
        ..entity(
            CPU_GRAPH,
            Kind::Plugin,
            "CPU Graph",
            "Live processor load on the bar",
        )
    };
    let now_playing = EntityInfo {
        dev: true,
        status: EntityStatus::Quarantined("Crashed three times in a minute".to_owned()),
        capabilities: vec![
            capability("feed.time", "Know the time", Level::Safe, None),
            capability(
                "net",
                "Fetch album art from the internet",
                Level::Sensitive,
                Some(Decision::Granted),
            ),
            capability("exec.playerctl", "Run playerctl", Level::Dangerous, None),
        ],
        ..entity(
            NOW_PLAYING,
            Kind::Plugin,
            "Now Playing",
            "The current track in the bar, with playback controls in its panel",
        )
    };
    let launcher_tools = EntityInfo {
        active: false,
        status: EntityStatus::Inactive,
        ..entity(
            "crown.example.launcher-tools",
            Kind::Plugin,
            "Launcher Tools",
            "A calculator and web search in the launcher",
        )
    };
    vec![nord, midnight, cpu_graph, now_playing, launcher_tools]
}

/// The recent log the detail view of `id` shows, stamped minutes before now.
pub fn logs_of(id: &str) -> Vec<LogLine> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_millis() as u64);
    let line = |level, minutes_ago: u64, message: &str| LogLine {
        id: id.to_owned(),
        level,
        at: now.saturating_sub(minutes_ago * 60_000),
        message: message.to_owned(),
    };
    match id {
        NOW_PLAYING => vec![
            line(LogLevel::Info, 14, "started"),
            line(LogLevel::Warn, 9, "playerctl: no players found"),
            line(
                LogLevel::Error,
                2,
                "main.luau:42: attempt to index a nil value (field 'metadata')",
            ),
        ],
        CPU_GRAPH => vec![
            line(LogLevel::Info, 48, "started"),
            line(LogLevel::Info, 47, "sampling every 3 s"),
        ],
        _ => Vec::new(),
    }
}

/// [`populate`], plus the stock wallpapers in the picker, entered first so
/// that opening the page does not mark the gallery unscanned again.
pub fn populate_all(cx: &mut Cx, store: Store) {
    populate(cx, store);
    let entries = crate::wallpapers::gallery(cx);
    store.wallpaper.update(cx, |wallpaper| {
        wallpaper.enter();
        wallpaper.current = Some(wallpaper_path(CURRENT_WALLPAPER));
        wallpaper.entries = entries;
        wallpaper.scanned = true;
        wallpaper.online = true;
    });
}
