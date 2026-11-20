//! The arrangement canvas: every monitor drawn where it sits on the desktop.
//! Click one to select it; drag an enabled one to move it. A drag snaps to
//! the other monitors' edges, settles off any overlap when it is let go, and
//! goes into the drafts, so Apply and Discard work as for every other edit.

mod geometry;
mod paint;

use crownui::kit::use_theme;
use crownui::prelude::{
    Cx, Listeners, Point, Role, Runtime, Semantic, Signal, Size, Styled, Vector, View, canvas,
    for_each, vstack,
};

use crate::net::outputs::{Mode, Monitor, MonitorDraft, OutputState, Rect, bounds, logical_size};
use crate::state::Store;
use crate::util::bridge::derived;
use crate::util::signal::set_if_changed;
use geometry::{View as Fit, dragged, fit, normalize, settle, snap};

const CANVAS_HEIGHT: f32 = 220.0;
/// Canvas pixels within which a dragged edge snaps to another monitor's.
const SNAP_REACH: f32 = 12.0;
/// Of a parked monitor's height, the gap above the row of switched-off monitors.
const PARKED_GAP: i32 = 4;

/// One monitor as the canvas shows it.
#[derive(Debug, Clone, PartialEq)]
pub struct Tile {
    pub name: String,
    pub rect: Rect,
    pub enabled: bool,
    pub virtual_output: bool,
    pub primary: bool,
    pub selected: bool,
    pub title: String,
    pub resolution: String,
}

#[derive(Debug, Clone, PartialEq)]
struct Layout {
    view: Fit,
    tiles: Vec<Tile>,
}

impl Layout {
    fn tile_at(&self, point: Point) -> Option<&Tile> {
        self.tiles.iter().rev().find(|tile| {
            let [x, y, width, height] = self.view.to_canvas(tile.rect);
            (x..=x + width).contains(&point.x) && (y..=y + height).contains(&point.y)
        })
    }

    fn tile(&self, name: &str) -> Option<&Tile> {
        self.tiles.iter().find(|tile| tile.name == name)
    }
}

/// A drag under way, with the view frozen so the layout holds still under
/// the pointer.
#[derive(Debug, Clone, PartialEq)]
struct Grab {
    name: String,
    start: (i32, i32),
    position: (i32, i32),
    size: (i32, i32),
    view: Fit,
}

#[derive(Clone, Copy)]
struct Arrangement {
    outputs: Signal<OutputState>,
    size: Signal<Size>,
    grab: Signal<Option<Grab>>,
    layout: Signal<Layout>,
}

pub fn canvas_row(cx: &mut Cx, store: Store) -> impl View + use<> {
    let size = cx.signal(Size::ZERO);
    let grab = cx.signal(None);
    let layout = derived(cx, move |runtime| {
        let tiles = tiles(runtime, store, grab);
        let canvas = size.get(runtime);
        let view = grab.with(runtime, |grab| grab.as_ref().map(|grab| grab.view));
        let view = view.unwrap_or_else(|| {
            let rects: Vec<Rect> = tiles.iter().map(|tile| tile.rect).collect();
            fit(&rects, [canvas.width, canvas.height])
        });
        Layout { view, tiles }
    });
    let arrangement = Arrangement {
        outputs: store.outputs,
        size,
        grab,
        layout,
    };
    arrangement.view(cx)
}

impl Arrangement {
    fn view(self, cx: &mut Cx) -> impl View + use<> {
        let theme = use_theme(cx);
        let names = derived(cx, move |runtime| {
            self.layout.with(runtime, |layout| {
                layout
                    .tiles
                    .iter()
                    .map(|tile| tile.name.clone())
                    .collect::<Vec<_>>()
            })
        });
        let picture = canvas(move |runtime, size, list| {
            set_if_changed(runtime, self.size, size);
            let colors = theme.with(runtime, paint::Colors::of);
            self.layout.with(runtime, |layout| {
                for tile in &layout.tiles {
                    paint::tile(list, layout.view, tile, &colors);
                }
            });
        })
        .w_full()
        .h(CANVAS_HEIGHT)
        .role(Role::Image)
        .label("Monitor arrangement: drag a monitor to move it, click it to select it")
        .on_click(move |cx, event| self.select_at(cx, event.position))
        .on_drag_start(move |cx, event| self.grab_at(cx, event.position - event.total))
        .on_drag(move |cx, event| self.drag_by(cx, event.total))
        .on_drag_end(move |cx, _| self.release(cx));
        let labels = for_each(names, String::clone, move |_, name: Signal<String>| {
            paint::label(move |runtime| {
                let name = name.get(runtime);
                self.layout.with(runtime, |layout| {
                    layout
                        .tile(&name)
                        .map(|tile| (layout.view.to_canvas(tile.rect), tile.clone()))
                })
            })
        });
        let labels = labels
            .absolute()
            .top(0.0)
            .left(0.0)
            .w_full()
            .h(CANVAS_HEIGHT)
            .pointer_events_none();
        vstack((picture, labels)).relative().w_full()
    }

