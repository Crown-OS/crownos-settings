//! The cross-device backend: the crownconnect daemon, which pairs, links and
//! syncs this computer with phones, tablets and other computers.
//!
//! The house shape (see [`crate::net`]): [`CrossDeviceCommand`] for everything
//! the Cross-device page can ask, [`CrossDeviceEvent`] for everything the daemon
//! can say, and [`CrossDeviceState`] in the [`Store`](crate::state::Store).
//!
//! The daemon's schema types are re-exported rather than mirrored: they are
//! plain data declared by the daemon itself, so a copy here could only drift.
//! Nothing else of `crownos_ipc` escapes this module.

mod daemon;
mod state;
mod worker;

pub use crownconnect_linux::ipc::proto::{
    Battery, DeviceClass, DeviceId, DeviceInfo, Edge, Feature, FeatureSet, FeatureState, LinkKind,
};
pub use state::{
    CrossDeviceState, Device, Link, PairingInvite, PairingOutcome, PairingPrompt, PairingSession,
};

use crownconnect_linux::ipc::proto;
use xilem::ViewCtx;
use xilem::core::{NoElement, View};
use xilem::view::worker_raw;

/// Everything the Cross-device page can ask of the daemon.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CrossDeviceCommand {
    /// Open a pairing window and fetch its QR invitation.
    BeginPairing,
    /// Close the pairing window and void its invitation.
    CancelPairing,
    /// Answer a nearby device's request once the codes were compared.
    ConfirmPairing {
        id: DeviceId,
        accept: bool,
    },
    /// Unpair a device and delete its keys.
    Forget(DeviceId),
    SetFeature {
        id: DeviceId,
        feature: Feature,
        enabled: bool,
    },
    SetHotspot {
        id: DeviceId,
        enabled: bool,
    },
    SetUnicursorEdge {
        id: DeviceId,
        edge: Edge,
    },
}

/// Everything the worker can tell the page.
#[derive(Debug)]
pub enum CrossDeviceEvent {
    /// Connected and subscribed; the device list follows.
    Online,
    /// The daemon cannot be reached. Retries are already scheduled.
    Offline,
    /// A pushed daemon event, or a reply shaped like one.
    Daemon(proto::Event),
    /// The authoritative feature set of one device, after a toggle.
    Features {
        id: DeviceId,
        allowed: FeatureSet,
    },
    PairingInvite(PairingInvite),
    /// The wall clock, once a second while an invitation is counting down.
    Tick {
        now_unix_ms: u64,
    },
    /// An operation failed. Already human-readable; shown as-is.
    Error(String),
}

/// The cross-device backend, as an element-less view for
/// [`fork`](xilem::core::fork)'s alongside slot. Lives as long as the window.
pub fn crownconnect_worker<State>(
    cross_device: fn(&mut State) -> &mut CrossDeviceState,
) -> impl View<State, (), ViewCtx, Element = NoElement> + Send + Sync
where
    State: 'static,
{
    worker_raw(
        worker::run,
        move |state: &mut State, sender| cross_device(state).attach(sender),
        move |state: &mut State, event: CrossDeviceEvent| cross_device(state).apply(event),
    )
}
