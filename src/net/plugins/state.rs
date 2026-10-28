//! What the plugin pages read, and the pure folding of daemon events into it.

use std::collections::{BTreeMap, HashMap};

use tokio::sync::mpsc::UnboundedSender;

use super::{
    Decision, EntityInfo, Kind, LogLine, PluginsCommand, PluginsEvent, SettingValue,
    SettingsSection, Source,
};

/// How many log lines the detail view keeps; the daemon keeps its own ring.
const LOG_LINES: usize = 200;

/// Whether the daemon can be reached.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Link {
    /// The first attempt has not come back yet.
    #[default]
    Connecting,
    Online,
    Offline,
}

/// The plugins half of the app's [`Store`](crate::state::Store).
///
/// Runtime only. A fresh [`default`](Default::default) is "still connecting,
/// nothing heard", which is what the page-building test renders.
#[derive(Debug, Default)]
pub struct PluginsState {
    sender: Option<UnboundedSender<PluginsCommand>>,
    pub link: Link,
    /// Every theme and plugin, in the daemon's order.
    pub entities: Vec<EntityInfo>,
    /// Each plugin's resolved settings, by plugin id.
    pub values: HashMap<String, BTreeMap<String, SettingValue>>,
    /// The entity whose detail view is open.
    pub selected: Option<String>,
    /// The open entity's recent log, oldest first.
    pub logs: Vec<LogLine>,
    pub installing: bool,
    /// The daemon's refusal of the last install, verbatim.
    pub install_error: Option<String>,
    /// The last failed operation, until the user dismisses it.
    pub error: Option<String>,
}

impl PluginsState {
    /// Ask the worker to do something. Silent without a worker, and retires a
    /// channel whose worker is gone.
    fn send(&mut self, command: PluginsCommand) {
        let Some(sender) = self.sender.as_ref() else {
            return;
        };
        if sender.send(command).is_err() {
            self.sender = None;
        }
    }

    pub(super) fn attach(&mut self, sender: UnboundedSender<PluginsCommand>) {
        self.sender = Some(sender);
    }

    pub fn entity(&self, id: &str) -> Option<&EntityInfo> {
        self.entities.iter().find(|entity| entity.id == id)
    }

    pub fn of_kind(&self, kind: Kind) -> impl Iterator<Item = &EntityInfo> {
        self.entities
            .iter()
            .filter(move |entity| entity.kind == kind)
    }

    /// Every enabled plugin that declares a settings page, in list order —
    /// what the sidebar's Plugins group shows.
    pub fn settings_pages(&self) -> impl Iterator<Item = (&EntityInfo, &SettingsSection)> {
        self.of_kind(Kind::Plugin)
            .filter(|entity| entity.active)
            .filter_map(|entity| Some((entity, entity.settings.as_ref()?)))
    }

    /// The value `key` of plugin `id` holds, or the field's default before
    /// the daemon has said.
    pub fn value(&self, id: &str, key: &str) -> Option<SettingValue> {
        if let Some(value) = self.values.get(id).and_then(|values| values.get(key)) {
            return Some(value.clone());
        }
        let field = self.entity(id)?.settings.as_ref()?.field(key)?;
        Some(field.default_value())
    }

    /// Read the list again, for a page arriving on screen.
    pub fn refresh(&mut self) {
        self.send(PluginsCommand::Refresh);
    }

    /// Read one plugin's values again, for its settings page arriving.
    pub fn load_settings(&mut self, id: String) {
        self.send(PluginsCommand::LoadSettings(id));
    }

    pub fn open(&mut self, id: String) {
        self.logs.clear();
        self.send(PluginsCommand::LoadLogs(id.clone()));
        self.selected = Some(id);
    }

    pub fn close(&mut self) {
        self.selected = None;
        self.logs.clear();
    }

    pub fn install(&mut self, source: Source) {
        self.installing = true;
        self.install_error = None;
        self.send(PluginsCommand::Install(source));
    }

    pub fn uninstall(&mut self, id: String) {
        self.close();
        self.send(PluginsCommand::Uninstall(id));
    }

    pub fn set_enabled(&mut self, id: String, enabled: bool) {
        self.send(PluginsCommand::SetEnabled { id, enabled });
    }

    pub fn set_theme(&mut self, id: Option<String>) {
        self.send(PluginsCommand::SetTheme(id));
    }

    pub fn set_grant(&mut self, id: String, capability: String, decision: Option<Decision>) {
        self.send(PluginsCommand::SetGrant {
            id,
            capability,
            decision,
        });
    }

    pub fn reload(&mut self, id: String) {
        self.send(PluginsCommand::Reload(id));
    }

    /// Store `value` and send it on, if the field declared under `key`
    /// accepts it; the daemon would refuse anything else anyway.
    pub fn set_setting(&mut self, id: &str, key: &str, value: SettingValue) {
        let accepted = self
            .entity(id)
            .and_then(|entity| entity.settings.as_ref())
            .and_then(|section| section.field(key))
            .is_some_and(|field| field.accepts(&value));
        if !accepted || self.value(id, key).as_ref() == Some(&value) {
            return;
        }
        self.values
            .entry(id.to_owned())
            .or_default()
            .insert(key.to_owned(), value.clone());
        self.send(PluginsCommand::SetSetting {
            id: id.to_owned(),
            key: key.to_owned(),
            value,
        });
    }

    pub fn dismiss_error(&mut self) {
        self.error = None;
    }

