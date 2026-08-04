//! The application state and the root view.

use xilem::WidgetView;
use xilem::core::{fork, lens};
use xilem::masonry::properties::types::AsUnit;
use xilem::view::{CrossAxisAlignment, FlexExt, flex_row};

use crate::config::ConfigStore;
use crate::pages::{self, PageDescriptor};
use crate::sidebar::sidebar_view;

/// The whole app: which page the content pane is showing, plus every config
/// section.
///
/// The two halves are deliberately separate. Navigation is this window's own
/// business and lives nowhere else; the [`ConfigStore`] is shared, file-backed
/// state, and it is all a page or a control ever gets to see.
pub struct AppState {
    page: &'static PageDescriptor,
    config: ConfigStore,
}

impl AppState {
    /// Load every section off disk and open the first page.
    pub fn load() -> Self {
        Self {
            page: pages::default_page(),
            config: ConfigStore::load(),
        }
    }

    /// Switch the content pane to `page`.
    pub fn show(&mut self, page: &'static PageDescriptor) {
        self.page = page;
    }

    /// The config half of the state, for the views that only need that half.
    fn config_mut(&mut self) -> &mut ConfigStore {
        &mut self.config
    }
}

/// Sidebar on the left, the current page's content pane on the right.
pub fn root_view(state: &mut AppState) -> impl WidgetView<AppState> + use<> {
    // Copied out so the closures below capture a plain `Copy` value rather than
    // borrowing `state`.
    let page = state.page;

    // The page builders produce views over `ConfigStore`, so they can't see —
    // and can't accidentally depend on — the navigation state. `lens` narrows
    // `AppState` down to that half on the way in, and widens control callbacks
    // back out on the way home.
    let content = lens(
        move |config: &mut ConfigStore| (page.build)(config),
        AppState::config_mut,
    );

    let root = flex_row((sidebar_view(page), content.flex(1.0)))
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .gap(0.0.px());

    // Realtime sync. The watchers draw nothing, so they ride alongside the real
    // view tree in `fork`'s second slot and live exactly as long as it does —
    // which is what makes hand-editing `~/.config/crownos/display.ron` move the
    // brightness slider while the window is open.
    fork(root, ConfigStore::watchers(AppState::config_mut))
}
