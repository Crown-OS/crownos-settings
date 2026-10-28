//! The Appearance page: the theme, and the wallpaper picker.

mod edge_glow;
mod gallery;

use std::ffi::OsStr;
use std::path::Path;

use crownos_config::schema::{AccentColor, appearance, glass};
use crownui::kit::{
    ButtonOptions, ButtonSize, ButtonVariant, StatusOptions, Tone, button, icons, status,
};
use crownui::prelude::{
    AnyView, Cx, Listeners, Runtime, Signal, Styled, View, bind, hstack, spacer, vstack,
};

use crate::controls::{Options, choice, fraction_slider, switch};
use crate::layout::{
    INLINE_GAP, optional, setting_row, setting_row_content, setting_row_desc, settings_card_titled,
    settings_divider, value_text, when,
};
use crate::net::wallpaper::WallpaperState;
use crate::pages::{PageDescriptor, page};
use crate::state::Store;

static ACCENTS: Options<AccentColor> = Options::new(&[
    ("Purple", AccentColor::Purple),
    ("Blue", AccentColor::Blue),
    ("Green", AccentColor::Green),
    ("Orange", AccentColor::Orange),
    ("Pink", AccentColor::Pink),
]);

pub const PAGE: PageDescriptor = PageDescriptor {
    title: "Appearance",
    slug: "appearance",
    icon: icons::PALETTE,
    build,
};

fn build(cx: &mut Cx, store: Store) -> AnyView {
    page(
        &PAGE,
        (
            settings_card_titled(
                "Theme",
                (
                    setting_row("Dark mode", switch(cx, store, appearance::DarkMode)),
                    settings_divider(),
                    setting_row(
                        "Accent color",
                        choice(cx, store, appearance::Accent, &ACCENTS),
                    ),
                    settings_divider(),
                    setting_row_desc(
                        "Transparency",
                        "Translucency of panels and menus",
                        // Stored 0.0–1.0, presented 0–100.
                        fraction_slider(cx, store, appearance::Transparency),
                    ),
                ),
            ),
            desktop_card(store),
            edge_glow::rim_card(cx, store, "Window glass", glass::Windows),
            edge_glow::rim_card(cx, store, "Panel glass", glass::Panels),
            edge_glow::rim_card(cx, store, "Menu glass", glass::Menus),
        ),
    )
}

// --- MARK: The Desktop card ---

/// The wallpaper picker: a name row, any bad news, then the grid.
fn desktop_card(store: Store) -> impl View {
    let wallpaper = store.wallpaper;
    settings_card_titled(
        "Desktop",
        (
            setting_row(
                "Wallpaper",
                value_text(bind(move |runtime| {
                    wallpaper_label(current_wallpaper(runtime, store).as_deref())
                })),
            ),
            optional(
                move |runtime| wallpaper.with(runtime, |wallpaper| wallpaper.error.clone()),
                move |_, message| vstack((settings_divider(), error_row(wallpaper, message))),
            ),
            // A standing condition rather than a dismissible error, so it stays
            // for as long as it is true. Picking still works — the choice lands
            // in `appearance.ron` — hence the reassurance.
            when(
                move |runtime| wallpaper.with(runtime, |wallpaper| !wallpaper.online),
                |_| {
                    vstack((
                        settings_divider(),
                        setting_row_content(
                            status("The wallpaper service is not reachable — picks are saved and apply when it returns")
                                .tone(Tone::Warning),
                        ),
                    ))
                },
            ),
            settings_divider(),
            gallery::gallery(store),
        ),
    )
}

/// The path the selection marker belongs on: the daemon's word when it has
/// spoken, the config's otherwise. The two converge — the mirror in
/// [`crate::state`] writes every `Changed` back into the config — so this
/// only decides who leads in the first seconds and when the daemon is gone.
pub(super) fn current_wallpaper(runtime: &mut Runtime, store: Store) -> Option<String> {
    if let Some(path) = store
        .wallpaper
        .with(runtime, |wallpaper| wallpaper.current.clone())
    {
        return Some(path);
    }
    let configured: String = store.get(runtime, appearance::Wallpaper);
    (!configured.is_empty()).then_some(configured)
}

/// The current wallpaper's display name, or a placeholder — the schema spells
/// "no wallpaper" as an empty string, which would otherwise be a blank row.
fn wallpaper_label(current: Option<&str>) -> String {
    current.map_or_else(|| "None".to_owned(), file_label)
}

/// A path reduced to the name a person would call the image.
pub(super) fn file_label(path: &str) -> String {
    Path::new(path)
        .file_stem()
        .and_then(OsStr::to_str)
        .map_or_else(|| path.to_owned(), str::to_owned)
}

/// The dismissible last failure, in the same dress as the Wi-Fi page's.
fn error_row(wallpaper: Signal<WallpaperState>, message: String) -> impl View {
    setting_row_content(
        hstack((
            status(message).tone(Tone::Danger),
            spacer(),
            button("Dismiss")
                .variant(ButtonVariant::Ghost)
                .control_size(ButtonSize::Small)
                .on_click(move |cx, _| wallpaper.update(cx, WallpaperState::dismiss_error)),
        ))
        .gap(INLINE_GAP)
        .items_center()
        .self_stretch(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_name_the_image_not_the_directory() {
        assert_eq!(wallpaper_label(None), "None");
        assert_eq!(
            wallpaper_label(Some("/usr/share/backgrounds/dunes.png")),
            "dunes"
        );
        assert_eq!(
            wallpaper_label(Some("/w/archive.tar.webp")),
            "archive.tar",
            "only the final extension comes off"
        );
    }
}
