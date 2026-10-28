//! The sidebar's Plugins group: one page per enabled plugin that declares
//! settings, appended after the static pages.

use crownui::kit::{CalloutOptions, Tone, callout, icons};
use crownui::prelude::{AnyView, Cx, Signal, Styled, View, keyed};

use super::fields::fields_card;
use super::parts::error_card;
use crate::layout::{optional, paragraph, setting_row_content, settings_card, settings_page};
use crate::net::plugins::{PluginsState, SettingsSection};
use crate::state::Store;
use crate::util::lucide::lucide_icon;

/// One runtime sidebar entry.
#[derive(Debug, Clone, PartialEq)]
pub struct PluginPage {
    /// Position within the Plugins group.
    pub slot: usize,
    pub id: String,
    pub title: String,
    pub icon: &'static str,
}

pub fn plugin_pages(state: &PluginsState) -> Vec<PluginPage> {
    state
        .settings_pages()
        .enumerate()
        .map(|(slot, (entity, section))| PluginPage {
            slot,
            id: entity.id.clone(),
            title: section.title.clone(),
            icon: lucide_icon(&section.icon),
        })
        .collect()
}

/// The page in `slot` of the Plugins group, following the plugin list: it
/// rebuilds when the plugin in that slot, or what it declares, changes.
pub fn plugin_settings_page(store: Store, slot: usize) -> AnyView {
    let state = store.plugins;
    keyed(
        move |runtime| {
            state.with(runtime, |state| {
                state
                    .settings_pages()
                    .nth(slot)
                    .map(|(entity, section)| (entity.id.clone(), section.clone()))
            })
        },
        move |cx, page| match page {
            Some((id, section)) => plugin_settings(cx, state, &id, section),
            None => gone().boxed(),
        },
    )
    .flex_1()
    .boxed()
}

fn plugin_settings(
    cx: &mut Cx,
    state: Signal<PluginsState>,
    id: &str,
    section: SettingsSection,
) -> AnyView {
    state.update(cx, |state| state.load_settings(id.to_owned()));
    let icon = lucide_icon(&section.icon);
    let about = (!section.description.is_empty())
        .then(|| settings_card((setting_row_content(paragraph(section.description.clone())),)));
    let fields = fields_card(cx, state, id, &section.fields);
    let failure = optional(
        move |runtime| state.with(runtime, |state| state.error.clone()),
        move |_, message| error_card(state, message),
    );
    settings_page(section.title, icon, (failure, about, fields)).boxed()
}

fn gone() -> impl View {
    settings_page(
        "Plugin",
        icons::PUZZLE,
        (callout(
            Tone::Neutral,
            "This plugin's settings are no longer available",
        )
        .body("It was disabled, uninstalled or stopped declaring settings.")
        .icon(icons::PLUG),),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::plugins::{EntityInfo, EntityStatus, Field, Kind};

    fn entity(id: &str, kind: Kind, active: bool, icon: Option<&str>) -> EntityInfo {
        EntityInfo {
            id: id.to_owned(),
            kind,
            name: id.to_owned(),
            version: "1.0.0".to_owned(),
            author: String::new(),
            description: String::new(),
            dir: String::new(),
            system: false,
            active,
            dev: false,
            status: EntityStatus::Running,
            capabilities: Vec::new(),
            settings: icon.map(|icon| SettingsSection {
                title: format!("{id} settings"),
                icon: icon.to_owned(),
                description: String::new(),
                fields: vec![Field::Toggle {
                    key: "on".to_owned(),
                    label: "On".to_owned(),
                    description: String::new(),
                    default: true,
                }],
            }),
        }
    }

    #[test]
    fn sidebar_entries_follow_enabled_plugins_with_settings() {
        let mut state = PluginsState::default();
        state.entities = vec![
            entity("weather", Kind::Plugin, true, Some("cloud-sun")),
            entity("disabled", Kind::Plugin, false, Some("cpu")),
            entity("nord", Kind::Theme, true, Some("palette")),
            entity("plain", Kind::Plugin, true, None),
            entity("custom", Kind::Plugin, true, Some("unheard-of")),
        ];
        assert_eq!(
            plugin_pages(&state),
            [
                PluginPage {
                    slot: 0,
                    id: "weather".to_owned(),
                    title: "weather settings".to_owned(),
                    icon: icons::CLOUD_SUN,
                },
                PluginPage {
                    slot: 1,
                    id: "custom".to_owned(),
                    title: "custom settings".to_owned(),
                    icon: icons::PUZZLE,
                },
            ]
        );
    }
}
