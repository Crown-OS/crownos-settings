//! # CrownOS Settings
//! ## Layout
//!
//! The modules are stacked so that each one only knows about the one below it:
//!
//! * [`config`] — the sections, typed and self-persisting. Knows nothing about UI.
//! * [`net`] — the live system backends, one long-lived async worker each.
//!   Knows nothing about UI either, and no backend crate's types escape it.
//! * [`state`] — the two of those joined into the one `Store` a page is handed.
//! * [`layout`] — this app's page/card/row chrome, composed from `crownuikit`'s
//!   base widgets. Knows nothing about state.
//! * [`controls`] — widgets pre-wired to a config key, which is what a page names
//!   to bind one. Knows nothing about pages.
//! * [`pages`] — one module per sidebar entry, plus the registry that lists them.
//!   Knows nothing about navigation.
//! * [`sidebar`], [`app`] — navigation and the root view, the only places that
//!   know a page can be switched.
//!
//! Adding a settings page therefore means adding a module under [`pages`] and one
//! line to [`pages::GROUPS`]; adding a whole new config *section* means one line
//! in the `sections!` invocation in [`config`].

mod app;
mod config;
mod controls;
mod layout;
mod net;
mod pages;
mod sidebar;
mod state;
mod util;

use crownuikit::util::inter_font;
use winit::dpi::LogicalSize;
use winit::error::EventLoopError;
use xilem::{EventLoop, WindowOptions, Xilem};

use crate::app::AppState;

/// Wide enough for the sidebar plus a comfortable card column.
const INITIAL_SIZE: LogicalSize<f64> = LogicalSize::new(1040.0, 720.0);
/// Below this the cards start clipping, so don't let the window get there.
const MIN_SIZE: LogicalSize<f64> = LogicalSize::new(720.0, 480.0);

fn main() -> Result<(), EventLoopError> {
    let window = WindowOptions::new("Settings")
        .with_initial_inner_size(INITIAL_SIZE)
        .with_min_inner_size(MIN_SIZE);

    let app = Xilem::new_simple(AppState::load(), app::root_view, window)
        .with_font(inter_font());

    let result = app.run_in(EventLoop::with_user_event());

    // The window is gone, so nothing is going to change again — but a write
    // debounced moments before it closed is still owed to the user.
    util::persist::flush();
    result
}