    pub(super) fn apply(&mut self, event: PluginsEvent) {
        match event {
            PluginsEvent::Online => {
                self.link = Link::Online;
                if let Some(id) = self.selected.clone() {
                    self.send(PluginsCommand::LoadLogs(id));
                }
            }
            PluginsEvent::Offline => {
                self.link = Link::Offline;
                self.installing = false;
            }
            PluginsEvent::Listed(entities) => {
                self.values
                    .retain(|id, _| entities.iter().any(|entity| &entity.id == id));
                self.entities = entities;
            }
            PluginsEvent::Values { id, values } => {
                self.values.insert(id, values.into_iter().collect());
            }
            PluginsEvent::Logs { id, lines } => {
                if self.selected.as_ref() == Some(&id) {
                    self.logs = lines;
                    self.trim_logs();
                }
            }
            PluginsEvent::Logged(line) => {
                if self.selected.as_ref() == Some(&line.id) {
                    self.logs.push(line);
                    self.trim_logs();
                }
            }
            PluginsEvent::Installed(id) => {
                self.installing = false;
                self.open(id);
            }
            PluginsEvent::InstallFailed(message) => {
                self.installing = false;
                self.install_error = Some(message);
            }
            PluginsEvent::Error(message) => self.error = Some(message),
        }
    }

    fn trim_logs(&mut self) {
        let excess = self.logs.len().saturating_sub(LOG_LINES);
        self.logs.drain(..excess);
    }
}

#[cfg(test)]
mod tests {
    use super::super::{EntityStatus, Field, LogLevel};
    use super::*;
    use tokio::sync::mpsc::unbounded_channel;

    fn plugin(id: &str, active: bool, settings: Option<SettingsSection>) -> EntityInfo {
        EntityInfo {
            id: id.to_owned(),
            kind: Kind::Plugin,
            name: id.to_owned(),
            version: "1.0.0".to_owned(),
            author: "Crown".to_owned(),
            description: String::new(),
            dir: String::new(),
            system: false,
            active,
            dev: false,
            status: EntityStatus::Running,
            capabilities: Vec::new(),
            settings,
        }
    }

    fn section() -> SettingsSection {
        SettingsSection {
            title: "CPU".to_owned(),
            icon: "cpu".to_owned(),
            description: String::new(),
            fields: vec![Field::Slider {
                key: "interval".to_owned(),
                label: "Interval".to_owned(),
                description: String::new(),
                min: 1.0,
                max: 10.0,
                step: 1.0,
                unit: "s".to_owned(),
                default: 2.0,
            }],
        }
    }

    #[test]
    fn only_enabled_plugins_with_settings_get_a_page() {
        let mut state = PluginsState::default();
        state.apply(PluginsEvent::Listed(vec![
            plugin("cpu", true, Some(section())),
            plugin("off", false, Some(section())),
            plugin("bare", true, None),
        ]));
        let ids: Vec<_> = state
            .settings_pages()
            .map(|(entity, _)| entity.id.as_str())
            .collect();
        assert_eq!(ids, ["cpu"]);
    }

    #[test]
    fn a_value_the_field_refuses_is_never_sent() {
        let (sender, mut receiver) = unbounded_channel();
        let mut state = PluginsState::default();
        state.attach(sender);
        state.apply(PluginsEvent::Listed(vec![plugin(
            "cpu",
            true,
            Some(section()),
        )]));
        assert_eq!(
            state.value("cpu", "interval"),
            Some(SettingValue::Number(2.0))
        );

        state.set_setting("cpu", "interval", SettingValue::Number(99.0));
        state.set_setting("cpu", "interval", SettingValue::Bool(true));
        state.set_setting("cpu", "missing", SettingValue::Number(3.0));
        assert!(receiver.try_recv().is_err());

        state.set_setting("cpu", "interval", SettingValue::Number(4.0));
        assert_eq!(
            receiver.try_recv().ok(),
            Some(PluginsCommand::SetSetting {
                id: "cpu".to_owned(),
                key: "interval".to_owned(),
                value: SettingValue::Number(4.0),
            })
        );
        assert_eq!(
            state.value("cpu", "interval"),
            Some(SettingValue::Number(4.0))
        );
        state.set_setting("cpu", "interval", SettingValue::Number(4.0));
        assert!(
            receiver.try_recv().is_err(),
            "an unchanged value is dropped"
        );
    }

    #[test]
    fn only_the_open_entitys_log_is_kept() {
        let mut state = PluginsState::default();
        state.open("cpu".to_owned());
        let line = |id: &str| LogLine {
            id: id.to_owned(),
            level: LogLevel::Info,
            at: 0,
            message: "tick".to_owned(),
        };
        state.apply(PluginsEvent::Logged(line("cpu")));
        state.apply(PluginsEvent::Logged(line("other")));
        assert_eq!(state.logs.len(), 1);

        state.close();
        state.apply(PluginsEvent::Logged(line("cpu")));
        assert!(state.logs.is_empty());
    }

    #[test]
    fn an_install_refusal_is_kept_verbatim() {
        let mut state = PluginsState::default();
        state.install(Source::Path("/tmp/x".to_owned()));
        assert!(state.installing);
        state.apply(PluginsEvent::InstallFailed(
            "manifest.ron: missing `id`".to_owned(),
        ));
        assert!(!state.installing);
        assert_eq!(
            state.install_error.as_deref(),
            Some("manifest.ron: missing `id`")
        );
    }
}
