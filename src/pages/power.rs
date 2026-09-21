//! The Battery & Power page.

use crate::layout::{
    setting_row_desc, setting_row_icon, settings_card, settings_card_titled, settings_divider,
};
use blinc_icons::icons;
use crownos_config::schema::{PowerProfile, power};

use crate::controls::{Options, choice};
use crate::pages::{PageDescriptor, PageView, page};
use crate::state::Store;

/// Idle timeouts, in minutes. A file holding a value that isn't offered here —
/// including `0`, i.e. "never" — shows as no selection rather than as a lie
/// about what's stored.
static TIMEOUTS: Options<u32> = Options::new(&[
    ("1 minute", 1),
    ("5 minutes", 5),
    ("10 minutes", 10),
    ("15 minutes", 15),
    ("30 minutes", 30),
    ("1 hour", 60),
]);

static PROFILES: Options<PowerProfile> = Options::new(&[
    ("Power Saver", PowerProfile::PowerSaver),
    ("Balanced", PowerProfile::Balanced),
    ("Performance", PowerProfile::Performance),
]);

pub const PAGE: PageDescriptor = PageDescriptor {
    title: "Battery & Power",
    icon: icons::BATTERY_FULL,
    build,
};

fn build(store: &Store) -> PageView {
    page(
        &PAGE,
        (
            settings_card_titled(
                "Idle",
                (
                    setting_row_desc(
                        "Turn display off after",
                        "Blank the screen when there's no input",
                        // Both idle rows share one table: they are the same kind
                        // of value, and the keys keep them pointed at different
                        // fields.
                        choice(store, power::ScreenOffMinutes, &TIMEOUTS),
                    ),
                    settings_divider(),
                    setting_row_desc(
                        "Sleep after",
                        "Suspend the machine when there's no input",
                        choice(store, power::SleepMinutes, &TIMEOUTS),
                    ),
                ),
            ),
            settings_card((setting_row_icon(
                icons::BATTERY_FULL,
                "Power profile",
                choice(store, power::Profile, &PROFILES),
            ),)),
        ),
    )
}
