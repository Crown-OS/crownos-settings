//! The Battery & Power page.

use crownos_config::schema::{PowerProfile, power};
use crownui::kit::icons;
use crownui::prelude::{AnyView, Cx};

use crate::controls::{Options, choice};
use crate::layout::{
    glyph, setting_row_desc, setting_row_icon, settings_card, settings_card_titled,
    settings_divider,
};
use crate::pages::{PageDescriptor, page};
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
    slug: "battery",
    icon: icons::BATTERY_FULL,
    build,
};

fn build(cx: &mut Cx, store: Store) -> AnyView {
    page(
        &PAGE,
        (
            settings_card_titled(
                "Idle",
                (
                    setting_row_desc(
                        "Turn display off after",
                        "Blank the screen when there's no input",
                        choice(cx, store, power::ScreenOffMinutes, &TIMEOUTS),
                    ),
                    settings_divider(),
                    setting_row_desc(
                        "Sleep after",
                        "Suspend the machine when there's no input",
                        choice(cx, store, power::SleepMinutes, &TIMEOUTS),
                    ),
                ),
            ),
            settings_card((setting_row_icon(
                glyph(icons::BATTERY_FULL),
                "Power profile",
                choice(cx, store, power::Profile, &PROFILES),
            ),)),
        ),
    )
}
