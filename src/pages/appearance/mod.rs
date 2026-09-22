//! The Appearance page
mod cover_image;

use std::ffi::OsStr;
use std::path::Path;

use blinc_icons::icons;
use crownos_config::schema::{AccentColor, appearance};
use crownuikit::config::theme;
use crownuikit::widgets::{ButtonVariant, Tone, button, spinner, status};
use xilem::masonry::properties::Padding;
use xilem::masonry::properties::types::AsUnit;
use xilem::style::Style;
use xilem::view::{
    CrossAxisAlignment, FlexSpacer, GridExt, GridParams, button as bare_button, flex_col, flex_row,
    grid, sized_box,
};
use xilem::{Color, WidgetView};

use crate::controls::{Options, choice, fraction_slider, switch, value_text};
use crate::layout::{
    setting_row, setting_row_content, setting_row_desc, settings_card_titled,
    settings_divider,
};
use crate::net::wallpaper::{WallpaperCommand, WallpaperEntry, WallpaperState};
use crate::pages::appearance::cover_image::cover_image;
use crate::pages::{PageDescriptor, PageView, page};
use crate::state::Store;
use crate::util::bento;


/// Cell columns the bento pattern tiles across. Fixed rather than measured —
/// the grid divides whatever width it is given into this many — so it is the
/// tile *count* per row that is constant, not a tile's width.
const GRID_COLUMNS: i32 = 4;
/// One cell row's height. The feature tile spans two of these, which lands
/// both tile shapes near a desktop's own proportions at the widths this pane
/// takes.
const CELL_HEIGHT: f64 = 88.0;
/// The gap the eye sees between two thumbnails, in both directions and
/// between blocks — the pattern's tiles differ in size but never in the
/// distance between them.
const TILE_GAP: f64 = 14.0;
/// The selection ring: an accent-colored border floated just off the
/// thumbnail, macOS-picker style, so choosing never occludes the image.
const RING_WIDTH: f64 = 2.0;
const RING_RADIUS: f64 = 12.0;
/// Breathing room between the ring and the thumbnail it crowns.
const RING_GAP: f64 = 3.0;
/// The thumbnail's own corners, inside the ring's.
const IMAGE_RADIUS: f64 = RING_RADIUS - RING_GAP;
/// How far inside its cell the ring holds the thumbnail. Reserved on every
/// tile, selected or not, so a pick never resizes anything.
const RING_INSET: f64 = RING_GAP + RING_WIDTH;
/// What the grid is told, so that what the eye sees is [`TILE_GAP`]: the ring
/// reserves [`RING_INSET`] on each of the two tiles either side of a gap.
const CELL_GAP: f64 = TILE_GAP - 2.0 * RING_INSET;
/// One block's height: its rows, plus the cell gaps between them.
const BLOCK_HEIGHT: f64 = bento::BLOCK_ROWS as f64 * (CELL_HEIGHT + CELL_GAP) - CELL_GAP;
/// A spinner sized to sit in a row without changing its height — the same
/// size the Wi-Fi page uses for the same purpose.
const ROW_SPINNER: f64 = 14.0;
/// Gap between the pieces of a status row.
const STATUS_GAP: f64 = 10.0;

static ACCENTS: Options<AccentColor> = Options::new(&[
    ("Purple", AccentColor::Purple),
    ("Blue", AccentColor::Blue),
    ("Green", AccentColor::Green),
    ("Orange", AccentColor::Orange),
    ("Pink", AccentColor::Pink),
]);

pub const PAGE: PageDescriptor = PageDescriptor {
    title: "Appearance",
    icon: icons::PALETTE,
    build,
};

