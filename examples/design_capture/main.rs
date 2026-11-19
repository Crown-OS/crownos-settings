//! Captures the common states of the Settings window for the CrownOS-Design
//! Figma file: each sidebar page over a believable machine, one sheet or
//! dialog per page that has one, and the notable empty state. Run both
//! appearances with
//! `../crownui/capture/scripts/capture-app.sh .`; output lands in `$CROWN_DESIGN_DIR`.
//!
//! The app is a binary, so its modules are compiled in here by path, with
//! this file standing in for `main.rs`.

#![allow(dead_code)]

#[path = "../../src/config.rs"]
mod config;
#[path = "../../src/controls/mod.rs"]
mod controls;
#[path = "../../src/layout/mod.rs"]
mod layout;
#[path = "../../src/net/mod.rs"]
mod net;
#[path = "../../src/pages/mod.rs"]
mod pages;
#[path = "../../src/sidebar.rs"]
mod sidebar;
#[path = "../../src/state.rs"]
mod state;
#[path = "../../src/util/mod.rs"]
mod util;

#[path = "../../src/app"]
mod app {
    mod navigation;
    mod root;
    mod search;
    mod theme;
    mod window;

    use crownui::prelude::{Cx, View};

    pub use navigation::Navigation;
    pub use theme::{provide_app_theme, provide_app_window_radius};
    pub use window::window_config;

    use crate::state::Store;

    /// The window's content over `store`, opened on the page at `ordinal`.
    pub fn shell(cx: &mut Cx, store: Store, ordinal: usize) -> impl View + use<> {
        root::shell(cx, store, ordinal)
    }
}

mod accounts;
mod fixtures;
mod pristine;
mod scenes;
mod session;
mod tasks;
mod wallpapers;

use capture::Carrier;

fn main() -> anyhow::Result<()> {
    accounts::isolate()?;
    let mut carrier = Carrier::new(
        "Settings",
        "The CrownOS system settings window: every sidebar page, one sheet or dialog per page that has one, and the empty Cross-device state, over a sample machine.",
    );
    scenes::capture_all(&mut carrier);
    accounts::clean_up();
    let path = carrier.write(None)?;
    println!("{}", path.display());
    capture::finish()
}
