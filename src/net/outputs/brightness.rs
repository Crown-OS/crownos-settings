//! `crownos_output_brightness_v1`: each output's brightness control and
//! perceptual level, which the compositor owns.

use crownos_protocols::output_brightness::v1::client::{
    crownos_output_brightness_manager_v1::CrownosOutputBrightnessManagerV1,
    crownos_output_brightness_v1::{self, Control, CrownosOutputBrightnessV1},
};
use wayland_client::{Connection, Dispatch, QueueHandle, WEnum, protocol::wl_output};

use super::monitor::{Brightness, BrightnessControl};
use super::protocol::State;

const FULL_LEVEL: f32 = 10_000.0;

pub struct BrightnessTracker {
    proxy: CrownosOutputBrightnessV1,
    control: Option<BrightnessControl>,
    level: Option<u32>,
}

impl BrightnessTracker {
    pub fn new(
        manager: &CrownosOutputBrightnessManagerV1,
        output: &wl_output::WlOutput,
        global: u32,
        queue: &QueueHandle<State>,
    ) -> Self {
        Self {
            proxy: manager.get_output(output, queue, global),
            control: None,
            level: None,
        }
    }

    pub fn brightness(&self) -> Option<Brightness> {
        Some(Brightness {
            control: self.control?,
            level: self.level? as f32 / FULL_LEVEL,
        })
    }

    pub fn set_level(&self, level: f32) {
        if self
            .control
            .is_some_and(|control| control != BrightnessControl::Fixed)
        {
            self.proxy
                .set_level((level.clamp(0.0, 1.0) * FULL_LEVEL).round() as u32);
        }
    }

    pub fn destroy(&self) {
        self.proxy.destroy();
    }

    fn apply(&mut self, event: crownos_output_brightness_v1::Event) -> bool {
        use crownos_output_brightness_v1::Event;
        match event {
            Event::Control { control } => {
                self.control = Some(control_from_wire(control));
                false
            }
            Event::Level { level } => {
                self.level = Some(level);
                true
            }
            _ => false,
        }
    }
}

fn control_from_wire(control: WEnum<Control>) -> BrightnessControl {
    match control.into_result() {
        Ok(Control::Backlight) => BrightnessControl::Backlight,
        Ok(Control::Ddc) => BrightnessControl::Ddc,
        Ok(Control::Software) => BrightnessControl::Software,
        _ => BrightnessControl::Fixed,
    }
}

impl Dispatch<CrownosOutputBrightnessManagerV1, ()> for State {
    fn event(
        _state: &mut Self,
        _proxy: &CrownosOutputBrightnessManagerV1,
        _event: <CrownosOutputBrightnessManagerV1 as wayland_client::Proxy>::Event,
        _data: &(),
        _conn: &Connection,
        _queue: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<CrownosOutputBrightnessV1, u32> for State {
    fn event(
        state: &mut Self,
        _proxy: &CrownosOutputBrightnessV1,
        event: crownos_output_brightness_v1::Event,
        global: &u32,
        _conn: &Connection,
        _queue: &QueueHandle<Self>,
    ) {
        let changed = state
            .output_mut(*global)
            .and_then(|output| output.brightness.as_mut())
            .is_some_and(|brightness| brightness.apply(event));
        state.done |= changed;
    }
}
