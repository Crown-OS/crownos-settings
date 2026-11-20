//! When a monitor runs at a variable refresh rate.
//!
//! A preference in `compositor.ron` rather than a draft sent over
//! `zwlr_output_management_v1`: the protocol only knows on and off, and the
//! mode worth having — on while a game is showing — is neither.

use crownos_config::schema::{Compositor, Vrr};
use crownui::prelude::{Cx, IntoProp, Semantic, View};

use super::output_names::OutputNames;
use crate::controls::{Options, field_choice};
use crate::layout::setting_row_desc;
use crate::state::Store;

static VRR_MODES: Options<Vrr> = Options::new(&[
    ("Off", Vrr::Off),
    ("For games", Vrr::OnDemand),
    ("Always", Vrr::On),
]);

fn vrr_of(names: &OutputNames, compositor: &Compositor) -> Vrr {
    names
        .setting(compositor)
        .and_then(|setting| setting.vrr)
        .unwrap_or_default()
}

/// Only offered for a monitor the compositor said something about adaptive
/// sync for — the protocol has no capability event, so its silence is how a
/// connector that cannot do it says so.
pub fn row<Off: IntoProp<bool> + 'static>(
    cx: &mut Cx,
    store: Store,
    names: OutputNames,
    off: Off,
) -> impl View + use<Off> {
    let reading = names.clone();
    let mode = field_choice(
        cx,
        &VRR_MODES,
        move |runtime| {
            store.section(runtime, |compositor: &Compositor| {
                vrr_of(&reading, compositor)
            })
        },
        move |runtime, mode| {
            store.update(runtime, |compositor: &mut Compositor| {
                names.entry(compositor).vrr = Some(mode);
            });
        },
    );
    setting_row_desc(
        "Adaptive sync",
        "Match the refresh rate to what is being drawn",
        mode.disabled(off),
    )
}

#[cfg(test)]
mod tests {
    use crownos_config::DynSection;
    use crownos_config::schema::OutputSetting;

    use super::*;
    use crate::pages::display::output_names::names;

    const IDENTITY: &str = "Dell U2720Q 7XYZ";

    fn setting(name: &str, vrr: Option<Vrr>) -> OutputSetting {
        OutputSetting {
            name: name.into(),
            vrr,
            ..OutputSetting::default()
        }
    }

    fn with_outputs(outputs: Vec<OutputSetting>) -> Compositor {
        Compositor {
            outputs,
            ..Compositor::default()
        }
    }

    #[test]
    fn an_unconfigured_monitor_reads_as_on_demand() {
        let monitor = names("DP-1", "DP-1");
        assert_eq!(vrr_of(&monitor, &Compositor::default()), Vrr::OnDemand);
        assert_eq!(
            vrr_of(&monitor, &with_outputs(vec![setting("DP-1", None)])),
            Vrr::OnDemand
        );
    }

    #[test]
    fn every_mode_round_trips_through_the_file() {
        let monitor = names("DP-1", IDENTITY);
        for mode in VRR_MODES.values() {
            let mut written = with_outputs(vec![setting("HDMI-A-1", Some(Vrr::On))]);
            monitor.entry(&mut written).vrr = Some(mode);

            let text = written.get_ron("outputs").expect("serialises");
            let mut parsed = Compositor::default();
            parsed.set_ron("outputs", &text).expect("deserialises");

            assert_eq!(vrr_of(&monitor, &parsed), mode);
            assert_eq!(
                parsed.outputs[0].vrr,
                Some(Vrr::On),
                "another monitor's entry is left alone"
            );
        }
    }

    #[test]
    fn a_new_entry_is_filed_under_the_identity() {
        let mut compositor = Compositor::default();
        names("DP-1", IDENTITY).entry(&mut compositor).vrr = Some(Vrr::OnDemand);
        assert_eq!(compositor.outputs, [setting(IDENTITY, Some(Vrr::OnDemand))]);
    }

    #[test]
    fn the_connector_entry_wins_like_it_does_in_the_compositor() {
        let mut compositor = with_outputs(vec![
            setting(IDENTITY, Some(Vrr::On)),
            setting("DP-1", Some(Vrr::Off)),
        ]);
        let monitor = names("DP-1", IDENTITY);
        assert_eq!(vrr_of(&monitor, &compositor), Vrr::Off);

        monitor.entry(&mut compositor).vrr = Some(Vrr::OnDemand);
        assert_eq!(compositor.outputs[1].vrr, Some(Vrr::OnDemand));
        assert_eq!(compositor.outputs[0].vrr, Some(Vrr::On));
    }
}
