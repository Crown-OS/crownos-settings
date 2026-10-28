//! Brightness and night light — the two settings on this page that are a
//! preference rather than a fact.
//!
//! Both are written to `display.ron` and both are carried out by the
//! compositor, which turns the pair into one gamma ramp per monitor. Nothing
//! here talks to the hardware.

use crownos_config::schema::display;
use crownui::kit::icons;
use crownui::prelude::{Cx, Semantic, View, bind};

use crate::controls::{percent_slider, switch};
use crate::layout::{
    glyph, setting_row, setting_row_desc, setting_row_icon, settings_card_titled, settings_divider,
};
use crate::state::Store;

pub fn brightness_card(cx: &mut Cx, store: Store) -> impl View + use<> {
    settings_card_titled(
        "Brightness",
        (setting_row_icon(
            glyph(icons::SUN),
            "Brightness",
            percent_slider(cx, store, display::Brightness),
        ),),
    )
}

pub fn night_light_card(cx: &mut Cx, store: Store) -> impl View + use<> {
    let off = bind(move |runtime| !store.get(runtime, display::NightLight));

    settings_card_titled(
        "Night Light",
        (
            setting_row_desc(
                "Night Light",
                "Shift colors warmer after sunset",
                switch(cx, store, display::NightLight),
            ),
            settings_divider(),
            setting_row(
                "Warmth",
                percent_slider(cx, store, display::NightLightWarmth).disabled(off),
            ),
        ),
    )
}
