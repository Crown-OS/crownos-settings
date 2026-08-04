//! The Appearance page.

use blinc_icons::icons;
use crownconfig::schema::{AccentColor, Appearance};
use crownuikit::layouts::settings::{
    setting_row, setting_row_desc, setting_row_icon, settings_card_titled, settings_divider,
};

use crate::config::ConfigStore;
use crate::controls::{Options, choice, fraction_slider, switch, value_text};
use crate::pages::{PageDescriptor, PageView, page};

static ACCENTS: Options<AccentColor> = Options::new(&[
    ("Purple", AccentColor::Purple),
    ("Blue", AccentColor::Blue),
    ("Green", AccentColor::Green),
    ("Orange", AccentColor::Orange),
    ("Pink", AccentColor::Pink),
]);

pub const PAGE: PageDescriptor = PageDescriptor {
    title: "Appearance",
    icon: icons::PALETTE,
    build,
};

fn build(config: &ConfigStore) -> PageView {
    let appearance: &Appearance = config.section();

    page(
        PAGE.title,
        (
            settings_card_titled(
                "Theme",
                (
                    setting_row_icon(
                        icons::MOON,
                        "Dark mode",
                        switch(appearance.dark_mode, |appearance: &mut Appearance, on| {
                            appearance.dark_mode = on;
                        }),
                    ),
                    settings_divider(),
                    setting_row(
                        "Accent color",
                        choice(
                            &ACCENTS,
                            appearance.accent,
                            |appearance: &mut Appearance, accent| appearance.accent = accent,
                        ),
                    ),
                    settings_divider(),
                    setting_row_desc(
                        "Transparency",
                        "Translucency of panels and menus",
                        // Stored 0.0–1.0, presented 0–100.
                        fraction_slider(
                            appearance.transparency,
                            |appearance: &mut Appearance, fraction| {
                                appearance.transparency = fraction;
                            },
                        ),
                    ),
                ),
            ),
            settings_card_titled(
                "Desktop",
                (setting_row("Wallpaper", value_text(wallpaper(appearance))),),
            ),
        ),
    )
}

/// The wallpaper path, or a placeholder — the schema spells "no wallpaper" as an
/// empty string, which would otherwise render as a blank row.
fn wallpaper(appearance: &Appearance) -> String {
    if appearance.wallpaper.is_empty() {
        "None".to_owned()
    } else {
        appearance.wallpaper.clone()
    }
}
