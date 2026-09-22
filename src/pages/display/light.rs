//! Brightness and night light — the two settings on this page that are a
//! preference rather than a fact.
//!
//! Both are written to `display.ron` and both are carried out by the
//! compositor, which turns the pair into one gamma ramp per monitor. Nothing
//! here talks to the hardware.

use blinc_icons::icons;
use crownos_config::schema::display;
use xilem::WidgetView;

use crate::controls::{percent_slider, switch};
use crate::layout::{
    setting_row, setting_row_desc, setting_row_icon, settings_card_titled, settings_divider,
};
use crate::state::Store;

pub fn brightness_card(store: &Store) -> impl WidgetView<Store> + use<> {
    settings_card_titled(
        "Brightness",
        (setting_row_icon(
            icons::SUN,
            "Brightness",
            percent_slider(store, display::Brightness),
        ),),
    )
}

pub fn night_light_card(store: &Store) -> impl WidgetView<Store> + use<> {
    let on = store.get(display::NightLight);

    settings_card_titled(
        "Night Light",
        (
            setting_row_desc(
                "Night Light",
                "Shift colors warmer after sunset",
                switch(store, display::NightLight),
            ),
            settings_divider(),
            setting_row(
                "Warmth",
                percent_slider(store, display::NightLightWarmth).disabled(!on),
            ),
        ),
    )
}
