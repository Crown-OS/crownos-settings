//! The words and glyphs the page uses for the daemon's types.

use std::borrow::Cow;

use blinc_icons::icons;

use crate::net::crownconnect::{Battery, DeviceClass, DeviceInfo, Edge, Feature, LinkKind};

/// How the page presents one [`Feature`].
pub struct FeatureCopy {
    pub glyph: &'static str,
    pub title: &'static str,
    pub description: &'static str,
}

/// The unicursor edges, in the order the picker lists them.
pub const EDGES: [(&str, Edge); 4] = [
    ("Left", Edge::Left),
    ("Right", Edge::Right),
    ("Top", Edge::Top),
    ("Bottom", Edge::Bottom),
];

const BATTERY_LOW_BELOW: u8 = 20;
const BATTERY_FULL_FROM: u8 = 90;

pub const fn class_glyph(class: DeviceClass) -> &'static str {
    match class {
        DeviceClass::Phone => icons::SMARTPHONE,
        DeviceClass::Tablet => icons::TABLET,
        DeviceClass::Watch => icons::WATCH,
        DeviceClass::Computer => icons::LAPTOP,
    }
}

pub const fn class_name(class: DeviceClass) -> &'static str {
    match class {
        DeviceClass::Phone => "Phone",
        DeviceClass::Tablet => "Tablet",
        DeviceClass::Watch => "Watch",
        DeviceClass::Computer => "Computer",
    }
}

pub const fn connection_summary(info: &DeviceInfo) -> &'static str {
    if !info.connected {
        return "Not connected";
    }
    match info.link {
        LinkKind::Lan => "Connected over your network",
        LinkKind::Usb => "Connected over USB",
        LinkKind::Bluetooth => "Connected over Bluetooth",
    }
}

pub fn battery_text(battery: Battery) -> String {
    if battery.charging {
        format!("{}%, charging", battery.percent)
    } else {
        format!("{}%", battery.percent)
    }
}

pub const fn battery_glyph(battery: Battery) -> &'static str {
    if battery.charging {
        icons::BATTERY_CHARGING
    } else if battery.percent < BATTERY_LOW_BELOW {
        icons::BATTERY_LOW
    } else if battery.percent >= BATTERY_FULL_FROM {
        icons::BATTERY_FULL
    } else {
        icons::BATTERY_MEDIUM
    }
}

pub const fn feature_copy(feature: Feature) -> FeatureCopy {
    let (glyph, title, description) = match feature {
        Feature::Mirror => (
            icons::SCREEN_SHARE,
            "Screen mirroring",
            "Show either screen on the other",
        ),
        Feature::Camera => (
            icons::CAMERA,
            "Camera",
            "Use the device's camera as a webcam",
        ),
        Feature::Mic => (
            icons::MIC,
            "Microphone",
            "Use the device's microphone on this computer",
        ),
        Feature::Monitor => (
            icons::MONITOR,
            "Extra display",
            "Extend this desktop onto the device",
        ),
        Feature::Unicursor => (
            icons::MOUSE_POINTER,
            "Shared cursor",
            "Move the pointer and keyboard across to the device",
        ),
        Feature::Calls => (icons::PHONE, "Calls", "Answer and place calls from here"),
        Feature::Clipboard => (
            icons::CLIPBOARD,
            "Clipboard",
            "Copy on one device, paste on the other",
        ),
        Feature::Notifications => (
            icons::BELL,
            "Notifications",
            "Show the device's notifications here",
        ),
        Feature::Battery => (
            icons::BATTERY,
            "Battery",
            "Report the device's battery level",
        ),
        Feature::Hotspot => (
            icons::RADIO_TOWER,
            "Hotspot",
            "Turn on the device's mobile hotspot from here",
        ),
        Feature::Files => (icons::FOLDER, "Files", "Send files between the two"),
    };
    FeatureCopy {
        glyph,
        title,
        description,
    }
}

pub fn edge_index(edge: Edge) -> Option<usize> {
    EDGES.iter().position(|(_, candidate)| *candidate == edge)
}

/// `m:ss`, the way a countdown is read.
pub fn countdown_text(seconds: u64) -> String {
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

/// A six-digit code split into two groups of three, which is how people
/// compare it between two screens; anything else is shown as sent.
pub fn grouped_code(code: &str) -> Cow<'_, str> {
    match code.split_at_checked(3) {
        Some((head, tail)) if code.len() == 6 && code.bytes().all(|byte| byte.is_ascii_digit()) => {
            Cow::Owned(format!("{head} {tail}"))
        }
        _ => Cow::Borrowed(code),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_are_grouped_only_when_they_are_six_digits() {
        assert_eq!(grouped_code("123456"), "123 456");
        assert_eq!(grouped_code("12345"), "12345");
        assert_eq!(grouped_code("12a456"), "12a456");
        assert_eq!(grouped_code(""), "");
    }

    #[test]
    fn countdowns_read_as_minutes_and_seconds() {
        assert_eq!(countdown_text(0), "0:00");
        assert_eq!(countdown_text(59), "0:59");
        assert_eq!(countdown_text(300), "5:00");
        assert_eq!(countdown_text(61), "1:01");
    }

    #[test]
    fn every_feature_and_edge_is_described() {
        for feature in Feature::ALL {
            assert!(!feature_copy(feature).title.is_empty());
        }
        for (_, edge) in EDGES {
            assert!(edge_index(edge).is_some());
        }
    }

    #[test]
    fn the_battery_glyph_follows_level_and_charging() {
        let at = |percent, charging| battery_glyph(Battery { percent, charging });
        assert_eq!(at(50, true), icons::BATTERY_CHARGING);
        assert_eq!(at(19, false), icons::BATTERY_LOW);
        assert_eq!(at(20, false), icons::BATTERY_MEDIUM);
        assert_eq!(at(90, false), icons::BATTERY_FULL);
    }
}