fn build(store: &Store) -> PageView {
    page(
        &PAGE,
        (
            settings_card_titled(
                "Theme",
                (
                    setting_row(
                        "Dark mode",
                        switch(store, appearance::DarkMode),
                    ),
                    settings_divider(),
                    setting_row("Accent color", choice(store, appearance::Accent, &ACCENTS)),
                    settings_divider(),
                    setting_row_desc(
                        "Transparency",
                        "Translucency of panels and menus",
                        // Stored 0.0–1.0, presented 0–100.
                        fraction_slider(store, appearance::Transparency),
                    ),
                ),
            ),
            desktop_card(store),
        ),
    )
}

// --- MARK: The Desktop card ---

/// The wallpaper picker: a name row, any bad news, then the grid.
fn desktop_card(store: &Store) -> impl WidgetView<Store> + use<> {
    let wallpaper = &store.wallpaper;
    let current = current_wallpaper(store);

    let mut rows: Vec<PageView> = vec![
        setting_row("Wallpaper", value_text(wallpaper_label(current.as_deref()))).boxed(),
    ];

    if let Some(message) = wallpaper.error.as_deref() {
        rows.push(settings_divider().boxed());
        rows.push(error_row(message.to_owned()).boxed());
    }
    // Not a dismissible error but a standing condition, so it stays for as
    // long as it is true. Picking still works — the choice lands in
    // `appearance.ron` — hence the reassurance rather than a prohibition.
    if !wallpaper.online {
        rows.push(settings_divider().boxed());
        rows.push(
            setting_row_content(
                status("The wallpaper service is not reachable — picks are saved and apply when it returns")
                    .tone(Tone::Warning),
            )
            .boxed(),
        );
    }

    rows.push(settings_divider().boxed());
    rows.push(gallery(wallpaper, current.as_deref()));

    settings_card_titled("Desktop", rows)
}

/// The path the selection marker belongs on: the daemon's word when it has
/// spoken, the config's otherwise. The two converge — the mirror in
/// [`crate::state`] writes every `Changed` back into the config — so this
/// only decides who leads in the first seconds and when the daemon is gone.
fn current_wallpaper(store: &Store) -> Option<String> {
    if let Some(path) = store.wallpaper.current.clone() {
        return Some(path);
    }
    let configured: String = store.get(appearance::Wallpaper);
    (!configured.is_empty()).then_some(configured)
}

/// The current wallpaper's display name, or a placeholder — the schema spells
/// "no wallpaper" as an empty string, which would otherwise be a blank row.
fn wallpaper_label(current: Option<&str>) -> String {
    current.map_or_else(|| "None".to_owned(), file_label)
}

/// A path reduced to the name a person would call the image.
fn file_label(path: &str) -> String {
    Path::new(path)
        .file_stem()
        .and_then(OsStr::to_str)
        .map_or_else(|| path.to_owned(), str::to_owned)
}

// --- MARK: The grid ---

/// The thumbnail grid, or whichever quiet state stands in for it.
fn gallery(wallpaper: &WallpaperState, current: Option<&str>) -> PageView {
    if wallpaper.entries.is_empty() {
        if wallpaper.scanned {
            return setting_row_content(
                status("No wallpapers found in ~/.backgrounds or /usr/share/backgrounds")
                    .tone(Tone::Neutral),
            )
            .boxed();
        }
        // Nothing has arrived yet — also the state the page-building test
        // renders, since a default store has never heard from the worker.
        return setting_row_content(
            flex_row((
                spinner().size(ROW_SPINNER),
                value_text("Looking for wallpapers…"),
            ))
            .gap(STATUS_GAP.px())
            .cross_axis_alignment(CrossAxisAlignment::Center),
        )
        .boxed();
    }

    // One grid per block rather than one grid for the gallery: a grid divides
    // the height it is handed among its rows, and a tall enough one would be
    // squashed to the scroll viewport — `sized_box` clamps a height to the
    // constraints it is given. A block is short enough never to meet that
    // ceiling, and stacking them keeps the pattern's own gap between blocks.
    let blocks: Vec<PageView> = wallpaper
        .entries
        .chunks(bento::block_size(GRID_COLUMNS))
        .enumerate()
        .map(|(block, entries)| {
            let tiles: Vec<_> = entries
                .iter()
                .enumerate()
                .map(|(slot, entry)| {
                    let cell = bento::cell(block, slot, GRID_COLUMNS);
                    tile(entry, current == Some(entry.path.as_str()))
                        .grid_item(GridParams::new(cell.x, cell.y, cell.width, cell.height))
                })
                .collect();

            sized_box(grid(tiles, GRID_COLUMNS, bento::BLOCK_ROWS).spacing(CELL_GAP.px()))
                .expand_width()
                .height(BLOCK_HEIGHT.px())
                .boxed()
        })
        .collect();

    setting_row_content(
        flex_col(blocks)
            .gap(CELL_GAP.px())
            .cross_axis_alignment(CrossAxisAlignment::Start),
    )
    .boxed()
}

