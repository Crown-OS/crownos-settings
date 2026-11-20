//! The arrangement's geometry: logical desktop pixels in `i32`, so snapped
//! edges meet exactly, and the canvas transform in `f32`.

use crate::net::outputs::{Rect, beside, bounds, intersects};

/// Canvas pixels kept clear around the layout.
const MARGIN: f32 = 16.0;
/// The scale shown before there is anything to fit.
const EMPTY_SCALE: f32 = 0.1;

/// Canvas point = logical point × `scale` + `offset`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct View {
    pub scale: f32,
    pub offset: [f32; 2],
}

impl View {
    /// `[x, y, width, height]` in canvas pixels.
    pub fn to_canvas(self, ((x, y), (width, height)): Rect) -> [f32; 4] {
        [
            x as f32 * self.scale + self.offset[0],
            y as f32 * self.scale + self.offset[1],
            width as f32 * self.scale,
            height as f32 * self.scale,
        ]
    }
}

/// The view that shows every rectangle as large as `canvas` allows, centred.
pub fn fit(rects: &[Rect], canvas: [f32; 2]) -> View {
    let Some(((left, top), (right, bottom))) = bounds(rects) else {
        return View {
            scale: EMPTY_SCALE,
            offset: canvas.map(|side| side / 2.0),
        };
    };
    let size = [(right - left) as f32, (bottom - top) as f32];
    let room = canvas.map(|side| (side - 2.0 * MARGIN).max(1.0));
    let scale = (room[0] / size[0].max(1.0)).min(room[1] / size[1].max(1.0));
    let offset = [
        (canvas[0] - size[0] * scale) / 2.0 - left as f32 * scale,
        (canvas[1] - size[1] * scale) / 2.0 - top as f32 * scale,
    ];
    View { scale, offset }
}

/// Where a monitor that started at `start` is after the pointer moved `total`
/// canvas pixels.
pub fn dragged(start: (i32, i32), total: [f32; 2], view: View) -> (i32, i32) {
    (
        start.0 + (total[0] / view.scale).round() as i32,
        start.1 + (total[1] / view.scale).round() as i32,
    )
}

/// `rect`'s position, moved on each axis to the nearest edge of `others`
/// within `reach`: edge to edge, or edges aligned.
pub fn snap(rect: Rect, others: &[Rect], reach: i32) -> (i32, i32) {
    let ((x, y), (width, height)) = rect;
    let shift = |start: i32, extent: i32, edges: &dyn Fn(&Rect) -> (i32, i32)| {
        others
            .iter()
            .flat_map(|other| {
                let (low, high) = edges(other);
                [
                    low - (start + extent),
                    high - start,
                    low - start,
                    high - (start + extent),
                ]
            })
            .filter(|shift| shift.abs() <= reach)
            .min_by_key(|shift| shift.abs())
            .unwrap_or(0)
    };
    let horizontal = shift(x, width, &|((x, _), (width, _))| (*x, x + width));
    let vertical = shift(y, height, &|((_, y), (_, height))| (*y, y + height));
    (x + horizontal, y + vertical)
}

/// `rect`'s position once pushed off every rectangle in `others`, each time
/// along the axis that needs the shorter move. A push that only lands on
/// another one gives up and goes right of everything.
pub fn settle(rect: Rect, others: &[Rect]) -> (i32, i32) {
    let (mut position, size) = rect;
    for _ in 0..=others.len() {
        let Some(&((other_x, other_y), (other_width, other_height))) = others
            .iter()
            .find(|other| intersects((position, size), **other))
        else {
            return position;
        };
        let nearer = |away_low: i32, away_high: i32| {
            if away_low.abs() <= away_high.abs() {
                away_low
            } else {
                away_high
            }
        };
        let horizontal = nearer(
            other_x - (position.0 + size.0),
            other_x + other_width - position.0,
        );
        let vertical = nearer(
            other_y - (position.1 + size.1),
            other_y + other_height - position.1,
        );
        if horizontal.abs() <= vertical.abs() {
            position.0 += horizontal;
        } else {
            position.1 += vertical;
        }
    }
    if others
        .iter()
        .any(|other| intersects((position, size), *other))
    {
        return beside(others);
    }
    position
}

