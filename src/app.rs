//! The application state and the root view.

use crownos_config::schema::Appearance;
use crownuikit::config::{Theme, apply_appearance, set_theme_immediately, theme, theme_ticker};
use crownuikit::layouts::sidebar::sidebar;
use crownuikit::layouts::transition::slide_transition;
use xilem::WidgetView;
use xilem::core::{fork, lens};
use xilem::style::Style;
use xilem::view::sized_box;

use crate::config::ConfigStore;
use crate::net::outputs::{OutputState, output_worker};
use crate::net::wallpaper::{WallpaperState, wallpaper_worker};
use crate::net::wifi::{WifiState, wifi_worker};
use crate::pages::{self, PageDescriptor};
use crate::sidebar::sidebar_menus;
use crate::state::Store;

/// The whole app: which page the content pane is showing, plus everything the
/// pages are allowed to see.
///
/// The two halves are deliberately separate. Navigation is this window's own
/// business and lives nowhere else; the [`Store`] is the settings on disk plus
/// the network as it actually is, and it is all a page or a control ever gets
/// to see.
pub struct AppState {
    page: &'static PageDescriptor,
    /// The page currently sliding out of the content pane, if any.
    ///
    /// Navigation is instant as far as [`page`](Self::page) is concerned — this
    /// is purely the outgoing pane's lifetime. The slide container animates it
    /// off-screen and then says so, which is the only thing that clears this.
    outgoing: Option<&'static PageDescriptor>,
    store: Store,
}

impl AppState {
    /// Load every section off disk and open the first page.
    pub fn load() -> Self {
        let store = Store::load();

        // Install the palette without a cross-fade, so the window opens in the
        // configured theme instead of fading into it from the kit's default on
        // the first frame. Every *later* change goes through
        // [`apply_appearance`] in `root_view`, which does fade.
        set_theme_immediately(Theme::from_appearance(store.section::<Appearance>()));

        let mut app = Self {
            page: pages::default_page(),
            outgoing: None,
            store,
        };
        // Should the startup page ever be (or become) the one with the
        // wallpaper picker, its thumbnails are owed from the first frame —
        // the worker replays this the moment it attaches.
        if pages::shows_wallpapers(app.page) {
            app.store.wallpaper.enter();
        }
        app
    }

    /// Switch the content pane to `page`, sliding the current one out.
    pub fn show(&mut self, page: &'static PageDescriptor) {
        // Re-clicking the open page is not a navigation, and starting a slide
        // for it would flash the pane out and back for no reason.
        if std::ptr::eq(self.page, page) {
            return;
        }
        let had_wallpapers = self.wallpapers_on_screen();
        self.outgoing = Some(self.page);
        self.page = page;
        self.sync_wallpapers(had_wallpapers);
    }

    /// The slide has finished, so the outgoing page can stop being built.
    fn slide_settled(&mut self) {
        let had_wallpapers = self.wallpapers_on_screen();
        self.outgoing = None;
        self.sync_wallpapers(had_wallpapers);
    }

    /// Whether the wallpaper picker is in either pane — including the one
    /// sliding out, which is still being built and still visible.
    fn wallpapers_on_screen(&self) -> bool {
        pages::shows_wallpapers(self.page)
            || self.outgoing.is_some_and(pages::shows_wallpapers)
    }

    /// Start or stop paying for wallpaper thumbnails, on the edges only.
    ///
    /// The picker's lifetime cannot follow its *view*: navigating away
    /// rebuilds the Appearance page once more in the outgoing pane and tears
    /// it down again when the slide settles, so a view-scoped worker would
    /// churn load→drop→load on every navigation. Instead the two places that
    /// change what is on screen — [`show`](Self::show) and
    /// [`slide_settled`](Self::slide_settled) — compare before with after:
    /// entering the picker loads thumbnails and warms the daemon's cache once,
    /// and only the settle that truly removes it (or a mid-slide navigation
    /// that drops it from the outgoing pane early) releases them.
    fn sync_wallpapers(&mut self, had_wallpapers: bool) {
        let has_wallpapers = self.wallpapers_on_screen();
        if has_wallpapers && !had_wallpapers {
            self.store.wallpaper.enter();
        }
        if !has_wallpapers && had_wallpapers {
            self.store.wallpaper.leave();
        }
    }

    /// Everything a page may see, for the views that only need that.
    fn store_mut(&mut self) -> &mut Store {
        &mut self.store
    }

    /// The config half, for the filesystem watchers.
    fn config_mut(&mut self) -> &mut ConfigStore {
        self.store.config_mut()
    }

