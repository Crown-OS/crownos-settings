//! The wallpaper grid: bento sections of clickable thumbnails.

use crownos_config::schema::appearance;
use crownui::kit::{StatusOptions, Tone, spinner, status, use_theme};
use crownui::prelude::{
    AnyView, CursorIcon, Listeners, Role, Semantic, Styled, TRANSPARENT, View, bind, component,
    cover_image, hstack, keyed, vstack,
};

use super::{current_wallpaper, file_label};
use crate::layout::{INLINE_GAP, setting_row_content, value_text};
use crate::net::wallpaper::{WallpaperCommand, WallpaperState};
use crate::state::Store;
use crate::util::bento;
use crate::util::theme::tint;

/// One cell row's height. The pattern spans three to nine of them, so the row
/// is a fine unit rather than a tile-sized one: this is the value that lands
/// every shape on the reference layout's proportions at the widths this pane
/// takes. Columns are not fixed the same way — a section divides whatever
/// width it is given into [`bento::COLUMNS`] of them.
const CELL_HEIGHT: f32 = 33.0;
/// The gap the eye sees between two thumbnails, in both directions and
/// between blocks.
const TILE_GAP: f32 = 14.0;
/// The selection ring: an accent border floated just off the thumbnail,
/// macOS-picker style, so choosing never occludes the image.
const RING_WIDTH: f32 = 2.0;
const RING_RADIUS: f32 = 12.0;
/// Breathing room between the ring and the thumbnail it crowns.
const RING_GAP: f32 = 3.0;
/// The thumbnail's own corners, inside the ring's.
const IMAGE_RADIUS: f32 = RING_RADIUS - RING_GAP;
/// How far inside its cell the ring holds the thumbnail. Reserved on every
/// tile, selected or not, so a pick never resizes anything.
const RING_INSET: f32 = RING_GAP + RING_WIDTH;
/// What the layout is told, so that what the eye sees is [`TILE_GAP`]: the
/// ring reserves [`RING_INSET`] on each of the two tiles either side of a gap.
const CELL_GAP: f32 = TILE_GAP - 2.0 * RING_INSET;
/// A spinner sized to sit in a row without changing its height.
const ROW_SPINNER: f32 = 14.0;

/// What the grid's structure depends on; thumbnails and selection are bound.
#[derive(Debug, Clone, PartialEq)]
enum Shape {
    Looking,
    Empty,
    Tiles(Vec<String>),
}

fn shape(wallpaper: &WallpaperState) -> Shape {
    match (wallpaper.entries.is_empty(), wallpaper.scanned) {
        (true, true) => Shape::Empty,
        (true, false) => Shape::Looking,
        (false, _) => Shape::Tiles(
            wallpaper
                .entries
                .iter()
                .map(|entry| entry.path.clone())
                .collect(),
        ),
    }
}

/// The thumbnail grid, or whichever quiet state stands in for it.
pub fn gallery(store: Store) -> impl View {
    keyed(
        move |runtime| store.wallpaper.with(runtime, shape),
        move |_, shape| match shape {
            Shape::Looking => setting_row_content(
                hstack((
                    spinner().size(ROW_SPINNER),
                    value_text("Looking for wallpapers…"),
                ))
                .gap(INLINE_GAP)
                .items_center(),
            )
            .boxed(),
            Shape::Empty => setting_row_content(
                status("No wallpapers found in ~/.backgrounds or /usr/share/backgrounds")
                    .tone(Tone::Neutral),
            )
            .boxed(),
            Shape::Tiles(paths) => {
                setting_row_content(vstack(sections(store, &paths)).gap(CELL_GAP).self_stretch())
                    .boxed()
            }
        },
    )
}

