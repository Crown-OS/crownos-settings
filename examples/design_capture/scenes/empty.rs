//! The app's notable empty state: Cross-device with nothing paired yet.

use anyhow::Result;
use capture::Carrier;

use super::ordinal;
use crate::net::crownlink::Link;
use crate::session::Window;

pub fn capture(carrier: &mut Carrier) -> Result<()> {
    let mut window = Window::open(ordinal("Cross-device"), |cx, store| {
        store
            .cross_device
            .update(cx, |state| state.link = Link::Online);
    })?;
    carrier.capture_state(
        &mut window.app,
        "Empty & error states",
        "Cross-device · nothing paired",
        "CrownLink running with no device paired yet: the onboarding card",
    )?;
    Ok(())
}
