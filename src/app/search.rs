use crownui::kit::{SearchOptions, Suggestion, TextInputOptions, sidebar_search, suggestion};
use crownui::prelude::{Cx, Listeners, Runtime, Signal, View};

use super::Navigation;
use crate::pages::{GROUPS, plugin_ordinal, plugin_pages};
use crate::sidebar::PLUGINS_LABEL;
use crate::state::Store;
use crate::util::bridge::derived;

const PLACEHOLDER: &str = "Search settings";

struct PageMatch {
    ordinal: usize,
    suggestion: Suggestion,
}

/// The header's search bar: offers every page whose title or slug holds the
/// query, and opens the one picked — or the first match on Enter.
pub fn page_search(cx: &mut Cx, navigation: Navigation, store: Store) -> impl View + use<> {
    let query = cx.signal(String::new());
    let suggestions = derived(cx, move |runtime| {
        matches(runtime, store, query)
            .into_iter()
            .map(|found| found.suggestion)
            .collect()
    });
    let open = move |cx: &mut Cx, index: usize| {
        let ordinal = cx.untrack(|runtime| {
            matches(runtime, store, query)
                .get(index)
                .map(|found| found.ordinal)
        });
        if let Some(ordinal) = ordinal {
            query.set(cx, String::new());
            navigation.show(cx, ordinal);
        }
    };
    sidebar_search(query)
        .placeholder(PLACEHOLDER)
        .suggestions(suggestions)
        .on_select(open)
        .on_submit(move |cx, _| open(cx, 0))
}

fn matches(runtime: &mut Runtime, store: Store, query: Signal<String>) -> Vec<PageMatch> {
    let needle = query.with(runtime, |query| query.trim().to_lowercase());
    if needle.is_empty() {
        return Vec::new();
    }
    let holds = |text: &str| text.to_lowercase().contains(&needle);
    let static_pages = GROUPS
        .iter()
        .flat_map(|group| group.pages.iter().map(move |page| (group.label, page)))
        .enumerate()
        .filter(|(_, (_, page))| holds(page.title) || holds(page.slug))
        .map(|(ordinal, (label, page))| PageMatch {
            ordinal,
            suggestion: suggestion(page.title).icon(page.icon).detail(label),
        });
    let plugins = store.plugins.with(runtime, plugin_pages);
    let plugin_pages = plugins
        .into_iter()
        .filter(|page| holds(&page.title))
        .map(|page| PageMatch {
            ordinal: plugin_ordinal(page.slot),
            suggestion: suggestion(page.title).icon(page.icon).detail(PLUGINS_LABEL),
        });
    static_pages.chain(plugin_pages).collect()
}
