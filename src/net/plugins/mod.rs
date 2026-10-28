//! The themes and plugins backend: `crownplugind`, the only process that runs
//! them and the only writer of `plugins.ron` and every `plugin.<id>.ron`.
//!
//! The house shape (see [`crate::net`]): [`PluginsCommand`] for everything the
//! Plugins & Themes page and the plugin settings pages can ask,
//! [`PluginsEvent`] for everything the daemon can say, and [`PluginsState`] in
//! the [`Store`](crate::state::Store). The daemon's schema types are
//! re-exported rather than mirrored, like crownconnect's.

mod daemon;
mod state;
mod worker;

pub use crownplugin_model::Field;
#[cfg(test)]
pub use crownplugin_model::SelectOption;
pub use crownplugin_proto::{
    CapabilityInfo, Decision, EntityInfo, EntityStatus, Kind, Level, LogLevel, LogLine,
    SettingValue, SettingsSection, Source,
};
pub use state::{Link, PluginsState};

use crownui::prelude::{Cx, Signal};

use crate::util::worker::spawn_worker;

/// Everything a page can ask of the daemon.
#[derive(Debug, Clone, PartialEq)]
pub enum PluginsCommand {
    Refresh,
    Install(Source),
    Uninstall(String),
    SetEnabled {
        id: String,
        enabled: bool,
    },
    /// `None` goes back to the stock look.
    SetTheme(Option<String>),
    SetGrant {
        id: String,
        capability: String,
        decision: Option<Decision>,
    },
    LoadSettings(String),
    SetSetting {
        id: String,
        key: String,
        value: SettingValue,
    },
    LoadLogs(String),
    Reload(String),
}

/// Everything the worker can tell the pages.
#[derive(Debug, Clone, PartialEq)]
pub enum PluginsEvent {
    /// Connected and subscribed; the list follows.
    Online,
    /// The daemon cannot be reached. Retries are already scheduled.
    Offline,
    Listed(Vec<EntityInfo>),
    Values {
        id: String,
        values: Vec<(String, SettingValue)>,
    },
    Logs {
        id: String,
        lines: Vec<LogLine>,
    },
    Logged(LogLine),
    Installed(String),
    /// The daemon's own words, which list every validation error.
    InstallFailed(String),
    /// An operation failed. Already human-readable; shown as-is.
    Error(String),
}

/// Starts the plugins backend for as long as the calling component lives —
/// the window.
pub fn plugins_worker(cx: &mut Cx, plugins: Signal<PluginsState>) {
    let sender = spawn_worker(cx, worker::run, move |cx, event| {
        plugins.update(cx, |state| state.apply(event));
    });
    plugins.update(cx, |state| state.attach(sender));
}
