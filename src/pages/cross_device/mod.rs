//! The Cross-device page: everything from pairing the first device to the
//! per-device features, over the crownconnect daemon.
//!
//! One screen at a time, picked by [`build`] in order of authority: the
//! daemon's reachability, then a pairing flow in progress, then an open
//! device, then the list — or the onboarding card when nothing is paired.

mod describe;
mod detail;
mod devices;
mod onboarding;
mod pairing;
mod parts;

use blinc_icons::icons;
use crownuikit::widgets::{Tone, callout, spinner};
use xilem::WidgetView;

use crate::net::crownconnect::{CrossDeviceState, Link};
use crate::pages::{PageDescriptor, PageView, page};
use crate::state::Store;

pub const PAGE: PageDescriptor = PageDescriptor {
    title: "Cross-device",
    icon: icons::MONITOR_SMARTPHONE,
    build,
};

fn build(store: &Store) -> PageView {
    let cross_device = &store.cross_device;
    let mut cards: Vec<PageView> = cross_device
        .error
        .clone()
        .map(|message| parts::error_card(message).boxed())
        .into_iter()
        .collect();
    cards.extend(screen(cross_device));
    page(&PAGE, cards)
}

fn screen(state: &CrossDeviceState) -> Vec<PageView> {
    match state.link {
        Link::Connecting => {
            return vec![
                parts::quiet_card(
                    spinner().size(parts::ROW_SPINNER),
                    "Connecting to CrownConnect…",
                )
                .boxed(),
            ];
        }
        Link::Offline => return vec![offline_notice().boxed()],
        Link::Online => {}
    }
    if let Some(session) = state.pairing.as_ref() {
        return pairing::cards(state, session);
    }
    if let Some(device) = state.selected.and_then(|id| state.device(id)) {
        return detail::cards(state, device);
    }
    if state.trusted().next().is_none() {
        return vec![onboarding::card().boxed()];
    }
    vec![devices::card(state).boxed()]
}

fn offline_notice() -> impl WidgetView<Store> {
    callout(Tone::Neutral, "CrownConnect is not running")
        .body("Start the crownconnect service to pair and use your devices. This page reconnects on its own.")
        .icon(icons::UNLINK)
        .view()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::crownconnect::{
        Device, DeviceClass, DeviceId, DeviceInfo, Feature, FeatureSet, LinkKind, PairingOutcome,
        PairingPrompt, PairingSession,
    };

    const PHONE: DeviceId = DeviceId([7; 32]);

    fn phone() -> Device {
        Device {
            info: DeviceInfo {
                id: PHONE,
                name: "Pixel".to_owned(),
                class: DeviceClass::Phone,
                trusted: true,
                connected: true,
                link: LinkKind::Usb,
                battery: None,
                features: FeatureSet::EMPTY
                    .with(Feature::Hotspot)
                    .with(Feature::Unicursor),
                active: FeatureSet::EMPTY,
            },
            hotspot: Some(true),
            unicursor_edge: None,
        }
    }

    #[test]
    fn every_screen_builds() {
        let mut store = Store::defaults();
        let mut build_now = |edit: &dyn Fn(&mut CrossDeviceState)| {
            edit(&mut store.cross_device);
            let _view: PageView = build(&store);
        };

        build_now(&|state| state.link = Link::Offline);
        build_now(&|state| {
            state.link = Link::Online;
            state.error = Some("Could not start pairing".to_owned());
        });
        build_now(&|state| state.pairing = Some(PairingSession::default()));
        build_now(&|state| {
            state.pairing = Some(PairingSession {
                prompt: Some(PairingPrompt {
                    id: PHONE,
                    name: "Pixel".to_owned(),
                    code: "123456".to_owned(),
                    answered: false,
                }),
                ..PairingSession::default()
            });
        });
        build_now(&|state| {
            state.pairing = Some(PairingSession {
                outcome: Some(PairingOutcome::Refused),
                ..PairingSession::default()
            });
        });
        build_now(&|state| {
            state.pairing = None;
            state.devices = vec![phone()];
        });
        build_now(&|state| state.selected = Some(PHONE));
        build_now(&|state| state.confirming_forget = true);
    }
}
