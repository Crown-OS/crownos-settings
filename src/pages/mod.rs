//! The settings pages, and the registry that drives both the sidebar and the
//! content pane.
//!
//! A page is described by a [`PageDescriptor`]: its sidebar label and icon, plus
//! a builder that turns the [`Store`] into a view. The sidebar and the router
//! both read the same [`GROUPS`] table, so a page cannot show up in one and be
//! missing from the other.
//!
//! Adding a page is a module here plus one entry in [`GROUPS`]. Ordinals past
//! the registered pages belong to the plugin settings pages, which the sidebar
//! lists from the live plugin list.

mod appearance;
mod bluetooth;
mod cross_device;
mod display;
mod input;
mod keybinds;
mod lock_screen;
mod notifications;
mod plugins;
mod power;
mod screenshot;
mod sound;
mod wifi;

use crownui::prelude::{AnyView, Cx, View, ViewSeq};

use crate::layout::settings_page;
use crate::state::Store;

pub use plugins::{PluginPage, plugin_pages};

// --- MARK: Descriptor ---

/// Builds a page's content pane. Runs once each time the page slides in;
/// everything inside follows the store through bindings from then on.
pub type Build = fn(&mut Cx, Store) -> AnyView;

/// Everything the app needs to know about one settings page.
pub struct PageDescriptor {
    /// The sidebar label, which doubles as the page's heading.
    pub title: &'static str,
    /// The name other apps open the page by: `crownsettings <slug>` or
    /// `crown://settings/<slug>`.
    pub slug: &'static str,
    /// Lucide-style SVG body for the sidebar icon.
    pub icon: &'static str,
    pub build: Build,
}

/// A run of related pages under one sidebar caption.
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
        pages: &[
            appearance::PAGE,
            lock_screen::PAGE,
            plugins::PAGE,
            display::PAGE,
            sound::PAGE,
            input::PAGE,
        ],
    },
    PageGroup {
        label: "System",
        pages: &[
            keybinds::PAGE,
            notifications::PAGE,
            screenshot::PAGE,
            power::PAGE,
        ],
    },
];

/// The page shown on startup: the first one in the sidebar.
pub const DEFAULT_PAGE: usize = 0;

/// Every page in sidebar order, groups flattened away. A page's position here
/// is its ordinal, which is also what gives the content pane's slide a
/// direction.
pub fn all_pages() -> impl Iterator<Item = &'static PageDescriptor> {
    GROUPS.iter().flat_map(|group| group.pages)
}

/// How many pages [`GROUPS`] registers; the runtime plugin pages follow.
pub fn static_page_count() -> usize {
    all_pages().count()
}

/// The ordinal of the plugin page in `slot` of the sidebar's Plugins group.
pub fn plugin_ordinal(slot: usize) -> usize {
    static_page_count() + slot
}

/// The ordinal of the Plugins & Themes page.
pub fn plugins_ordinal() -> usize {
    all_pages()
        .position(|page| page.title == plugins::PAGE.title)
        .unwrap_or(DEFAULT_PAGE)
}

/// The ordinal of the page a deep link names, ignoring any sub-path after the slug.
pub fn ordinal_of(link: &str) -> Option<usize> {
    let slug = link.split('/').next()?;
    all_pages().position(|page| page.slug == slug)
}

/// Builds the page at `ordinal`: a registered page, or a plugin's settings.
pub fn build(cx: &mut Cx, store: Store, ordinal: usize) -> AnyView {
    match all_pages().nth(ordinal) {
        Some(page) => (page.build)(cx, store),
        None => plugins::plugin_settings_page(store, ordinal - static_page_count()),
    }
}

/// Whether the page at `ordinal` hosts the wallpaper picker, so navigation
/// knows when thumbnails and the daemon's preload cache are worth paying for.
pub fn shows_wallpapers(ordinal: usize) -> bool {
    all_pages()
        .nth(ordinal)
        .is_some_and(|page| page.title == appearance::PAGE.title)
}

// --- MARK: Page frame ---

/// The frame every page shares, from the page's own descriptor so the sidebar
/// icon and the header's icon chip can never drift apart.
fn page(desc: &'static PageDescriptor, cards: impl ViewSeq + 'static) -> AnyView {
    settings_page(desc.title, desc.icon, cards).boxed()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exactly_one_registered_page_hosts_the_picker() {
        let ordinals = (0..all_pages().count()).filter(|ordinal| shows_wallpapers(*ordinal));
        assert_eq!(ordinals.count(), 1);
    }

    #[test]
    fn flattening_the_groups_loses_no_page() {
        assert_eq!(
            all_pages().count(),
            GROUPS.iter().map(|group| group.pages.len()).sum::<usize>()
        );
    }
}
