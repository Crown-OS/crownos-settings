//! The common states, in the order the Figma page lays them out: each
//! sidebar page once, one sheet or dialog for the pages that have one, and
//! the app's notable empty state.

mod dialogs;
mod empty;
mod pages;

use anyhow::Result;
use capture::Carrier;
use crowntest::{Query, Role, role};

use crate::pages as settings_pages;
use crate::session::{Area, Setup, Window};

pub const LOCK_SCREEN: &str = "Lock Screen";
pub const PERFORMANCE: &str = "Performance";
/// The lock screen preview while the page is at its top.
pub const PREVIEW_PROBE: Area = Area {
    left: 320,
    top: 120,
    right: 980,
    bottom: 528,
};

type Scenes = fn(&mut Carrier) -> Result<()>;

/// Every group; one that fails is reported and the rest still captured.
pub fn capture_all(carrier: &mut Carrier) {
    let groups: [(&str, Scenes); 3] = [
        ("sidebar pages", pages::capture),
        ("dialogs", dialogs::capture),
        ("empty", empty::capture),
    ];
    let only = std::env::var("DESIGN_CAPTURE_ONLY").ok();
    for (name, scenes) in groups {
        if only.as_deref().is_some_and(|only| only != name) {
            continue;
        }
        if let Err(error) = scenes(carrier) {
            eprintln!("design_capture: {name} stopped: {error:#}");
        }
    }
}

pub fn ordinal(title: &str) -> usize {
    settings_pages::all_pages()
        .position(|page| page.title == title)
        .unwrap_or(settings_pages::DEFAULT_PAGE)
}

pub fn page_view(title: &str) -> Query {
    role(Role::ScrollView).name(title)
}

/// The page titled `title` over `setup`, with every backend answered the way
/// that page needs before it is captured.
pub fn open_page(title: &str, setup: impl Setup) -> Result<Window> {
    if title == PERFORMANCE {
        let mut window = Window::held(ordinal(title), setup)?;
        window.answer(&pages::SAMPLE_MEMORY);
        return Ok(window);
    }
    let mut window = Window::open(ordinal(title), setup)?;
    if title == LOCK_SCREEN {
        window.await_painter(PREVIEW_PROBE);
    }
    Ok(window)
}
