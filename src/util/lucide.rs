//! Lucide icon names, as plugin manifests spell them, to the SVG bodies the
//! kit draws.

use crownui::kit::icons;

/// What a name this table does not know is drawn with.
pub const FALLBACK: &str = icons::PUZZLE;

const NAMED: &[(&str, &str)] = &[
    ("activity", icons::ACTIVITY),
    ("battery", icons::BATTERY),
    ("bell", icons::BELL),
    ("bell-ring", icons::BELL_RING),
    ("bluetooth", icons::BLUETOOTH),
    ("calendar", icons::CALENDAR),
    ("camera", icons::CAMERA),
    ("chart-bar", icons::CHART_BAR),
    ("clipboard", icons::CLIPBOARD),
    ("clock", icons::CLOCK),
    ("cloud", icons::CLOUD),
    ("cloud-sun", icons::CLOUD_SUN),
    ("code", icons::CODE),
    ("cpu", icons::CPU),
    ("download", icons::DOWNLOAD),
    ("eye", icons::EYE),
    ("file", icons::FILE),
    ("folder", icons::FOLDER),
    ("gamepad-2", icons::GAMEPAD_2),
    ("gauge", icons::GAUGE),
    ("gift", icons::GIFT),
    ("globe", icons::GLOBE),
    ("hard-drive", icons::HARD_DRIVE),
    ("headphones", icons::HEADPHONES),
    ("heart", icons::HEART),
    ("house", icons::HOUSE),
    ("image", icons::IMAGE),
    ("keyboard", icons::KEYBOARD),
    ("layout-grid", icons::LAYOUT_GRID),
    ("list", icons::LIST),
    ("lock", icons::LOCK),
    ("mail", icons::MAIL),
    ("map", icons::MAP),
    ("memory-stick", icons::MEMORY_STICK),
    ("message-square", icons::MESSAGE_SQUARE),
    ("mic", icons::MIC),
    ("monitor", icons::MONITOR),
    ("moon", icons::MOON),
    ("music", icons::MUSIC),
    ("network", icons::NETWORK),
    ("newspaper", icons::NEWSPAPER),
    ("package", icons::PACKAGE),
    ("palette", icons::PALETTE),
    ("plug", icons::PLUG),
    ("puzzle", icons::PUZZLE),
    ("rss", icons::RSS),
    ("search", icons::SEARCH),
    ("settings", icons::SETTINGS),
    ("shield", icons::SHIELD),
    ("sparkles", icons::SPARKLES),
    ("star", icons::STAR),
    ("sun", icons::SUN),
    ("terminal", icons::TERMINAL),
    ("thermometer", icons::THERMOMETER),
    ("timer", icons::TIMER),
    ("volume-2", icons::VOLUME_2),
    ("wifi", icons::WIFI),
    ("wrench", icons::WRENCH),
    ("zap", icons::ZAP),
];

/// The glyph a Lucide name stands for, or [`FALLBACK`].
pub fn lucide_icon(name: &str) -> &'static str {
    NAMED
        .binary_search_by(|(known, _)| known.cmp(&name))
        .map_or(FALLBACK, |index| NAMED[index].1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_is_sorted_for_binary_search() {
        assert!(NAMED.windows(2).all(|pair| pair[0].0 < pair[1].0));
    }

    #[test]
    fn known_names_resolve_and_unknown_ones_fall_back() {
        assert_eq!(lucide_icon("cpu"), icons::CPU);
        assert_eq!(lucide_icon("cloud-sun"), icons::CLOUD_SUN);
        assert_eq!(lucide_icon("no-such-icon"), FALLBACK);
        assert_eq!(lucide_icon(""), FALLBACK);
    }
}
