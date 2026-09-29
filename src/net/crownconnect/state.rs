//! What the Cross-device page reads, and the pure folding of daemon events
//! into it.

use crownconnect_linux::ipc::proto::{self, Event};
use xilem::ImageBrush;
use xilem::tokio::sync::mpsc::UnboundedSender;

use super::{
    CrossDeviceCommand, CrossDeviceEvent, DeviceId, DeviceInfo, Edge, Feature, FeatureSet,
    FeatureState,
};

/// Used when a pairing finishes for a device that never named itself.
const UNNAMED_DEVICE: &str = "your device";

/// Whether the daemon can be reached.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Link {
    /// The first attempt has not come back yet.
    #[default]
    Connecting,
    Online,
    Offline,
}

/// One device, plus the two facts the daemon pushes but never lists.
#[derive(Debug, Clone, PartialEq)]
pub struct Device {
    pub info: DeviceInfo,
    /// `None` until a [`proto::HotspotChanged`] arrives or the user toggles it.
    pub hotspot: Option<bool>,
    /// The edge last chosen in this session; the schema has no getter.
    pub unicursor_edge: Option<Edge>,
}

impl Device {
    fn new(info: DeviceInfo) -> Self {
        Self {
            info,
            hotspot: None,
            unicursor_edge: None,
        }
    }
}

/// The QR invitation on screen, rasterized once when it arrives.
#[derive(Debug, Clone, PartialEq)]
pub struct PairingInvite {
    pub qr: ImageBrush,
    pub expires_unix_ms: u64,
}

impl PairingInvite {
    /// Whole seconds until expiry, rounded up so zero means expired.
    pub fn seconds_left(&self, now_unix_ms: u64) -> u64 {
        self.expires_unix_ms
            .saturating_sub(now_unix_ms)
            .div_ceil(1000)
    }
}

/// A nearby device asking to pair, with the code both screens show.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PairingPrompt {
    pub id: DeviceId,
    pub name: String,
    pub code: String,
    /// Accept or Decline was pressed; waiting for the result.
    pub answered: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PairingOutcome {
    Paired { name: String },
    Refused,
}

/// One run of the pairing flow, from "Connect a device" to its outcome.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PairingSession {
    /// `None` while `pairing_begin` is in flight.
    pub invite: Option<PairingInvite>,
    pub prompt: Option<PairingPrompt>,
    pub outcome: Option<PairingOutcome>,
}

/// The cross-device half of the app's [`Store`](crate::state::Store).
///
/// Runtime only. A fresh [`default`](Default::default) is "still connecting,
/// nothing heard", which is what the page-building test renders.
#[derive(Debug, Default)]
pub struct CrossDeviceState {
    /// Set once, when the worker is first built.
    sender: Option<UnboundedSender<CrossDeviceCommand>>,
    pub link: Link,
    /// Every device the daemon listed, paired or merely nearby.
    pub devices: Vec<Device>,
    /// `Some` while the pairing flow is on screen.
    pub pairing: Option<PairingSession>,
    /// The device whose detail view is open.
    pub selected: Option<DeviceId>,
    /// The detail view asked "Forget this device?".
    pub confirming_forget: bool,
    /// The last [`CrossDeviceEvent::Tick`], for the invitation countdown.
    pub now_unix_ms: u64,
    /// The last failed operation, until the user dismisses it.
    pub error: Option<String>,
}

impl CrossDeviceState {
    /// Ask the worker to do something. Silent without a worker, and retires a
    /// channel whose worker is gone.
    fn send(&mut self, command: CrossDeviceCommand) {
        let Some(sender) = self.sender.as_ref() else {
            return;
        };
        if sender.send(command).is_err() {
            self.sender = None;
        }
    }

    pub(super) fn attach(&mut self, sender: UnboundedSender<CrossDeviceCommand>) {
        self.sender = Some(sender);
    }

    pub fn device(&self, id: DeviceId) -> Option<&Device> {
        self.devices.iter().find(|device| device.info.id == id)
    }