    /// The Wi-Fi half, for the backend worker.
    fn wifi_mut(&mut self) -> &mut WifiState {
        &mut self.store.wifi
    }

    /// The wallpaper half, for its backend worker.
    fn wallpaper_mut(&mut self) -> &mut WallpaperState {
        &mut self.store.wallpaper
    }

    /// The monitors half, for its backend worker.
    fn outputs_mut(&mut self) -> &mut OutputState {
        &mut self.store.outputs
    }

    /// Nothing to mirror: the compositor persists the arrangement itself, so
    /// copying it into a RON file here would be a second source of truth.
    fn observe_outputs(_state: &mut Self, _event: &crate::net::outputs::OutputEvent) {}

    /// Copy what NetworkManager reports back into `wifi.ron`.
    ///
    /// Handed to [`wifi_worker`] as its second projection: the worker owns the
    /// runtime state, this window owns the decision to publish it to the rest
    /// of CrownOS. See [`Store::mirror_wifi`].
    fn mirror_wifi(&mut self, event: &crate::net::wifi::WifiEvent) {
        self.store.mirror_wifi(event);
    }

    /// Copy what crownpaper reports back into `appearance.ron` — the same
    /// arrangement as [`mirror_wifi`](Self::mirror_wifi). See
    /// [`Store::mirror_wallpaper`].
    fn mirror_wallpaper(&mut self, event: &crate::net::wallpaper::WallpaperEvent) {
        self.store.mirror_wallpaper(event);
    }
}

/// Sidebar on the left, the current page's content pane on the right.
pub fn root_view(state: &mut AppState) -> impl WidgetView<AppState> + use<> {
    // Install the palette before a single child view is built, so everything
    // below reads the theme the Appearance page is currently describing. This
    // covers both directions at once: a control in this window writing the
    // section, and the watcher below adopting somebody else's write. Cheap and
    // idempotent when nothing changed — `apply_appearance` drops a no-op — so
    // running it on every rebuild costs nothing and cannot go stale.
    //
    // `crownuikit::config::themed` would be the other way to do this, but it
    // opens a second subscription to a file this app already watches.
    apply_appearance(state.store.section::<Appearance>());

    // Copied out so the closures below capture plain `Copy` values rather than
    // borrowing `state`.
    let page = state.page;
    let outgoing = state.outgoing;

    // The page builders produce views over `Store`, so they can't see — and
    // can't accidentally depend on — the navigation state. `lens` narrows
    // `AppState` down to that half on the way in, and widens control callbacks
    // back out on the way home.
    let incoming_pane = lens(
        move |store: &mut Store| (page.build)(store),
        AppState::store_mut,
    );
    let outgoing_pane = lens(
        move |store: &mut Store| match outgoing {
            Some(previous) => (previous.build)(store),
            None => pages::blank_page(),
        },
        AppState::store_mut,
    );

    // The slide has to sit out here rather than inside the lens: it reports back
    // when the outgoing pane is off-screen, and only `AppState` — not the
    // `Store` half — can act on that.
    let content = slide_transition(
        pages::page_index(page),
        incoming_pane,
        outgoing_pane,
        AppState::slide_settled,
    );

    // The kit's `sidebar` owns the whole two-pane arrangement now: the fixed
    // menu column on the left and the rounded, bordered content pane the slide
    // plays out in. The `sized_box` behind it exists because the layout keeps a
    // gutter around both panes (and the light palette's sidebar fill is
    // transparent) — without a painted root the window's black base color
    // would show through there.
    let root = sized_box(sidebar(sidebar_menus(page), content))
        .expand()
        .background_color(theme().surface.bg);

    // Realtime sync, the driver for theme cross-fades, and the two system
    // backends. None of these draws anything, so they ride alongside the real
    // view tree in `fork`'s second slot and live exactly as long as it does —
    // which is what makes hand-editing `~/.config/crownos/display.ron` move the
    // brightness slider while the window is open, what makes flipping dark mode
    // fade the window over rather than blink it, and what ties the D-Bus and
    // crownpaper connections' lifetimes to the window's. The wallpaper worker
    // in particular must sit here and not in the Appearance page: the page is
    // rebuilt in both panes of every slide transition, and a worker that came
    // and went with it would reconnect and re-decode on every navigation.
    fork(
        root,
        (
            ConfigStore::watchers(AppState::config_mut),
            theme_ticker::<AppState>(),
            wifi_worker(AppState::wifi_mut, AppState::mirror_wifi),
            wallpaper_worker(AppState::wallpaper_mut, AppState::mirror_wallpaper),
            output_worker(AppState::outputs_mut, AppState::observe_outputs),
        ),
    )
}
