//! How a monitor looks on the arrangement: a filled box with the selected
//! outline, and a label with its title, resolution and badges.

use crownui::ext::DrawList;
use crownui::kit::{INTER_FAMILY, Theme, use_theme};
use crownui::prelude::{
    Color, CornerRadii, Rect, Runtime, Styled, View, bind, component, text, vstack,
};

use super::Tile;
use super::geometry::View as Fit;

const RADIUS: f32 = 6.0;
const BORDER: f32 = 1.0;
const SELECTED_BORDER: f32 = 2.0;
const PADDING: f32 = 6.0;
const TITLE_SIZE: f32 = 12.0;
const DETAIL_SIZE: f32 = 11.0;
const FILL_ALPHA: f32 = 0.22;
const SWITCHED_OFF_ALPHA: f32 = 0.45;

pub struct Colors {
    fill: Color,
    border: Color,
    selected: Color,
    switched_off: Color,
}

impl Colors {
    pub fn of(theme: &Theme) -> Self {
        Self {
            fill: theme.accent.start.with_alpha_factor(FILL_ALPHA),
            border: theme.surface.border,
            selected: theme.accent.start,
            switched_off: theme.surface.hover,
        }
    }
}

pub fn tile(list: &mut DrawList, view: Fit, tile: &Tile, colors: &Colors) {
    let [x, y, width, height] = view.to_canvas(tile.rect);
    if width <= 0.0 || height <= 0.0 {
        return;
    }
    let rect = Rect::from_xywh(x, y, width, height);
    let radii = CornerRadii::uniform(RADIUS);
    let fill = if tile.enabled {
        colors.fill
    } else {
        colors.switched_off
    };
    list.fill(rect, radii, fill);
    if tile.selected {
        list.stroke(rect, radii, SELECTED_BORDER, colors.selected);
    } else {
        list.stroke(rect, radii, BORDER, colors.border);
    }
}

fn badges(tile: &Tile) -> String {
    [
        (tile.primary, "Primary"),
        (tile.virtual_output, "Remote"),
        (!tile.enabled, "Off"),
    ]
    .into_iter()
    .filter_map(|(shown, badge)| shown.then_some(badge))
    .collect::<Vec<_>>()
    .join(" · ")
}

/// A label over one tile, placed from what `placed` reads: the tile's canvas
/// box and the tile.
pub fn label(
    mut placed: impl FnMut(&mut Runtime) -> Option<([f32; 4], Tile)> + 'static,
) -> impl View {
    component(move |cx| {
        let theme = use_theme(cx);
        let current = cx.memo(move |runtime| placed(runtime));
        let read = move |pick: fn(&[f32; 4], &Tile) -> f32| {
            bind(move |runtime| {
                current.with(runtime, |placed| {
                    placed.as_ref().map_or(0.0, |(rect, tile)| pick(rect, tile))
                })
            })
        };
        let line = move |size: f32, pick: fn(&Tile) -> String| {
            text(bind(move |runtime| {
                current.with(runtime, |placed| {
                    placed
                        .as_ref()
                        .map_or_else(String::new, |(_, tile)| pick(tile))
                })
            }))
            .text_size(size)
            .font_family(INTER_FAMILY)
            .text_color(bind(move |runtime| {
                let switched_off = current.with(runtime, |placed| {
                    placed.as_ref().is_some_and(|(_, tile)| !tile.enabled)
                });
                theme.with(runtime, |theme| {
                    if switched_off {
                        theme.text.disabled
                    } else {
                        theme.text.primary
                    }
                })
            }))
            .truncate()
        };
        vstack((
            line(TITLE_SIZE, |tile| tile.title.clone()).font_semibold(),
            line(DETAIL_SIZE, |tile| tile.resolution.clone()),
            line(DETAIL_SIZE, badges),
        ))
        .absolute()
        .left(read(|rect, _| rect[0] + PADDING))
        .top(read(|rect, _| rect[1] + PADDING))
        .w(read(|rect, _| (rect[2] - 2.0 * PADDING).max(0.0)))
        .h(read(|rect, _| (rect[3] - 2.0 * PADDING).max(0.0)))
        .opacity(read(|_, tile| {
            if tile.enabled {
                1.0
            } else {
                SWITCHED_OFF_ALPHA
            }
        }))
        .overflow_hidden()
        .pointer_events_none()
    })
}