    fn device_mut(&mut self, id: DeviceId) -> Option<&mut Device> {
        self.devices.iter_mut().find(|device| device.info.id == id)
    }

    /// Paired devices, in the daemon's order.
    pub fn trusted(&self) -> impl Iterator<Item = &Device> {
        self.devices.iter().filter(|device| device.info.trusted)
    }

    /// Devices seen nearby that are not paired yet.
    pub fn nearby(&self) -> impl Iterator<Item = &Device> {
        self.devices.iter().filter(|device| !device.info.trusted)
    }

    // --- MARK: Page verbs ---

    /// Start (or restart, for an expired code) the pairing flow.
    pub fn begin_pairing(&mut self) {
        self.pairing = Some(PairingSession::default());
        self.selected = None;
        self.send(CrossDeviceCommand::BeginPairing);
    }

    /// Leave the pairing flow, closing the daemon's pairing window.
    pub fn end_pairing(&mut self) {
        self.pairing = None;
        self.send(CrossDeviceCommand::CancelPairing);
    }

    pub fn answer_pairing(&mut self, accept: bool) {
        let Some(prompt) = self
            .pairing
            .as_mut()
            .and_then(|session| session.prompt.as_mut())
            .filter(|prompt| !prompt.answered)
        else {
            return;
        };
        prompt.answered = true;
        let id = prompt.id;
        self.send(CrossDeviceCommand::ConfirmPairing { id, accept });
    }

    pub fn open(&mut self, id: DeviceId) {
        self.selected = Some(id);
        self.confirming_forget = false;
    }

    pub fn close(&mut self) {
        self.selected = None;
        self.confirming_forget = false;
    }

    pub fn ask_forget(&mut self, asking: bool) {
        self.confirming_forget = asking;
    }

    pub fn forget(&mut self, id: DeviceId) {
        self.close();
        self.send(CrossDeviceCommand::Forget(id));
    }

    /// Flip a feature at once; the worker's feature refresh corrects it if the
    /// daemon disagrees.
    pub fn set_feature(&mut self, id: DeviceId, feature: Feature, enabled: bool) {
        if let Some(device) = self.device_mut(id) {
            let state = if enabled {
                FeatureState::Enabled
            } else {
                FeatureState::Disabled
            };
            apply_feature_state(&mut device.info, feature, state);
        }
        self.send(CrossDeviceCommand::SetFeature {
            id,
            feature,
            enabled,
        });
    }

    pub fn set_hotspot(&mut self, id: DeviceId, enabled: bool) {
        if let Some(device) = self.device_mut(id) {
            device.hotspot = Some(enabled);
        }
        self.send(CrossDeviceCommand::SetHotspot { id, enabled });
    }

    pub fn set_unicursor_edge(&mut self, id: DeviceId, edge: Edge) {
        if let Some(device) = self.device_mut(id) {
            device.unicursor_edge = Some(edge);
        }
        self.send(CrossDeviceCommand::SetUnicursorEdge { id, edge });
    }

    pub fn dismiss_error(&mut self) {
        self.error = None;
    }

    // --- MARK: Event folding ---

    pub(super) fn apply(&mut self, event: CrossDeviceEvent) {
        match event {
            CrossDeviceEvent::Online => self.link = Link::Online,
            CrossDeviceEvent::Offline => {
                self.link = Link::Offline;
                self.devices = Vec::new();
                self.pairing = None;
                self.close();
            }
            CrossDeviceEvent::Daemon(event) => self.apply_daemon(event),
            CrossDeviceEvent::Features { id, allowed } => {
                if let Some(device) = self.device_mut(id) {
                    device.info.features = allowed;
                    device.info.active = restrict(device.info.active, allowed);
                }
            }
            CrossDeviceEvent::PairingInvite(invite) => {
                if let Some(session) = self.pairing.as_mut() {
                    session.invite = Some(invite);
                }
            }
            CrossDeviceEvent::Tick { now_unix_ms } => self.now_unix_ms = now_unix_ms,
            CrossDeviceEvent::Error(message) => self.error = Some(message),
        }
    }

