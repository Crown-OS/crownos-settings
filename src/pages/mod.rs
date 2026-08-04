//! The settings pages, and the registry that drives both the sidebar and the
//! content pane.
//!
//! A page is described by a [`PageDescriptor`]: its sidebar label and icon, plus
//! a builder that turns the current [`ConfigStore`] into a view. The sidebar and
//! the router both read the same [`GROUPS`] table, so a page cannot show up in
//! one and be missing from the other — which is what the old arrangement of a
//! `Nav` enum, a hand-written sidebar list and a seven-armed `match` allowed.
//!
//! Adding a page is a module here plus one entry in [`GROUPS`].

mod appearance;
mod bluetooth;
mod display;
mod notifications;
mod power;
mod sound;
mod wifi;

use crownuikit::layouts::settings::settings_page;
use xilem::masonry::core::ArcStr;
use xilem::masonry::properties::types::AsUnit;
use xilem::view::{CrossAxisAlignment, FlexSequence, flex_col};
use xilem::{AnyWidgetView, WidgetView};

use crate::config::ConfigStore;

/// Breathing room between a page's cards.
const CARD_GAP: f64 = 24.0;

// --- MARK: Descriptor ---

/// A finished content pane, type-erased.
///
/// Every page builder composes a different concrete view type, so they are
/// unified by boxing — cheaper to read than a seven-armed `OneOf`, and
/// `Box<AnyWidgetView<_>>` is itself a `WidgetView`, so it drops straight into
/// the surrounding layout.
pub type PageView = Box<AnyWidgetView<ConfigStore>>;

/// Everything the app needs to know about one settings page.
///
/// Pages are stateless — the state is the [`ConfigStore`] — so a descriptor is a
/// plain `const` living next to the page it describes, and the app carries a
/// `&'static PageDescriptor` around as "the current page".
pub struct PageDescriptor {
    /// The sidebar label, which doubles as the page's heading.
    pub title: &'static str,
    /// Lucide-style SVG body for the sidebar icon.
    pub icon: &'static str,
    /// Builds the content pane from the current config.
    pub build: fn(&ConfigStore) -> PageView,
}

/// Every page, in sidebar order, split into the groups that get a separator
/// drawn between them.
pub static GROUPS: &[&[PageDescriptor]] = &[
    // Connectivity
    &[wifi::PAGE, bluetooth::PAGE],
    // Personalization
    &[appearance::PAGE, display::PAGE, sound::PAGE],
    // System
    &[notifications::PAGE, power::PAGE],
];

/// The page shown on startup: the first one in the sidebar.
pub fn default_page() -> &'static PageDescriptor {
    GROUPS
        .iter()
        .copied()
        .flatten()
        .next()
        .expect("GROUPS must list at least one page")
}

// --- MARK: Page frame ---

/// The frame every page shares: a heading over a stack of cards.
///
/// Factored out so a page module is only its own cards — none of them has a
/// reason to differ on spacing or alignment, and when one eventually does, this
/// is the one place that changes.
fn page<Seq>(title: impl Into<ArcStr> + 'static, cards: Seq) -> PageView
where
    Seq: FlexSequence<ConfigStore> + Send + Sync + 'static,
{
    let content = flex_col(cards)
        .gap(CARD_GAP.px())
        .cross_axis_alignment(CrossAxisAlignment::Start);

    settings_page(title, content).boxed()
}
