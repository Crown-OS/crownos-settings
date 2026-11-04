use crownui::kit::{sidebar_with_header, slide_transition, title_bar_spacer, window_controls};
use crownui::prelude::{Cx, Styled, View, component, hstack};

use super::Navigation;
use super::search::page_search;
use super::theme::{provide_app_theme, provide_app_window_radius};
use crate::pages;
use crate::sidebar::sidebar_menus;
use crate::state::Store;

/// Sidebar on the left and, above the content pane with the current page
/// sliding in it, the window's own title bar: the page search centered between
/// two equal drag areas, the trailing one ending in the window controls.
///
/// The store, the theme, the config watchers and every backend live in this
/// component's scope, so they last exactly as long as the window.
///
/// `plugin` opens straight onto that theme or plugin's detail view, `page` onto that page.
pub fn root(plugin: Option<String>, page: Option<usize>) -> impl View {
    component(move |cx| {
        let store = Store::load(cx);
        provide_app_theme(cx, store);
        provide_app_window_radius(cx, store);
        store.start(cx);
        let ordinal = match plugin {
            Some(id) => {
                store.plugins.update(cx, |plugins| plugins.open(id));
                pages::plugins_ordinal()
            }
            None => page.unwrap_or(pages::DEFAULT_PAGE),
        };
        shell(cx, store, ordinal)
    })
}

/// Everything the window draws, over whatever store and theme it is given,
/// opened on the page at `ordinal`.
pub(super) fn shell(cx: &mut Cx, store: Store, ordinal: usize) -> impl View + use<> {
    let navigation = Navigation::new(cx, store, ordinal);

    let content = slide_transition(navigation.current(), move |cx, ordinal| {
        pages::build(cx, store, ordinal)
    })
    .flex_1();

    let menus = sidebar_menus(cx, navigation, store);
    let search = page_search(cx, navigation, store);
    let controls = hstack((title_bar_spacer(), window_controls())).flex_1();
    sidebar_with_header(menus, (title_bar_spacer(), search, controls), content)
}
