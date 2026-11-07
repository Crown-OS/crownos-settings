//! When a monitor runs at a variable refresh rate.
//!
//! A preference in `compositor.ron` rather than a draft sent over
//! `zwlr_output_management_v1`: the protocol only knows on and off, and the
//! mode worth having — on while a game is showing — is neither.

use crownos_config::schema::{Compositor, OutputSetting, Vrr};
use crownui::prelude::{Cx, IntoProp, Semantic, View};

use crate::controls::{Options, field_choice};
use crate::layout::setting_row_desc;
use crate::net::outputs::Monitor;
use crate::state::Store;

static VRR_MODES: Options<Vrr> = Options::new(&[
    ("Off", Vrr::Off),
    ("For games", Vrr::OnDemand),
    ("Always", Vrr::On),
]);

/// The two names `compositor.ron` may file a monitor under, in the order the
/// compositor looks them up.
#[derive(Debug, Clone, PartialEq)]
pub struct OutputNames {
    connector: String,
    edid: Option<String>,
}

impl OutputNames {
    /// The EDID name is the compositor's: make, model and serial, and only
    /// when the panel reports a serial.
    pub fn of(monitor: &Monitor) -> Self {
        let edid = match (&monitor.make, &monitor.model, &monitor.serial) {
            (Some(make), Some(model), Some(serial)) if !serial.is_empty() => {
                Some(format!("{make} {model} {serial}"))
            }
            _ => None,
        };
        Self {
            connector: monitor.name.clone(),
            edid,
        }
    }

    fn position_in(&self, outputs: &[OutputSetting]) -> Option<usize> {
        outputs
            .iter()
            .position(|setting| setting.name == self.connector)
            .or_else(|| {
                let edid = self.edid.as_deref()?;
                outputs.iter().position(|setting| setting.name == edid)
            })
    }

    fn vrr_in(&self, outputs: &[OutputSetting]) -> Vrr {
        self.position_in(outputs)
            .and_then(|index| outputs[index].vrr)
            .unwrap_or_default()
    }

    fn set_vrr_in(&self, outputs: &mut Vec<OutputSetting>, mode: Vrr) {
        match self.position_in(outputs) {
            Some(index) => outputs[index].vrr = Some(mode),
            None => outputs.push(OutputSetting {
                name: self.edid.as_ref().unwrap_or(&self.connector).clone(),
                vrr: Some(mode),
                ..OutputSetting::default()
            }),
        }
    }
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
                reading.vrr_in(&compositor.outputs)
            })
        },
        move |runtime, mode| {
            store.update(runtime, |compositor: &mut Compositor| {
                names.set_vrr_in(&mut compositor.outputs, mode);
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

    use super::*;

    fn names(edid: Option<&str>) -> OutputNames {
        OutputNames {
            connector: "DP-1".into(),
            edid: edid.map(str::to_owned),
        }
    }

    fn setting(name: &str, vrr: Option<Vrr>) -> OutputSetting {
        OutputSetting {
            name: name.into(),
            vrr,
            ..OutputSetting::default()
        }
    }

    #[test]
    fn an_unconfigured_monitor_reads_as_on_demand() {
        assert_eq!(names(None).vrr_in(&[]), Vrr::OnDemand);
        assert_eq!(names(None).vrr_in(&[setting("DP-1", None)]), Vrr::OnDemand);
    }

    #[test]
    fn every_mode_round_trips_through_the_file() {
        let monitor = names(Some("Dell U2720Q 7XYZ"));
        for mode in VRR_MODES.values() {
            let mut outputs = vec![setting("HDMI-A-1", Some(Vrr::On))];
            monitor.set_vrr_in(&mut outputs, mode);

            let written = Compositor {
                outputs,
                ..Compositor::default()
            };
            let text = written.get_ron("outputs").expect("serialises");
            let mut parsed = Compositor::default();
            parsed.set_ron("outputs", &text).expect("deserialises");

            assert_eq!(monitor.vrr_in(&parsed.outputs), mode);
            assert_eq!(
                parsed.outputs[0].vrr,
                Some(Vrr::On),
                "another monitor's entry is left alone"
            );
        }
    }

    #[test]
    fn a_new_entry_is_filed_under_the_edid_name() {
        let mut outputs = Vec::new();
        names(Some("Dell U2720Q 7XYZ")).set_vrr_in(&mut outputs, Vrr::OnDemand);
        assert_eq!(outputs, [setting("Dell U2720Q 7XYZ", Some(Vrr::OnDemand))]);
    }

    #[test]
    fn the_connector_entry_wins_like_it_does_in_the_compositor() {
        let mut outputs = vec![
            setting("Dell U2720Q 7XYZ", Some(Vrr::On)),
            setting("DP-1", Some(Vrr::Off)),
        ];
        let monitor = names(Some("Dell U2720Q 7XYZ"));
        assert_eq!(monitor.vrr_in(&outputs), Vrr::Off);

        monitor.set_vrr_in(&mut outputs, Vrr::OnDemand);
        assert_eq!(outputs[1].vrr, Some(Vrr::OnDemand));
        assert_eq!(outputs[0].vrr, Some(Vrr::On));
    }
}