/// Every section of the pattern the wallpapers reach, in order.
fn sections(store: Store, paths: &[String]) -> Vec<AnyView> {
    paths
        .chunks(bento::BLOCK_SIZE)
        .enumerate()
        .flat_map(|(block, chunk)| {
            let (showcase, band) = chunk.split_at(chunk.len().min(bento::SHOWCASE_TILES));
            [
                section(
                    store,
                    showcase,
                    bento::showcase(block),
                    bento::SHOWCASE_ROWS,
                ),
                section(store, band, bento::band(block), bento::BAND_ROWS),
            ]
        })
        .flatten()
        .collect()
}

/// One section as a row of columns, or nothing when the gallery ran out of
/// wallpapers before reaching it. A column whose tiles ran out keeps its
/// width, so a short last section leaves holes rather than reflowing.
fn section(
    store: Store,
    paths: &[String],
    cells: impl Iterator<Item = bento::Cell>,
    rows: i32,
) -> Option<AnyView> {
    if paths.is_empty() {
        return None;
    }
    let columns: Vec<_> = bento::columns(cells)
        .into_iter()
        .map(|column| {
            let tiles: Vec<_> = column
                .tiles
                .iter()
                .filter_map(|(index, height)| {
                    paths
                        .get(*index)
                        .map(|path| tile(store, path.clone(), span(*height)))
                })
                .collect();
            // A column `width` cells wide also spans the gaps between them, so
            // its basis is those gaps and its share of the rest is its width.
            vstack(tiles)
                .gap(CELL_GAP)
                .grow(column.width as f32)
                .basis((column.width - 1) as f32 * CELL_GAP)
        })
        .collect();
    Some(hstack(columns).gap(CELL_GAP).h(span(rows)).boxed())
}

/// The length `cells` cell rows (or gaps between columns) cover.
fn span(cells: i32) -> f32 {
    cells as f32 * (CELL_HEIGHT + CELL_GAP) - CELL_GAP
}

/// One clickable wallpaper, filling its place in the pattern.
///
/// What is drawn is the thumbnail (or the sunken well it will land in) and
/// the accent ring when it is the wallpaper on the desktop right now. The
/// ring's room is reserved on every tile, so selection never changes a tile's
/// size and the grid never shifts under a click.
fn tile(store: Store, path: String, height: f32) -> impl View {
    component(move |cx| {
        let theme = use_theme(cx);
        let wallpaper = store.wallpaper;
        let (thumbnail_path, ring_path, label) = (path.clone(), path.clone(), file_label(&path));

        let face = keyed(
            move |runtime| {
                wallpaper.with(runtime, |wallpaper| wallpaper.thumbnail_of(&thumbnail_path))
            },
            |_, thumbnail| match thumbnail {
                Some(thumbnail) => cover_image(thumbnail.image, thumbnail.size, IMAGE_RADIUS)
                    .flex_1()
                    .boxed(),
                // Still in the decode queue: a quiet well the thumbnail will
                // land in.
                None => vstack(()).boxed(),
            },
        )
        .flex_1();

        vstack(
            vstack(face)
                .flex_1()
                .rounded(IMAGE_RADIUS)
                .bg(tint(theme, |theme| theme.surface.sunken)),
        )
        .h(height)
        .p(RING_INSET)
        .border(RING_WIDTH)
        .rounded(RING_RADIUS)
        .border_color(bind(move |runtime| {
            let selected = current_wallpaper(runtime, store).as_deref() == Some(ring_path.as_str());
            theme.with(runtime, |theme| {
                if selected {
                    theme.accent.end
                } else {
                    TRANSPARENT
                }
            })
        }))
        .cursor(CursorIcon::Pointer)
        .role(Role::Button)
        .label(label)
        .on_click(move |cx, _| {
            // Both halves of a pick, deliberately: the config write is the
            // preference (instant, survives a dead daemon), the command is the
            // transition. The marker follows the daemon's `Changed` back.
            store.set(cx, appearance::Wallpaper, path.clone());
            wallpaper.update(cx, |wallpaper| {
                wallpaper.send(WallpaperCommand::Set { path: path.clone() });
            });
        })
    })
}
