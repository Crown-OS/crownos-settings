//! The Cross-device page: everything from pairing the first device to the
//! per-device features, over the crownconnect daemon.
//!
//! One screen at a time, picked by [`screen`] in order of authority: the
//! daemon's reachability, then a pairing flow in progress, then an open
//! device, then the list — or the onboarding card when nothing is paired.

mod describe;
mod detail;
mod devices;
mod onboarding;
mod pairing;
mod parts;

use crownui::kit::{CalloutOptions, Tone, callout, icons, spinner};
use crownui::prelude::{AnyView, Cx, Signal, Styled, View, keyed};

use crate::layout::optional;
use crate::net::crownconnect::{CrossDeviceState, DeviceId, Link};
use crate::pages::{PageDescriptor, page};
use crate::state::Store;
use pairing::Stage;

pub const PAGE: PageDescriptor = PageDescriptor {
    title: "Cross-device",
    slug: "cross-device",
    icon: icons::MONITOR_SMARTPHONE,
    build,
};

/// What the page's structure depends on; everything finer is bound.
#[derive(Debug, Clone, PartialEq)]
enum Screen {
    Connecting,
    Offline,
    Pairing(Stage),
    Detail { id: DeviceId, name: String },
    Onboarding,
    Devices,
}

fn screen(state: &CrossDeviceState) -> Screen {
    match state.link {
        Link::Connecting => return Screen::Connecting,
        Link::Offline => return Screen::Offline,
        Link::Online => {}
    }
    if let Some(session) = state.pairing.as_ref() {
        return Screen::Pairing(Stage::of(session, state.now_unix_ms));
    }
    if let Some(device) = state.selected.and_then(|id| state.device(id)) {
        return Screen::Detail {
            id: device.info.id,
            name: device.info.name.clone(),
        };
    }
    if state.trusted().next().is_none() {
        return Screen::Onboarding;
    }
    Screen::Devices
}

fn build(_cx: &mut Cx, store: Store) -> AnyView {
    let state = store.cross_device;
    page(
        &PAGE,
        (
            optional(
                move |runtime| state.with(runtime, |state| state.error.clone()),
                move |_, message| parts::error_card(state, message),
            ),
            keyed(
                move |runtime| state.with(runtime, screen),
                move |cx, screen| screen_view(cx, state, screen),
            ),
        ),
    )
}

fn screen_view(cx: &mut Cx, state: Signal<CrossDeviceState>, screen: Screen) -> AnyView {
    match screen {
        Screen::Connecting => parts::quiet_card(
            spinner().size(parts::ROW_SPINNER),
            "Connecting to CrownConnect…",
        )
        .boxed(),
        Screen::Offline => offline_notice().boxed(),
        Screen::Pairing(stage) => pairing::cards(state, stage),
        Screen::Detail { id, name } => detail::cards(cx, state, id, name),
        Screen::Onboarding => onboarding::card(state).boxed(),
        Screen::Devices => devices::card(cx, state).boxed(),
    }
}

fn offline_notice() -> impl View {
    callout(Tone::Neutral, "CrownConnect is not running")
        .body("Start the crownconnect service to pair and use your devices. This page reconnects on its own.")
        .icon(icons::UNLINK)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::crownconnect::{
        Device, DeviceClass, DeviceInfo, Feature, FeatureSet, LinkKind, PairingOutcome,
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
    fn screens_follow_the_order_of_authority() {
        let mut state = CrossDeviceState::default();
        assert_eq!(screen(&state), Screen::Connecting);

        state.link = Link::Offline;
        assert_eq!(screen(&state), Screen::Offline);

        state.link = Link::Online;
        assert_eq!(screen(&state), Screen::Onboarding);

        state.devices = vec![phone()];
        assert_eq!(screen(&state), Screen::Devices);

        state.selected = Some(PHONE);
        assert_eq!(
            screen(&state),
            Screen::Detail {
                id: PHONE,
                name: "Pixel".to_owned()
            }
        );

        state.pairing = Some(PairingSession::default());
        assert_eq!(screen(&state), Screen::Pairing(Stage::Waiting));

        state.pairing = Some(PairingSession {
            prompt: Some(PairingPrompt {
                id: PHONE,
                name: "Pixel".to_owned(),
                code: "123456".to_owned(),
                answered: false,
            }),
            ..PairingSession::default()
        });
        assert!(matches!(
            screen(&state),
            Screen::Pairing(Stage::Compare { .. })
        ));

        state.pairing = Some(PairingSession {
            outcome: Some(PairingOutcome::Refused),
            ..PairingSession::default()
        });
        assert_eq!(
            screen(&state),
            Screen::Pairing(Stage::Outcome(PairingOutcome::Refused))
        );
    }
}
