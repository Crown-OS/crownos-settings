//! Night light, a preference in `display.ron` that the compositor turns into
//! a gamma ramp on each monitor. Brightness is per monitor, in the selected
//! monitor's card.

use crownos_config::schema::display;
use crownui::prelude::{Cx, Semantic, View, bind};

use crate::controls::{percent_slider, switch};
use crate::layout::{setting_row, setting_row_desc, settings_card_titled, settings_divider};
use crate::state::Store;

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
