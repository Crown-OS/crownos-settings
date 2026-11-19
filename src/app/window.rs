use crownui::AppConfig;
use crownui::kit::{INTER_FAMILY, inter_font};
use crownui::prelude::BLACK;

/// Wide enough for the sidebar plus a comfortable card column.
const INITIAL_SIZE: (u32, u32) = (1040, 720);
/// Below this the cards start clipping, so don't let the window get there.
const MIN_SIZE: (u32, u32) = (720, 480);

/// The window the app opens: its size, fonts and what shows through it.
pub fn window_config() -> AppConfig {
    AppConfig::new("Settings")
        .app_id("crownsettings")
        .size(INITIAL_SIZE.0, INITIAL_SIZE.1)
        .min_size(MIN_SIZE.0, MIN_SIZE.1)
        // The palette's surfaces are translucent; what shows through them is
        // black, as it always was, rather than the toolkit's white.
        .clear_color(BLACK)
        .font(inter_font().to_vec(), Some(INTER_FAMILY))
}
