use crownui::kit::{sidebar, slide_transition, use_theme};
use crownui::prelude::{Curve, Cx, Styled, View, component, vstack, zstack};

use super::Navigation;
use super::theme::provide_app_theme;
use crate::pages;
use crate::sidebar::sidebar_menus;
use crate::state::Store;
use crate::util::theme::tint;

/// The kit's palette cross-fade, so the window backdrop changes with the cards.
const THEME_FADE_MILLIS: u64 = 240;

/// Sidebar on the left, the current page sliding in the content pane on the
/// right, over a backdrop in the theme's surface color.
///
/// The store, the theme, the config watchers and every backend live in this
/// component's scope, so they last exactly as long as the window.
///
/// `plugin` opens straight onto that theme or plugin's detail view, `page` onto that page.
pub fn root(plugin: Option<String>, page: Option<usize>) -> impl View {
    component(move |cx| {
        let store = Store::load(cx);
        provide_app_theme(cx, store);
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
    let theme = use_theme(cx);
    let navigation = Navigation::new(cx, store, ordinal);

    let content = slide_transition(navigation.current(), move |cx, ordinal| {
        pages::build(cx, store, ordinal)
    })
    .flex_1();
    let backdrop = vstack(())
        .w_full()
        .h_full()
        .bg(tint(theme, |theme| theme.surface.bg))
        .transition(Curve::ease_in_out(THEME_FADE_MILLIS));

    let menus = sidebar_menus(cx, navigation, store);
    zstack((backdrop, sidebar(menus, content)))
}