    fn apply_daemon(&mut self, event: Event) {
        match event {
            Event::DevicesChanged(proto::DevicesChanged { devices }) => {
                self.replace_devices(devices)
            }
            Event::DeviceConnected(proto::DeviceConnected {
                id,
                name,
                connected,
            }) => {
                if let Some(device) = self.device_mut(id) {
                    device.info.name = name;
                    device.info.connected = connected;
                }
            }
            Event::PairingRequest(proto::PairingRequest { id, name, code }) => {
                let session = self.pairing.get_or_insert_default();
                session.outcome = None;
                session.prompt = Some(PairingPrompt {
                    id,
                    name,
                    code,
                    answered: false,
                });
            }
            Event::PairingResult(proto::PairingResult { id, accepted }) => {
                self.finish_pairing(id, accepted);
            }
            Event::FeatureChanged(proto::FeatureChanged { id, feature, state }) => {
                if let Some(device) = self.device_mut(id) {
                    apply_feature_state(&mut device.info, feature, state);
                }
            }
            Event::BatteryChanged(proto::BatteryChanged {
                id,
                percent,
                charging,
            }) => {
                if let Some(device) = self.device_mut(id) {
                    device.info.battery = Some(proto::Battery { percent, charging });
                }
            }
            Event::HotspotChanged(proto::HotspotChanged { id, enabled }) => {
                if let Some(device) = self.device_mut(id) {
                    device.hotspot = Some(enabled);
                }
            }
            Event::FeatureFailed(proto::FeatureFailed { reason, .. }) => self.error = Some(reason),
            Event::CallStateChanged(_) | Event::MediaChanged(_) => {}
        }
    }

    /// Adopt a new list, keeping what only events told us about survivors.
    fn replace_devices(&mut self, listed: Vec<DeviceInfo>) {
        let previous = std::mem::take(&mut self.devices);
        self.devices = listed
            .into_iter()
            .map(|info| {
                let known = previous.iter().find(|device| device.info.id == info.id);
                Device {
                    hotspot: known.and_then(|device| device.hotspot),
                    unicursor_edge: known.and_then(|device| device.unicursor_edge),
                    ..Device::new(info)
                }
            })
            .collect();
        if self.selected.is_some_and(|id| self.device(id).is_none()) {
            self.close();
        }
    }

    /// Record the outcome, dropping the invitation and its pixels.
    fn finish_pairing(&mut self, id: DeviceId, accepted: bool) {
        let named = self.device(id).map(|device| device.info.name.clone());
        let Some(session) = self.pairing.as_mut() else {
            return;
        };
        let prompted = session.prompt.take().filter(|prompt| prompt.id == id);
        session.invite = None;
        session.outcome = Some(if accepted {
            PairingOutcome::Paired {
                name: prompted
                    .map(|prompt| prompt.name)
                    .or(named)
                    .unwrap_or_else(|| UNNAMED_DEVICE.to_owned()),
            }
        } else {
            PairingOutcome::Refused
        });
    }
}

fn apply_feature_state(info: &mut DeviceInfo, feature: Feature, state: FeatureState) {
    let (allowed, running) = match state {
        FeatureState::Disabled => (false, false),
        FeatureState::Enabled => (true, false),
        FeatureState::Active => (true, true),
    };
    info.features = toggled(info.features, feature, allowed);
    info.active = toggled(info.active, feature, running);
}

fn toggled(set: FeatureSet, feature: Feature, present: bool) -> FeatureSet {
    if present {
        set.with(feature)
    } else {
        set.without(feature)
    }
}