/// Moves every position so the layout starts at (0, 0).
pub fn normalize(positions: &mut [(i32, i32)]) {
    let Some(left) = positions.iter().map(|position| position.0).min() else {
        return;
    };
    let top = positions
        .iter()
        .map(|position| position.1)
        .min()
        .unwrap_or(0);
    for position in positions {
        position.0 -= left;
        position.1 -= top;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WIDE: (i32, i32) = (1920, 1080);

    fn at(x: i32, y: i32) -> Rect {
        ((x, y), WIDE)
    }

    #[test]
    fn fitting_centres_the_layout_at_the_largest_scale_that_shows_it() {
        let view = fit(&[at(0, 0), at(1920, 0)], [800.0, 300.0]);
        assert!((view.scale - (800.0 - 2.0 * MARGIN) / 3840.0).abs() < 1e-6);
        let [x, y, width, height] = view.to_canvas(at(0, 0));
        assert!((x - MARGIN).abs() < 1e-3);
        assert!((y + height / 2.0 - 150.0).abs() < 1e-3);
        assert!((width - 1920.0 * view.scale).abs() < 1e-3);
    }

    #[test]
    fn fitting_nothing_or_monitors_without_a_mode_does_not_divide_by_zero() {
        for rects in [vec![], vec![((0, 0), (0, 0))]] {
            let view = fit(&rects, [400.0, 200.0]);
            assert!(view.scale.is_finite() && view.scale > 0.0);
        }
    }

    #[test]
    fn a_drag_moves_by_the_pointer_travel_in_logical_pixels() {
        let view = View {
            scale: 0.1,
            offset: [0.0, 0.0],
        };
        assert_eq!(dragged((1920, 0), [-10.0, 4.04], view), (1820, 40));
    }

    #[test]
    fn a_near_edge_snaps_edge_to_edge() {
        assert_eq!(snap(at(1930, 5), &[at(0, 0)], 50), (1920, 0));
        assert_eq!(snap(at(-1900, 1075), &[at(0, 0)], 50), (-1920, 1080));
    }

    #[test]
    fn a_near_edge_snaps_aligned() {
        let tall = ((1920, 30), (1080, 1920));
        assert_eq!(snap(tall, &[at(0, 0)], 50), (1920, 0));
    }

    #[test]
    fn a_far_edge_does_not_snap() {
        assert_eq!(snap(at(2100, 300), &[at(0, 0)], 50), (2100, 300));
    }

    #[test]
    fn settling_pushes_out_along_the_smaller_overlap() {
        assert_eq!(settle(at(1800, 200), &[at(0, 0)]), (1920, 200));
        assert_eq!(settle(at(300, 1000), &[at(0, 0)]), (300, 1080));
    }

    #[test]
    fn settling_leaves_a_monitor_that_overlaps_nothing() {
        assert_eq!(settle(at(1920, 0), &[at(0, 0)]), (1920, 0));
    }

    #[test]
    fn settling_between_two_monitors_does_not_land_on_a_third() {
        let others = [at(0, 0), at(1920, 0), at(3840, 0)];
        let position = settle(at(1000, 0), &others);
        assert!(
            others
                .iter()
                .all(|other| !intersects((position, WIDE), *other)),
            "{position:?} overlaps"
        );
    }

    #[test]
    fn beside_is_right_of_everything_level_with_the_top() {
        assert_eq!(beside(&[at(0, 100), at(1920, 0)]), (3840, 0));
        assert_eq!(beside(&[]), (0, 0));
    }

    #[test]
    fn normalizing_starts_the_layout_at_the_origin() {
        let mut positions = [(-1920, 300), (0, 0)];
        normalize(&mut positions);
        assert_eq!(positions, [(0, 300), (1920, 0)]);
    }
}
