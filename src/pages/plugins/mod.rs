//! The Plugins & Themes page, over crownplugind — plus the per-plugin
//! settings pages the sidebar grows at runtime.
//!
//! One screen at a time, picked by [`screen`] in order of authority: the
//! daemon's reachability, then an open entity, then the lists.

mod capabilities;
mod detail;
mod entries;
mod fields;
mod install;
mod list;
mod parts;

pub use entries::{PluginPage, plugin_pages, plugin_settings_page};

use crownui::kit::{CalloutOptions, Tone, callout, icons, spinner};
use crownui::prelude::{AnyView, Cx, Signal, Styled, View, keyed};

use crate::layout::{card_stack, inline, optional, setting_row_content, settings_card, value_text};
use crate::net::plugins::{Link, PluginsState};
use crate::pages::{PageDescriptor, page};
use crate::state::Store;

const ROW_SPINNER: f32 = 14.0;

pub const PAGE: PageDescriptor = PageDescriptor {
    title: "Plugins & Themes",
    slug: "plugins",
    icon: icons::PUZZLE,
    build,
};

/// What the page's structure depends on; everything finer is bound.
#[derive(Debug, Clone, PartialEq)]
enum Screen {
    Connecting,
    Offline,
    Detail { id: String, name: String },
    Lists,
}

fn screen(state: &PluginsState) -> Screen {
    match state.link {
        Link::Connecting => return Screen::Connecting,
        Link::Offline => return Screen::Offline,
        Link::Online => {}
    }
    match state.selected.as_deref().and_then(|id| state.entity(id)) {
        Some(entity) => Screen::Detail {
            id: entity.id.clone(),
            name: entity.name.clone(),
        },
        None => Screen::Lists,
    }
}

fn build(cx: &mut Cx, store: Store) -> AnyView {
    let state = store.plugins;
    state.update(cx, PluginsState::refresh);
    page(
        &PAGE,
        (
            optional(
                move |runtime| state.with(runtime, |state| state.error.clone()),
                move |_, message| parts::error_card(state, message),
            ),
            keyed(
                move |runtime| state.with(runtime, screen),
                move |cx, screen| screen_view(cx, state, screen),
            ),
        ),
    )
}

fn screen_view(cx: &mut Cx, state: Signal<PluginsState>, screen: Screen) -> AnyView {
    match screen {
        Screen::Connecting => settings_card((setting_row_content(inline((
            spinner().size(ROW_SPINNER),
            value_text("Connecting to plugins…"),
        ))),))
        .boxed(),
        Screen::Offline => offline_notice().boxed(),
        Screen::Detail { id, name } => detail::cards(cx, state, id, name),
        Screen::Lists => card_stack((
            list::themes_card(cx, state),
            list::plugins_card(cx, state),
            install::card(cx, state),
        ))
        .boxed(),
    }
}

fn offline_notice() -> impl View {
    callout(Tone::Neutral, "Plugins aren't running")
        .body("Start the crownplugind service to manage themes and plugins. This page reconnects on its own.")
        .icon(icons::PLUG)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::plugins::{EntityInfo, EntityStatus, Kind};

    fn theme(id: &str) -> EntityInfo {
        EntityInfo {
            id: id.to_owned(),
            kind: Kind::Theme,
            name: "Nord".to_owned(),
            version: "1.0.0".to_owned(),
            author: String::new(),
            description: String::new(),
            dir: String::new(),
            system: true,
            active: false,
            dev: false,
            status: EntityStatus::Inactive,
            capabilities: Vec::new(),
            settings: None,
        }
    }

    #[test]
    fn screens_follow_the_order_of_authority() {
        let mut state = PluginsState::default();
        assert_eq!(screen(&state), Screen::Connecting);

        state.link = Link::Offline;
        assert_eq!(screen(&state), Screen::Offline);

        state.link = Link::Online;
        assert_eq!(screen(&state), Screen::Lists);

        state.selected = Some("nord".to_owned());
        assert_eq!(
            screen(&state),
            Screen::Lists,
            "a deep link waits for the list to name it"
        );

        state.entities = vec![theme("nord")];
        assert_eq!(
            screen(&state),
            Screen::Detail {
                id: "nord".to_owned(),
                name: "Nord".to_owned()
            }
        );
    }
}
