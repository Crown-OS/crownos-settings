//! The Battery & Power page.

use blinc_icons::icons;
use crownconfig::schema::{Power, PowerProfile};
use crownuikit::layouts::settings::{
    setting_row_desc, setting_row_icon, settings_card, settings_card_titled, settings_divider,
};

use crate::config::ConfigStore;
use crate::controls::{Options, choice};
use crate::pages::{PageDescriptor, PageView, page};

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

fn build(config: &ConfigStore) -> PageView {
    let power: &Power = config.section();

    page(
        PAGE.title,
        (
            settings_card_titled(
                "Idle",
                (
                    setting_row_desc(
                        "Turn display off after",
                        "Blank the screen when there's no input",
                        choice(
                            &TIMEOUTS,
                            power.screen_off_minutes,
                            |power: &mut Power, minutes| power.screen_off_minutes = minutes,
                        ),
                    ),
                    settings_divider(),
                    setting_row_desc(
                        "Sleep after",
                        "Suspend the machine when there's no input",
                        choice(
                            &TIMEOUTS,
                            power.sleep_minutes,
                            |power: &mut Power, minutes| {
                                power.sleep_minutes = minutes;
                            },
                        ),
                    ),
                ),
            ),
            settings_card((setting_row_icon(
                icons::BATTERY_FULL,
                "Power profile",
                choice(
                    &PROFILES,
                    power.power_profile,
                    |power: &mut Power, profile| power.power_profile = profile,
                ),
            ),)),
        ),
    )
}