    fn select_at(self, cx: &mut Cx, at: Point) {
        let hit = self.layout.with_untracked(cx, |layout| {
            layout.tile_at(at).map(|tile| tile.name.clone())
        });
        if let Some(name) = hit {
            self.outputs
                .update(cx, |outputs| outputs.selected = Some(name));
        }
    }

    fn grab_at(self, cx: &mut Cx, at: Point) {
        let grab = self.layout.with_untracked(cx, |layout| {
            let tile = layout.tile_at(at).filter(|tile| tile.enabled)?;
            Some(Grab {
                name: tile.name.clone(),
                start: tile.rect.0,
                position: tile.rect.0,
                size: tile.rect.1,
                view: layout.view,
            })
        });
        if let Some(grab) = grab {
            self.select_at(cx, at);
            self.grab.set(cx, Some(grab));
        }
    }

    fn drag_by(self, cx: &mut Cx, total: Vector) {
        let Some(grab) = self.grab.with_untracked(cx, Clone::clone) else {
            return;
        };
        let others = self
            .outputs
            .with_untracked(cx, |outputs| outputs.enabled_rects(&grab.name));
        let moved = dragged(grab.start, [total.x, total.y], grab.view);
        let reach = (SNAP_REACH / grab.view.scale).round() as i32;
        let position = snap((moved, grab.size), &others, reach);
        if position != grab.position {
            self.grab.set(cx, Some(Grab { position, ..grab }));
        }
    }

    fn release(self, cx: &mut Cx) {
        let Some(grab) = self.grab.with_untracked(cx, Clone::clone) else {
            return;
        };
        self.grab.set(cx, None);
        self.outputs.update(cx, |outputs| {
            if outputs.draft(&grab.name).is_none() {
                return;
            }
            let position = settle(
                (grab.position, grab.size),
                &outputs.enabled_rects(&grab.name),
            );
            let mut placed: Vec<(String, (i32, i32))> = outputs
                .drafts
                .iter()
                .filter(|draft| draft.enabled)
                .map(|draft| {
                    let at = if draft.name == grab.name {
                        position
                    } else {
                        draft.position
                    };
                    (draft.name.clone(), at)
                })
                .collect();
            let mut positions: Vec<(i32, i32)> = placed.iter().map(|(_, at)| *at).collect();
            normalize(&mut positions);
            for ((_, at), normalized) in placed.iter_mut().zip(positions) {
                *at = normalized;
            }
            outputs.place(placed.iter().map(|(name, at)| (name.as_str(), *at)));
        });
    }
}

fn tiles(runtime: &mut Runtime, store: Store, grab: Signal<Option<Grab>>) -> Vec<Tile> {
    let primary = store.with_primary_monitor(runtime, |primary| {
        primary.map(|monitor| monitor.name.clone())
    });
    let grab = grab.get(runtime);
    store.outputs.with(runtime, |outputs| {
        let mut tiles: Vec<Tile> = outputs
            .drafts
            .iter()
            .filter_map(|draft| {
                let monitor = outputs.monitor(&draft.name)?;
                let mut rect = draft.geometry(monitor);
                if let Some(grab) = grab.as_ref().filter(|grab| grab.name == draft.name) {
                    rect.0 = grab.position;
                }
                Some(Tile {
                    name: draft.name.clone(),
                    rect,
                    enabled: draft.enabled,
                    virtual_output: monitor.is_virtual(),
                    primary: primary.as_deref() == Some(draft.name.as_str()),
                    selected: outputs.selected.as_deref() == Some(draft.name.as_str()),
                    title: monitor.title(),
                    resolution: resolution(monitor, draft),
                })
            })
            .collect();
        park_switched_off(&mut tiles, outputs);
        tiles
    })
}

/// Switched-off monitors have no place on the desktop, so they are shown in a
/// row under it, at the size their preferred mode would have.
fn park_switched_off(tiles: &mut [Tile], outputs: &OutputState) {
    let placed: Vec<Rect> = tiles
        .iter()
        .filter(|tile| tile.enabled)
        .map(|tile| tile.rect)
        .collect();
    let ((left, _), (_, bottom)) = bounds(&placed).unwrap_or(((0, 0), (0, 0)));
    let mut x = left;
    for tile in tiles.iter_mut().filter(|tile| !tile.enabled) {
        let size = outputs
            .monitor(&tile.name)
            .zip(outputs.draft(&tile.name))
            .map_or((0, 0), |(monitor, draft)| parked_size(monitor, draft));
        let gap = size.1 / PARKED_GAP;
        tile.rect = ((x, bottom + gap), size);
        x += size.0 + gap;
    }
}

fn parked_size(monitor: &Monitor, draft: &MonitorDraft) -> (i32, i32) {
    let mode = draft
        .mode
        .and_then(|index| monitor.modes.get(index))
        .or_else(|| monitor.modes.iter().find(|mode| mode.preferred))
        .or_else(|| monitor.modes.first());
    logical_size(mode, draft.scale, draft.transform)
}

fn resolution(monitor: &Monitor, draft: &MonitorDraft) -> String {
    draft
        .mode
        .and_then(|index| monitor.modes.get(index))
        .map_or_else(String::new, |Mode { width, height, .. }| {
            format!("{width}×{height}")
        })
}