fn restrict(set: FeatureSet, allowed: FeatureSet) -> FeatureSet {
    set.iter()
        .filter(|feature| allowed.contains(*feature))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crownconnect_linux::ipc::proto::{DeviceClass, LinkKind};
    use xilem::masonry::peniko::{ImageAlphaType, ImageData};
    use xilem::tokio::sync::mpsc::unbounded_channel;
    use xilem::{Blob, ImageFormat};

    const PHONE: DeviceId = DeviceId([1; 32]);
    const TABLET: DeviceId = DeviceId([2; 32]);

    fn info(id: DeviceId, name: &str, trusted: bool) -> DeviceInfo {
        DeviceInfo {
            id,
            name: name.to_owned(),
            class: DeviceClass::Phone,
            trusted,
            connected: true,
            link: LinkKind::Lan,
            battery: None,
            features: FeatureSet::EMPTY.with(Feature::Clipboard),
            active: FeatureSet::EMPTY,
        }
    }

    fn online_with(devices: Vec<DeviceInfo>) -> CrossDeviceState {
        let mut state = CrossDeviceState::default();
        state.apply(CrossDeviceEvent::Online);
        state.apply(CrossDeviceEvent::Daemon(Event::DevicesChanged(
            proto::DevicesChanged { devices },
        )));
        state
    }

    fn invite() -> PairingInvite {
        PairingInvite {
            qr: ImageBrush::new(ImageData {
                data: Blob::from(vec![0_u8; 4]),
                format: ImageFormat::Rgba8,
                alpha_type: ImageAlphaType::Alpha,
                width: 1,
                height: 1,
            }),
            expires_unix_ms: 10_000,
        }
    }

    #[test]
    fn a_fresh_state_is_still_connecting() {
        let state = CrossDeviceState::default();
        assert_eq!(state.link, Link::Connecting);
        assert!(state.devices.is_empty() && state.pairing.is_none());
    }

    #[test]
    fn devices_split_into_paired_and_nearby() {
        let state = online_with(vec![info(PHONE, "Pixel", true), info(TABLET, "Tab", false)]);
        assert_eq!(state.trusted().count(), 1);
        assert_eq!(
            state.nearby().map(|device| device.info.id).next(),
            Some(TABLET)
        );
    }

    #[test]
    fn a_new_list_keeps_what_only_events_said() {
        let mut state = online_with(vec![info(PHONE, "Pixel", true)]);
        state.apply(CrossDeviceEvent::Daemon(Event::HotspotChanged(
            proto::HotspotChanged {
                id: PHONE,
                enabled: true,
            },
        )));
        state.apply(CrossDeviceEvent::Daemon(Event::DevicesChanged(
            proto::DevicesChanged {
                devices: vec![info(PHONE, "Pixel 9", true)],
            },
        )));
        let phone = state.device(PHONE).expect("still listed");
        assert_eq!(phone.hotspot, Some(true));
        assert_eq!(phone.info.name, "Pixel 9");
    }

    #[test]
    fn a_forgotten_device_closes_its_detail_view() {
        let mut state = online_with(vec![info(PHONE, "Pixel", true)]);
        state.open(PHONE);
        state.apply(CrossDeviceEvent::Daemon(Event::DevicesChanged(
            proto::DevicesChanged {
                devices: Vec::new(),
            },
        )));
        assert_eq!(state.selected, None);
    }

    #[test]
    fn feature_events_keep_active_a_subset_of_allowed() {
        let mut state = online_with(vec![info(PHONE, "Pixel", true)]);
        let changed = |state: FeatureState| {
            CrossDeviceEvent::Daemon(Event::FeatureChanged(proto::FeatureChanged {
                id: PHONE,
                feature: Feature::Mirror,
                state,
            }))
        };

        state.apply(changed(FeatureState::Active));
        let phone = &state.devices[0].info;
        assert!(phone.features.contains(Feature::Mirror) && phone.active.contains(Feature::Mirror));

        state.apply(changed(FeatureState::Disabled));
        let phone = &state.devices[0].info;
        assert!(
            !phone.features.contains(Feature::Mirror) && !phone.active.contains(Feature::Mirror)
        );

        state.apply(changed(FeatureState::Active));
        state.apply(CrossDeviceEvent::Features {
            id: PHONE,
            allowed: FeatureSet::EMPTY,
        });
        assert!(state.devices[0].info.active.is_empty());
    }

    #[test]
    fn toggles_are_optimistic_and_reach_the_worker() {
        let (sender, mut receiver) = unbounded_channel();
        let mut state = online_with(vec![info(PHONE, "Pixel", true)]);
        state.attach(sender);

        state.set_feature(PHONE, Feature::Clipboard, false);
        assert!(!state.devices[0].info.features.contains(Feature::Clipboard));
        assert_eq!(
            receiver.try_recv().ok(),
            Some(CrossDeviceCommand::SetFeature {
                id: PHONE,
                feature: Feature::Clipboard,
                enabled: false,
            })
        );

        state.set_hotspot(PHONE, true);
        assert_eq!(state.devices[0].hotspot, Some(true));
    }

    #[test]
    fn battery_events_land_on_their_device() {
        let mut state = online_with(vec![info(PHONE, "Pixel", true)]);
        state.apply(CrossDeviceEvent::Daemon(Event::BatteryChanged(
            proto::BatteryChanged {
                id: PHONE,
                percent: 42,
                charging: true,
            },
        )));
        assert_eq!(
            state.devices[0].info.battery,
            Some(proto::Battery {
                percent: 42,
                charging: true
            })
        );
    }

    #[test]
    fn qr_pairing_runs_from_invite_to_outcome() {
        let mut state = online_with(Vec::new());
        state.begin_pairing();
        state.apply(CrossDeviceEvent::PairingInvite(invite()));
        assert!(
            state
                .pairing
                .as_ref()
                .is_some_and(|session| session.invite.is_some())
        );

        state.apply(CrossDeviceEvent::Daemon(Event::DevicesChanged(
            proto::DevicesChanged {
                devices: vec![info(PHONE, "Pixel", true)],
            },
        )));
        state.apply(CrossDeviceEvent::Daemon(Event::PairingResult(
            proto::PairingResult {
                id: PHONE,
                accepted: true,
            },
        )));
        let session = state.pairing.as_ref().expect("the outcome stays on screen");
        assert!(session.invite.is_none(), "the QR pixels are dropped");
        assert_eq!(
            session.outcome,
            Some(PairingOutcome::Paired {
                name: "Pixel".to_owned()
            })
        );
    }

    #[test]
    fn nearby_pairing_answers_once_and_names_the_prompter() {
        let (sender, mut receiver) = unbounded_channel();
        let mut state = online_with(Vec::new());
        state.attach(sender);
        state.apply(CrossDeviceEvent::Daemon(Event::PairingRequest(
            proto::PairingRequest {
                id: TABLET,
                name: "Tab".to_owned(),
                code: "123456".to_owned(),
            },
        )));

        state.answer_pairing(true);
        state.answer_pairing(true);
        assert_eq!(
            receiver.try_recv().ok(),
            Some(CrossDeviceCommand::ConfirmPairing {
                id: TABLET,
                accept: true
            })
        );
        assert!(
            receiver.try_recv().is_err(),
            "a second press is not a second answer"
        );

        state.apply(CrossDeviceEvent::Daemon(Event::PairingResult(
            proto::PairingResult {
                id: TABLET,
                accepted: true,
            },
        )));
        assert_eq!(
            state.pairing.and_then(|session| session.outcome),
            Some(PairingOutcome::Paired {
                name: "Tab".to_owned()
            })
        );
    }

    #[test]
    fn an_invite_after_cancel_is_ignored() {
        let mut state = online_with(Vec::new());
        state.begin_pairing();
        state.end_pairing();
        state.apply(CrossDeviceEvent::PairingInvite(invite()));
        assert!(state.pairing.is_none());
    }

    #[test]
    fn the_countdown_rounds_up_to_whole_seconds() {
        let invite = invite();
        assert_eq!(invite.seconds_left(0), 10);
        assert_eq!(invite.seconds_left(9_001), 1);
        assert_eq!(invite.seconds_left(10_000), 0);
        assert_eq!(invite.seconds_left(20_000), 0);
    }

    #[test]
    fn going_offline_forgets_everything_live() {
        let mut state = online_with(vec![info(PHONE, "Pixel", true)]);
        state.begin_pairing();
        state.apply(CrossDeviceEvent::Offline);
        assert_eq!(state.link, Link::Offline);
        assert!(state.devices.is_empty() && state.pairing.is_none());
    }
}
