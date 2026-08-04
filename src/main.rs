//! # CrownOS Settings
//!
//! A macOS-System-Settings-shaped desktop app: a fixed left sidebar of menu
//! items and a scrollable right pane showing the selected page's grouped
//! setting cards.
//!
//! Every control writes straight through to [`crownconfig`], so the RON files
//! in `~/.config/crownos` *are* the app's state — there is no in-app "Apply" step. The
//! reverse direction works too: a watcher per section lives alongside the view
//! tree, so hand-editing `~/.config/crownos/display.ron` moves the brightness slider
//! while the window is open.
//!
//! ## Layout
//!
//! The modules are stacked so that each one only knows about the one below it:
//!
//! * [`config`] — the sections, typed and self-persisting. Knows nothing about UI.
//! * [`controls`] — widgets pre-wired to a section field. Knows nothing about
//!   pages.
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
mod pages;
mod sidebar;

use crownuikit::util::INTER_FONT_DATA;
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
        .with_font(INTER_FONT_DATA.to_vec());

    app.run_in(EventLoop::with_user_event())
}
