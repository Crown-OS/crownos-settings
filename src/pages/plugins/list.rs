//! The installed themes and plugins, one row each.

use crownui::kit::{ButtonVariant, StatusOptions, Tone, badge, icons, status, toggle, use_theme};
use crownui::prelude::{
    Cx, Runtime, Semantic, Signal, View, bind, component, for_each, keyed, vstack,
};

use super::parts::{acting, row_button, status_line, troubled};
use crate::layout::{
    Listed, glyph, hairline_unless_first, inline, listed, setting_row_content,
    setting_row_icon_desc, settings_card_titled, settings_divider, when,
};
use crate::net::plugins::{EntityInfo, Kind, PluginsState};
use crate::util::bridge::{bridge, derived};
use crate::util::theme::tint;

type Row = Signal<Listed<EntityInfo>>;

pub fn themes_card(cx: &mut Cx, state: Signal<PluginsState>) -> impl View + use<> {
    let themes = derived(cx, move |runtime| {
        state.with(runtime, |state| listed(state.of_kind(Kind::Theme).cloned()))
    });
    let stock_active = move |runtime: &mut Runtime| {
        !state.with(runtime, |state| {
            state.of_kind(Kind::Theme).any(|theme| theme.active)
        })
    };
    settings_card_titled(
        "Themes",
        (
            setting_row_icon_desc(
                glyph(icons::PALETTE),
                "Stock look",
                "CrownOS as it ships",
                activation(state, stock_active, "Use Stock look".to_owned(), |state| {
                    state.set_theme(None);
                }),
            ),
            for_each(
                themes,
                |row: &Listed<EntityInfo>| row.item.id.clone(),
                move |cx, row| theme_row(cx, state, row),
            ),
        ),
    )
}

fn theme_row(cx: &mut Cx, state: Signal<PluginsState>, row: Row) -> impl View + use<> {
    let (id, name) = row.with_untracked(cx, |row| (row.item.id.clone(), row.item.name.clone()));
    let used = id.clone();
    let opened = id.clone();
    vstack((
        settings_divider(),
        setting_row_icon_desc(
            glyph(icons::PALETTE),
            name.clone(),
            bind(move |runtime| row.with(runtime, |row| status_line(&row.item))),
            inline((
                details_button(state, name.clone(), opened),
                activation(
                    state,
                    move |runtime| row.with(runtime, |row| row.item.active),
                    format!("Use {name}"),
                    move |state| state.set_theme(Some(used.clone())),
                ),
            )),
        ),
    ))
}

pub fn plugins_card(cx: &mut Cx, state: Signal<PluginsState>) -> impl View + use<> {
    let plugins = derived(cx, move |runtime| {
        state.with(runtime, |state| {
            listed(state.of_kind(Kind::Plugin).cloned())
        })
    });
    settings_card_titled(
        "Plugins",
        (
            when(
                move |runtime| plugins.with(runtime, Vec::is_empty),
                |_| setting_row_content(status("No plugins installed").tone(Tone::Neutral)),
            ),
            for_each(
                plugins,
                |row: &Listed<EntityInfo>| row.item.id.clone(),
                move |cx, row| plugin_row(cx, state, row),
            ),
        ),
    )
}

fn plugin_row(cx: &mut Cx, state: Signal<PluginsState>, row: Row) -> impl View + use<> {
    let (id, name) = row.with_untracked(cx, |row| (row.item.id.clone(), row.item.name.clone()));
    let switched = id.clone();
    let enabled = bridge(
        cx,
        move |runtime| row.with(runtime, |row| row.item.active),
        move |runtime, enabled| {
            state.update(runtime, |state| {
                state.set_enabled(switched.clone(), enabled)
            });
        },
    );
    vstack((
        hairline_unless_first(row),
        setting_row_icon_desc(
            glyph(icons::PUZZLE),
            name.clone(),
            bind(move |runtime| row.with(runtime, |row| status_line(&row.item))),
            inline((
                when(
                    move |runtime| row.with(runtime, |row| row.item.dev),
                    |_| dev_badge(),
                ),
                when(
                    move |runtime| row.with(runtime, |row| troubled(&row.item)),
                    |_| status("Stopped").tone(Tone::Danger),
                ),
                details_button(state, name.clone(), id),
                toggle(enabled).label(format!("Enable {name}")),
            )),
        ),
    ))
}

fn details_button(state: Signal<PluginsState>, name: String, id: String) -> impl View {
    acting(
        row_button("Details…", ButtonVariant::Ghost).label(format!("Details of {name}")),
        state,
        move |state| state.open(id.clone()),
    )
}

/// "Active" while `active` holds, otherwise a button that makes it so.
fn activation(
    state: Signal<PluginsState>,
    active: impl FnMut(&mut Runtime) -> bool + 'static,
    label: String,
    activate: impl Fn(&mut PluginsState) + Clone + 'static,
) -> impl View {
    component(move |cx| {
        let theme = use_theme(cx);
        keyed(active, move |_, active| {
            if active {
                badge("Active", tint(theme, |theme| theme.status.success)).boxed()
            } else {
                acting(
                    row_button("Use", ButtonVariant::Secondary).label(label.clone()),
                    state,
                    activate.clone(),
                )
                .boxed()
            }
        })
    })
}

fn dev_badge() -> impl View {
    component(|cx| {
        let theme = use_theme(cx);
        badge("Dev", tint(theme, |theme| theme.status.warning))
    })
}
