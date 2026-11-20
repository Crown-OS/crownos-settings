//! The `wl_output`s on this connection, joined to the output-management heads
//! by connector name. Both CrownOS output protocols take a `wl_output`, and
//! a head has none.

use wayland_client::{
    Connection, Dispatch, Proxy, QueueHandle,
    globals::GlobalListContents,
    protocol::{wl_output, wl_registry},
};

use super::brightness::BrightnessTracker;
use super::info::InfoTracker;
use super::protocol::State;

/// Version 4 adds the `name` event, which carries the connector name.
const OUTPUT_VERSION: u32 = 4;
const OUTPUT_INTERFACE: &str = "wl_output";

pub struct WaylandOutput {
    global: u32,
    output: wl_output::WlOutput,
    connector: Option<String>,
    pub info: Option<InfoTracker>,
    pub brightness: Option<BrightnessTracker>,
}

impl WaylandOutput {
    pub fn connector(&self) -> Option<&str> {
        self.connector.as_deref()
    }

    fn release(&self) {
        if let Some(info) = &self.info {
            info.destroy();
        }
        if let Some(brightness) = &self.brightness {
            brightness.destroy();
        }
        if self.output.version() >= 3 {
            self.output.release();
        }
    }
}

impl State {
    pub fn add_output(
        &mut self,
        registry: &wl_registry::WlRegistry,
        global: u32,
        interface: &str,
        version: u32,
        queue: &QueueHandle<Self>,
    ) {
        if interface != OUTPUT_INTERFACE || version < OUTPUT_VERSION {
            return;
        }
        let output: wl_output::WlOutput = registry.bind(global, OUTPUT_VERSION, queue, global);
        let info = self
            .info_manager
            .as_ref()
            .map(|manager| InfoTracker::new(manager, &output, global, queue));
        let brightness = self
            .brightness_manager
            .as_ref()
            .map(|manager| BrightnessTracker::new(manager, &output, global, queue));
        self.outputs.push(WaylandOutput {
            global,
            output,
            connector: None,
            info,
            brightness,
        });
    }

    fn remove_output(&mut self, global: u32) {
        self.outputs.retain(|output| {
            let removed = output.global == global;
            if removed {
                output.release();
            }
            !removed
        });
        self.done = true;
    }

    pub fn output_mut(&mut self, global: u32) -> Option<&mut WaylandOutput> {
        self.outputs
            .iter_mut()
            .find(|output| output.global == global)
    }

    pub fn output_named(&self, connector: &str) -> Option<&WaylandOutput> {
        self.outputs
            .iter()
            .find(|output| output.connector() == Some(connector))
    }
}

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for State {
    fn event(
        state: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _data: &GlobalListContents,
        _conn: &Connection,
        queue: &QueueHandle<Self>,
    ) {
        match event {
            wl_registry::Event::Global {
                name,
                interface,
                version,
            } => state.add_output(registry, name, &interface, version, queue),
            wl_registry::Event::GlobalRemove { name } => state.remove_output(name),
            _ => {}
        }
    }
}

impl Dispatch<wl_output::WlOutput, u32> for State {
    fn event(
        state: &mut Self,
        _proxy: &wl_output::WlOutput,
        event: wl_output::Event,
        global: &u32,
        _conn: &Connection,
        _queue: &QueueHandle<Self>,
    ) {
        if let wl_output::Event::Name { name } = event
            && let Some(output) = state.output_mut(*global)
        {
            output.connector = Some(name);
            state.done = true;
        }
    }
}
