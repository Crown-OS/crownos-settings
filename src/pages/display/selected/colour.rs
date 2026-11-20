//! 10-bit scanout, and whether the display can show HDR.

use crownos_config::schema::Compositor;
use crownui::kit::toggle;
use crownui::prelude::{Cx, Runtime, Semantic, Signal, View, bind, vstack};

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
                scan_out_deep_color(compositor, &names, on);
            });
        },
    );
    let outputs = store.outputs;
    let description = bind(move |runtime| {
        let on = on.get(runtime);
        deep_color_note(on, scanout_bits(runtime, outputs, &name)).to_owned()
    });
    vstack((
        settings_divider(),
        setting_row_desc(
            "10-bit colour",
            description,
            toggle(on).label("10-bit colour"),
        ),
    ))
}

/// 8 bits is the default, so switching 10-bit off clears the override.
fn scan_out_deep_color(compositor: &mut Compositor, names: &OutputNames, on: bool) {
    names.set_override(
        compositor,
        |setting| &mut setting.deep_color,
        on.then_some(true),
    );
}

fn scanout_bits(runtime: &mut Runtime, outputs: Signal<OutputState>, name: &str) -> Option<u32> {
    outputs.with(runtime, |outputs| {
        outputs
            .monitor(name)
            .and_then(|monitor| monitor.described())
            .map(|info| info.scanout_bits)
    })
}

/// "Running at 8-bit" only when the compositor reported a shallower scanout.
fn deep_color_note(on: bool, scanout_bits: Option<u32>) -> &'static str {
    if on && scanout_bits.is_some_and(|bits| bits < DEEP_COLOR_BITS) {
        "Running at 8-bit"
    } else {
        "Scan out with 10 bits for each colour"
    }
}

pub fn hdr_row(hdr_capable: bool) -> impl View {
    let support = if hdr_capable {
        "Supported"
    } else {
        "Not supported"
    };
    vstack((settings_divider(), setting_row("HDR", value_text(support))))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pages::display::output_names::names;

    #[test]
    fn switching_10_bit_off_clears_the_override() {
        let monitor = names("DP-1", "DP-1");
        let mut compositor = Compositor::default();

        scan_out_deep_color(&mut compositor, &monitor, false);
        assert!(compositor.outputs.is_empty());

        scan_out_deep_color(&mut compositor, &monitor, true);
        assert_eq!(
            monitor
                .setting(&compositor)
                .and_then(|setting| setting.deep_color),
            Some(true)
        );

        scan_out_deep_color(&mut compositor, &monitor, false);
        assert_eq!(
            monitor
                .setting(&compositor)
                .map(|setting| setting.deep_color),
            Some(None)
        );
    }

    #[test]
    fn the_8_bit_note_shows_only_when_the_compositor_reported_8_bits() {
        let running_at_8_bit = "Running at 8-bit";
        assert_eq!(deep_color_note(true, Some(8)), running_at_8_bit);
        assert_ne!(deep_color_note(true, Some(10)), running_at_8_bit);
        assert_ne!(deep_color_note(true, None), running_at_8_bit);
        assert_ne!(deep_color_note(false, Some(8)), running_at_8_bit);
    }
}
