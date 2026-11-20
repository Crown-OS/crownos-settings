//! The brightness the compositor drives for one monitor.

use crownui::prelude::{Cx, Semantic, Signal, View, vstack};

use crate::controls::stepped_slider;
use crate::layout::{setting_row_desc, settings_divider};
use crate::net::outputs::{BrightnessControl, OutputState};
use crate::util::bridge::bridge;

const PERCENT: f32 = 100.0;

pub fn row(
    cx: &mut Cx,
    outputs: Signal<OutputState>,
    name: String,
    control: BrightnessControl,
) -> impl View + use<> {
    let reading = name.clone();
    let level = bridge(
        cx,
        move |runtime| {
            outputs.with(runtime, |outputs| {
                outputs
                    .monitor(&reading)
                    .and_then(|monitor| monitor.brightness)
                    .map_or(PERCENT, |brightness| brightness.level * PERCENT)
            })
        },
        move |runtime, percent| {
            outputs.update(runtime, |outputs| {
                outputs.set_brightness(&name, percent / PERCENT);
            });
        },
    );
    vstack((
        settings_divider(),
        setting_row_desc(
            "Brightness",
            control.label(),
            stepped_slider(level, 0.0, PERCENT, 1.0).disabled(control == BrightnessControl::Fixed),
        ),
    ))
}
