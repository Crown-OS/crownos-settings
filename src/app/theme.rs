use crownconfig::{
    SectionWatch, ThemeOverrides, ThemeSettings, load_theme_overrides, watch_section,
};
use crownos_config::schema::Appearance;
use crownui::kit::{Theme, provide_theme};
use crownui::prelude::{Cx, Signal};

use crate::state::Store;

/// Provides the theme the Appearance page is describing, to everything built
/// inside the calling component.
///
/// Derived from the store's own `appearance` section rather than from the
/// file, so flipping dark mode restyles the window the moment the switch
/// moves instead of after the debounced write lands. `theme.ron` — edited
/// elsewhere — is followed as well.
pub fn provide_app_theme(cx: &mut Cx, store: Store) -> Signal<Theme> {
    let overrides = cx.signal(load_theme_overrides());
    let initial = overrides.with_untracked(cx, Clone::clone);
    follow_overrides(
        cx,
        overrides,
        watch_section(ThemeOverrides::SECTION, initial),
    );

    let settings = move |runtime: &mut _| ThemeSettings {
        appearance: store.section(runtime, Appearance::clone),
        overrides: overrides.with(runtime, Clone::clone),
    };
    let first = cx.untrack(|runtime| Theme::from_settings(&settings(runtime)));
    let theme = provide_theme(cx, first);
    cx.effect(move |runtime| {
        let next = Theme::from_settings(&settings(runtime));
        if theme.with_untracked(runtime, |current| *current != next) {
            theme.set(runtime, next);
        }
    });
    theme
}

fn follow_overrides(
    cx: &mut Cx,
    overrides: Signal<ThemeOverrides>,
    mut watch: SectionWatch<ThemeOverrides>,
) {
    let next_edit = async move {
        let value = watch.changed().await;
        (watch, value)
    };
    cx.spawn(next_edit, move |cx, (watch, value)| {
        overrides.set(cx, value);
        follow_overrides(cx, overrides, watch);
    });
}
