//! The sidebar's menu column, built from the page registry.
//!
//! This module knows how the sidebar *looks*; [`crate::pages::GROUPS`] knows
//! what goes in it. Nothing here names an individual page, so a new page appears
//! in the sidebar as soon as it's registered. The chrome — the fixed column and
//! the rounded content pane beside it — is the kit's `sidebar`.
//!
//! After the static groups comes the Plugins group, listed from the live
//! plugin list: one row per enabled plugin that declares settings.

use crownui::kit::{sidebar_item, sidebar_section};
use crownui::prelude::{AnyView, Cx, Listeners, Runtime, Signal, View, bind, for_each};

use crate::app::Navigation;
use crate::layout::when;
use crate::pages::{GROUPS, PluginPage, plugin_ordinal, plugin_pages};
use crate::state::Store;
use crate::util::bridge::derived;

const PLUGINS_LABEL: &str = "Plugins";

/// One labelled section per registry group, then the Plugins group.
pub fn sidebar_menus(cx: &mut Cx, navigation: Navigation, store: Store) -> Vec<AnyView> {
    let current = navigation.current();
    let mut ordinal = 0;
    let mut menus: Vec<AnyView> = GROUPS
        .iter()
        .map(|group| {
            let items: Vec<_> = group
                .pages
                .iter()
                .map(|page| {
                    let index = ordinal;
                    ordinal += 1;
                    sidebar_item(
                        page.title,
                        page.icon,
                        bind(move |runtime| current.get(runtime) == index),
                    )
                    .on_click(move |cx, _| navigation.show(cx, index))
                })
                .collect();
            sidebar_section(group.label, items).boxed()
        })
        .collect();
    menus.push(plugin_menu(cx, navigation, store).boxed());
    menus
}

fn plugin_menu(cx: &mut Cx, navigation: Navigation, store: Store) -> impl View + use<> {
    let plugins = store.plugins;
    let entries = derived(cx, move |runtime| plugins.with(runtime, plugin_pages));
    when(
        move |runtime| entries.with(runtime, |entries| !entries.is_empty()),
        move |_| {
            sidebar_section(
                PLUGINS_LABEL,
                for_each(
                    entries,
                    |entry: &PluginPage| (entry.id.clone(), entry.icon),
                    move |cx, entry| plugin_item(cx, navigation, entry),
                ),
            )
        },
    )
}

fn plugin_item(
    cx: &mut Cx,
    navigation: Navigation,
    entry: Signal<PluginPage>,
) -> impl View + use<> {
    let current = navigation.current();
    let icon = entry.with_untracked(cx, |entry| entry.icon);
    let ordinal =
        move |runtime: &mut Runtime| plugin_ordinal(entry.with(runtime, |entry| entry.slot));
    sidebar_item(
        bind(move |runtime| entry.with(runtime, |entry| entry.title.clone())),
        icon,
        bind(move |runtime| current.get(runtime) == ordinal(runtime)),
    )
    .on_click(move |cx, _| {
        let target = cx.untrack(ordinal);
        navigation.show(cx, target);
    })
}