/// One clickable wallpaper, filling the cell the pattern gave it.
///
/// xilem's bare `button` rather than the kit's: the kit's button is a
/// lettered pill, and this is a picture that wants nothing drawn over it. The
/// bare one still arrives wearing masonry's default button chrome — a fill, a
/// border and asymmetric padding — which is stripped here, because a tile that
/// is inset by its own chrome is a tile whose gaps no longer match its
/// neighbours'. What is left is the thumbnail (or the sunken well it will land
/// in) and the accent ring when it is the wallpaper on the desktop right now.
/// The ring is drawn — at full transparency — on every tile, so selection
/// never changes a tile's size and the grid never shifts under a click.
fn tile(entry: &WallpaperEntry, selected: bool) -> PageView {
    let theme = theme();

    let face: PageView = match entry.thumbnail.clone() {
        Some(brush) => cover_image(brush).corner_radius(IMAGE_RADIUS).boxed(),
        // Still in the decode queue: a quiet well the thumbnail will land in.
        None => flex_col(()).boxed(),
    };

    let face = sized_box(face)
        .expand()
        .background_color(theme.surface.sunken)
        .corner_radius(IMAGE_RADIUS);

    let ring = if selected {
        theme.accent.end
    } else {
        Color::TRANSPARENT
    };
    let framed = sized_box(face)
        .padding(Padding::all(RING_GAP))
        .border(ring, RING_WIDTH)
        .corner_radius(RING_RADIUS);

    let path = entry.path.clone();
    bare_button(framed, move |store: &mut Store| {
        // Both halves of a pick, deliberately: the config write is the
        // preference (instant, survives a dead daemon), the command is the
        // transition. The marker follows the daemon's `Changed` coming back.
        store.set(appearance::Wallpaper, path.clone());
        store
            .wallpaper
            .send(WallpaperCommand::Set { path: path.clone() });
    })
    .padding(Padding::ZERO)
    .background_color(Color::TRANSPARENT)
    .active_background_color(Color::TRANSPARENT)
    .border(Color::TRANSPARENT, 0.0)
    .boxed()
}

// --- MARK: Bad news ---

/// The dismissible last failure, in the same dress as the Wi-Fi page's.
fn error_row(message: String) -> impl WidgetView<Store> + use<> {
    setting_row_content(
        flex_row((
            status(message).tone(Tone::Danger),
            FlexSpacer::Flex(1.0),
            button("Dismiss", |store: &mut Store| {
                store.wallpaper.dismiss_error();
            })
            .variant(ButtonVariant::Ghost)
            .small(),
        ))
        .gap(STATUS_GAP.px())
        .cross_axis_alignment(CrossAxisAlignment::Center),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The one derivation this page performs on a path.
    #[test]
    fn labels_name_the_image_not_the_directory() {
        assert_eq!(wallpaper_label(None), "None");
        assert_eq!(wallpaper_label(Some("/usr/share/backgrounds/dunes.png")), "dunes");
        assert_eq!(
            wallpaper_label(Some("/w/archive.tar.webp")),
            "archive.tar",
            "only the final extension comes off"
        );
    }
}
