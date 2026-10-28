//! # CrownOS Settings
//!
//! The modules are stacked so that each one only knows about the one below it:
//!
//! * [`config`] — the sections, typed, reactive and self-persisting. Knows
//!   nothing about UI.
//! * [`net`] — the live system backends, one long-lived async worker each.
//!   No backend crate's types escape it.
//! * [`state`] — the two of those joined into the one `Store` a page is handed.
//! * [`layout`] — this app's page/card/row chrome, composed from the kit's
//!   widgets. Knows nothing about state.
//! * [`controls`] — kit widgets pre-wired to a config key, which is what a page
//!   names to bind one. Knows nothing about pages.
//! * [`pages`] — one module per sidebar entry, plus the registry that lists them.
//! * [`sidebar`], [`app`] — navigation and the root view, the only places that
//!   know a page can be switched.
//!
//! Adding a settings page means adding a module under [`pages`] and one line
//! to [`pages::GROUPS`]; adding a config *section* means one line in the
//! `sections!` invocation in [`config`].

mod app;
mod config;
mod controls;
mod layout;
mod net;
mod pages;
mod sidebar;
mod state;
mod util;

use anyhow::{Result, bail};
use crownui::AppConfig;
use crownui::kit::{INTER_FAMILY, inter_font};
use crownui::prelude::BLACK;

/// Wide enough for the sidebar plus a comfortable card column.
const INITIAL_SIZE: (u32, u32) = (1040, 720);
/// Below this the cards start clipping, so don't let the window get there.
const MIN_SIZE: (u32, u32) = (720, 480);

fn main() -> Result<()> {
    let plugin = plugin_argument(std::env::args().skip(1))?;
    let page = page_argument(std::env::args().skip(1)).and_then(|link| pages::ordinal_of(&link));
    let config = AppConfig::new("Settings")
        .app_id("crownsettings")
        .size(INITIAL_SIZE.0, INITIAL_SIZE.1)
        .min_size(MIN_SIZE.0, MIN_SIZE.1)
        // The palette's surfaces are translucent; what shows through them is
        // black, as it always was, rather than the toolkit's white.
        .clear_color(BLACK)
        .font(inter_font().to_vec(), Some(INTER_FAMILY));

    let result = crownui::run(app::root(plugin, page), config);

    // The window is gone, so nothing is going to change again — but a write
    // debounced moments before it closed is still owed to the user.
    util::persist::flush();
    Ok(result?)
}

const SETTINGS_URI: &str = "crown://settings/";

/// The page a positional argument names, as `<slug>` or `crown://settings/<slug>`.
fn page_argument(mut args: impl Iterator<Item = String>) -> Option<String> {
    while let Some(arg) = args.next() {
        if arg == "--plugin" {
            args.next();
            continue;
        }
        if !arg.starts_with("--") {
            return Some(arg.strip_prefix(SETTINGS_URI).unwrap_or(&arg).to_owned());
        }
    }
    None
}

/// The id after `--plugin`, the deep link into that plugin's detail view.
fn plugin_argument(mut args: impl Iterator<Item = String>) -> Result<Option<String>> {
    while let Some(arg) = args.next() {
        if let Some(id) = arg.strip_prefix("--plugin=") {
            return Ok(Some(id.to_owned()));
        }
        if arg == "--plugin" {
            let Some(id) = args.next() else {
                bail!("--plugin needs a plugin id");
            };
            return Ok(Some(id));
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parsed(args: &[&str]) -> Option<String> {
        plugin_argument(args.iter().map(|arg| (*arg).to_owned())).unwrap_or_default()
    }

    #[test]
    fn the_plugin_deep_link_is_read_in_both_spellings() {
        assert_eq!(parsed(&[]), None);
        assert_eq!(parsed(&["--plugin", "cpu"]), Some("cpu".to_owned()));
        assert_eq!(parsed(&["--plugin=cpu"]), Some("cpu".to_owned()));
        assert!(plugin_argument(["--plugin".to_owned()].into_iter()).is_err());
    }

    #[test]
    fn the_page_deep_link_is_read_as_slug_or_uri() {
        let page = |args: &[&str]| page_argument(args.iter().map(|arg| (*arg).to_owned()));
        assert_eq!(page(&[]), None);
        assert_eq!(page(&["screenshot"]), Some("screenshot".to_owned()));
        assert_eq!(
            page(&["crown://settings/sound/input"]),
            Some("sound/input".to_owned())
        );
        assert_eq!(page(&["--plugin", "cpu"]), None);
        assert_eq!(pages::ordinal_of("sound/input"), pages::ordinal_of("sound"));
        assert!(pages::ordinal_of("screenshot").is_some());
        assert_eq!(pages::ordinal_of("nonsense"), None);
    }
}
