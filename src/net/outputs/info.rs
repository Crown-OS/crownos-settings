//! `crownos_output_info_v1`: the compositor's identity, primary flag and
//! capabilities for each output, applied a `done` at a time.

use crownos_protocols::output_info::v1::client::{
    crownos_output_info_manager_v1::CrownosOutputInfoManagerV1,
    crownos_output_info_v1::{self, Capability, CrownosOutputInfoV1},
};
use wayland_client::{Connection, Dispatch, QueueHandle, WEnum, protocol::wl_output};

use super::monitor::OutputInfo;
use super::protocol::State;

pub struct InfoTracker {
    proxy: CrownosOutputInfoV1,
    pending: OutputInfo,
    committed: Option<OutputInfo>,
}

impl InfoTracker {
    pub fn new(
        manager: &CrownosOutputInfoManagerV1,
        output: &wl_output::WlOutput,
        global: u32,
        queue: &QueueHandle<State>,
    ) -> Self {
        Self {
            proxy: manager.get_output(output, queue, global),
            pending: OutputInfo::default(),
            committed: None,
        }
    }

    pub fn described(&self) -> Option<&OutputInfo> {
        self.committed.as_ref()
    }

    pub fn destroy(&self) {
        self.proxy.destroy();
    }

    fn apply(&mut self, event: crownos_output_info_v1::Event) -> bool {
        use crownos_output_info_v1::{Event, State as Role};
        match event {
            Event::Identity { identity } => self.pending.identity = identity,
            Event::State { state } => {
                self.pending.primary =
                    known(state, Role::from_bits_truncate).contains(Role::Primary);
            }
            Event::Capabilities { capabilities } => {
                let capabilities = known(capabilities, Capability::from_bits_truncate);
                self.pending.hdr_capable = capabilities.contains(Capability::Hdr);
                self.pending.virtual_output = capabilities.contains(Capability::Virtual);
            }
            Event::ScanoutDepth { bits } => self.pending.scanout_bits = bits,
            Event::Done => {
                self.committed = Some(self.pending.clone());
                return true;
            }
            _ => {}
        }
        false
    }
}

fn known<T>(value: WEnum<T>, truncate: fn(u32) -> T) -> T {
    match value {
        WEnum::Value(value) => value,
        WEnum::Unknown(bits) => truncate(bits),
    }
}

impl Dispatch<CrownosOutputInfoManagerV1, ()> for State {
    fn event(
        _state: &mut Self,
        _proxy: &CrownosOutputInfoManagerV1,
        _event: <CrownosOutputInfoManagerV1 as wayland_client::Proxy>::Event,
        _data: &(),
        _conn: &Connection,
        _queue: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<CrownosOutputInfoV1, u32> for State {
    fn event(
        state: &mut Self,
        _proxy: &CrownosOutputInfoV1,
        event: crownos_output_info_v1::Event,
        global: &u32,
        _conn: &Connection,
        _queue: &QueueHandle<Self>,
    ) {
        let done = state
            .output_mut(*global)
            .and_then(|output| output.info.as_mut())
            .is_some_and(|info| info.apply(event));
        state.done |= done;
    }
}
