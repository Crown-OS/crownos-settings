//! 10-bit scanout, and whether the display can show HDR.

use crownos_config::schema::Compositor;
use crownui::kit::toggle;
use crownui::prelude::{Cx, Runtime, Signal, View, bind, vstack};

use super::super::output_names::OutputNames;
use crate::layout::{setting_row, setting_row_desc, settings_divider, value_text};
use crate::net::outputs::OutputState;
use crate::state::Store;
use crate::util::bridge::bridge;

const DEEP_COLOR_BITS: u32 = 10;

pub fn deep_color_row(
    cx: &mut Cx,
    store: Store,
    name: String,
    names: OutputNames,
) -> impl View + use<> {
    let reading = names.clone();
    let on = bridge(
        cx,
        move |runtime| {
            store.section(runtime, |compositor: &Compositor| {
                reading
                    .setting(compositor)
                    .and_then(|setting| setting.deep_color)
                    .unwrap_or(false)
            })
        },
        move |runtime, on| {
            store.update(runtime, |compositor: &mut Compositor| {
                names.entry(compositor).deep_color = Some(on);
            });
        },
    );
    let outputs = store.outputs;
    let description = bind(move |runtime| {
        if on.get(runtime) && scanout_bits(runtime, outputs, &name) < Some(DEEP_COLOR_BITS) {
            "Running at 8-bit".to_owned()
        } else {
            "Scan out with 10 bits for each colour".to_owned()
        }
    });
    vstack((
        settings_divider(),
        setting_row_desc("10-bit colour", description, toggle(on)),
    ))
}

fn scanout_bits(runtime: &mut Runtime, outputs: Signal<OutputState>, name: &str) -> Option<u32> {
    outputs.with(runtime, |outputs| {
        outputs
            .monitor(name)
            .and_then(|monitor| monitor.described())
            .map(|info| info.scanout_bits)
    })
}

pub fn hdr_row(hdr_capable: bool) -> impl View {
    let support = if hdr_capable {
        "Supported"
    } else {
        "Not supported"
    };
    vstack((settings_divider(), setting_row("HDR", value_text(support))))
}
