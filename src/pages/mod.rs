//! The settings pages, and the registry that drives both the sidebar and the
//! content pane.
//!
//! A page is described by a [`PageDescriptor`]: its sidebar label and icon, plus
//! a builder that turns the current [`Store`] into a view. The sidebar and
//! the router both read the same [`GROUPS`] table, so a page cannot show up in
//! one and be missing from the other — which is what the old arrangement of a
//! `Nav` enum, a hand-written sidebar list and a seven-armed `match` allowed.
//!
//! Adding a page is a module here plus one entry in [`GROUPS`].

mod appearance;
mod bluetooth;
mod cross_device;
mod display;
mod input;
mod keybinds;
mod notifications;
mod power;
mod sound;
mod wifi;

use crate::layout::settings_page;
use xilem::masonry::properties::types::AsUnit;
use xilem::view::{CrossAxisAlignment, FlexSequence, flex_col};
use xilem::{AnyWidgetView, WidgetView};

use crate::state::Store;

/// Breathing room between a page's cards.
const CARD_GAP: f64 = 24.0;

// --- MARK: Descriptor ---

/// A finished content pane, type-erased.
///
/// Every page builder composes a different concrete view type, so they are
/// unified by boxing — cheaper to read than a seven-armed `OneOf`, and
/// `Box<AnyWidgetView<_>>` is itself a `WidgetView`, so it drops straight into
/// the surrounding layout.
pub type PageView = Box<AnyWidgetView<Store>>;

/// Everything the app needs to know about one settings page.
///
/// Pages are stateless — the state is the [`Store`] — so a descriptor is a
/// plain `const` living next to the page it describes, and the app carries a
/// `&'static PageDescriptor` around as "the current page".
pub struct PageDescriptor {
    /// The sidebar label, which doubles as the page's heading.
    pub title: &'static str,
    /// Lucide-style SVG body for the sidebar icon.
    pub icon: &'static str,
    /// Builds the content pane from the current settings and system state.
    pub build: fn(&Store) -> PageView,
}

/// A run of related pages under one sidebar caption.
///
/// The label is the muted section heading `crownuikit`'s
/// `sidebar_section` prints above the group's rows.
pub struct PageGroup {
    pub label: &'static str,
    pub pages: &'static [PageDescriptor],
}

/// Every page, in sidebar order, split into the labelled sections the sidebar
/// draws them under.
pub static GROUPS: &[PageGroup] = &[
    PageGroup {
        label: "Connectivity",
        pages: &[wifi::PAGE, bluetooth::PAGE, cross_device::PAGE],
    },
    PageGroup {
        label: "Personalization",
        pages: &[appearance::PAGE, display::PAGE, sound::PAGE, input::PAGE],
    },
    PageGroup {
        label: "System",
        pages: &[keybinds::PAGE, notifications::PAGE, power::PAGE],
    },
];

/// Every page in sidebar order, groups flattened away.
fn all_pages() -> impl Iterator<Item = &'static PageDescriptor> {
    GROUPS.iter().flat_map(|group| group.pages)
}

/// The page shown on startup: the first one in the sidebar.
pub fn default_page() -> &'static PageDescriptor {
    all_pages()
        .next()
        .expect("GROUPS must list at least one page")
}

/// Where `page` sits in sidebar order, ignoring the group boundaries.
///
/// This is what gives the content pane's slide a direction: moving to a
/// higher-numbered page pushes the panes leftwards, moving back up reverses it,
/// so the motion matches the direction the eye travelled down the sidebar.
pub fn page_index(page: &'static PageDescriptor) -> usize {
    // Same identity test the sidebar uses for selection — both sides come out
    // of `GROUPS`, so pages don't need to carry a comparable id.
    all_pages()
        .position(|candidate| std::ptr::eq(candidate, page))
        .expect("every page must be registered in GROUPS")
}

/// A content pane with nothing in it.
///
/// Stands in for "no page is on its way out" in the outgoing half of the
/// [`crate::app`]'s slide transition, which always wants *some* view there.
pub fn blank_page() -> PageView {
    flex_col(()).boxed()
}

/// Whether `page` hosts the wallpaper picker.
///
/// [`crate::app`]'s navigation asks this to know when thumbnails and the
/// crownpaper daemon's preload cache are worth paying for — exactly while
/// this page is on screen.
///
/// Compared by title, not `ptr::eq`: descriptors are `const`s, so the copy
/// [`GROUPS`] holds — the one every current-page pointer comes from — lives
/// at a different address than `appearance::PAGE` itself. Titles are unique
/// in the sidebar, which makes them the identity that survives the copy.
pub fn shows_wallpapers(page: &'static PageDescriptor) -> bool {
    page.title == appearance::PAGE.title
}

// --- MARK: Page frame ---

/// The frame every page shares: an icon-and-title header over a stack of
/// cards.
///
/// Takes the whole descriptor rather than just a title so the sidebar icon
/// and the header's icon chip can never drift apart — both come from the one
/// place a page names it.
///
/// Factored out so a page module is only its own cards — none of them has a
/// reason to differ on spacing or alignment, and when one eventually does, this
/// is the one place that changes.
fn page<Seq>(desc: &'static PageDescriptor, cards: Seq) -> PageView
where
    Seq: FlexSequence<Store> + Send + Sync + 'static,
{
    let content = flex_col(cards)
        .gap(CARD_GAP.px())
        .cross_axis_alignment(CrossAxisAlignment::Start);

    settings_page(desc.title, desc.icon, content).boxed()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build every page against a real store.
    ///
    /// Running the app only exercises whichever page opens first, so the other
    /// six are only ever built when someone clicks them. Every control is bound
    /// by key, and a key that names the wrong field is a compile error — but
    /// "this page still assembles" is worth asserting somewhere that runs.
    ///
    /// Deliberately a defaults-only store rather than [`Store::load`]:
    /// `CROWN_CONFIG_DIR` is process-global and the store test in
    /// [`crate::config`] sets it, so loading here would race that — and, losing
    /// the race, would write default RON files into the developer's own
    /// `~/.config/crownos`.
    ///
    /// The defaults also stand in for a machine with no NetworkManager: the
    /// Wi-Fi half of the store starts as "we have not heard anything yet", and
    /// the Wi-Fi page has to render that as happily as it renders a full
    /// snapshot. Nothing here opens a D-Bus connection.
    #[test]
    fn every_registered_page_builds() {
        let store = Store::defaults();

        for page in all_pages() {
            let _view: PageView = (page.build)(&store);
        }

        assert_eq!(
            all_pages().count(),
            GROUPS.iter().map(|group| group.pages.len()).sum::<usize>(),
            "flattening the groups must not lose a page"
        );
    }

    /// The current-page pointer always comes out of [`GROUPS`], which holds
    /// *copies* of the page consts — so the picker test must recognize the
    /// copy, not just `appearance::PAGE`'s own address. This is exactly the
    /// bug that once left the picker spinning forever: `ptr::eq` against the
    /// original const never matched the copy navigation actually carries.
    #[test]
    fn wallpaper_page_recognized_through_groups() {
        let matches = all_pages().filter(|page| shows_wallpapers(page)).count();
        assert_eq!(matches, 1, "exactly one registered page hosts the picker");
    }
}