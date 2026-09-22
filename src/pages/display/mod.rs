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

use blinc_icons::icons;

use crate::controls::value_text;
use crate::layout::setting_row_content;
use crate::pages::{PageDescriptor, PageView, page};
use crate::state::Store;
use xilem::WidgetView;

pub const PAGE: PageDescriptor = PageDescriptor {
    title: "Display",
    icon: icons::MONITOR,
    build,
};

fn build(store: &Store) -> PageView {
    page(
        &PAGE,
        (
            monitors::card(store),
            selected::card(store),
            light::brightness_card(store),
            light::night_light_card(store),
        ),
    )
}

/// A full-width line of text inside a card, for status and errors.
fn status_row(text: String) -> impl WidgetView<Store> + use<> {
    setting_row_content(value_text(text))
}
