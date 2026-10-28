//! The Display page: the monitors, and what to do with them.
//!
//! Split by what each card answers to, because the two halves of this page
//! come from different places and change for different reasons:
//!
//! * [`monitors`] and [`selected`] are *facts* read from the compositor over
//!   `zwlr_output_management_v1` — see [`crate::net::outputs`] for why they
//!   are not read from a RON file, and why edits accumulate as drafts and go
//!   out as one atomic configuration rather than a field at a time.
//! * [`light`] is a *preference* in `display.ron`, like every other page's
//!   controls. The compositor turns it into a gamma ramp.

mod light;
mod monitors;
mod selected;

use crownui::kit::icons;
use crownui::prelude::{AnyView, Cx, IntoProp, View};

use crate::layout::{setting_row_content, value_text};
use crate::pages::{PageDescriptor, page};
use crate::state::Store;

pub const PAGE: PageDescriptor = PageDescriptor {
    title: "Display",
    slug: "display",
    icon: icons::MONITOR,
    build,
};

fn build(cx: &mut Cx, store: Store) -> AnyView {
    page(
        &PAGE,
        (
            monitors::card(cx, store),
            selected::card(store),
            light::brightness_card(cx, store),
            light::night_light_card(cx, store),
        ),
    )
}

/// A full-width line of text inside a card, for status and errors.
fn status_row(text: impl IntoProp<String> + 'static) -> impl View {
    setting_row_content(value_text(text))
}
