//! Drawing for the editor: images carried through the clock's edit, and the
//! handles over the clock — ColorOS's thin rounded outline with its thick
//! quarter-arc corner, or the warp corners, or the hours and minutes boxes.

use crownui::ext::{DrawCommand, DrawList, ImageId, Paint, Path, Stroke};
use crownui::prelude::{Color, CornerRadii, Point, Rect, WHITE};
use lockscreen::clock::Homography;
use lockscreen::layout::ClockGroup;

use super::geometry::ClockEdit;
use super::{EditMode, OUTLINE_PAD};
use crate::util::affine;

/// The most cells per side an image is cut into to follow a warp.
const GRID: usize = 8;
/// Canvas pixels a cell's flat triangles may stray from the warp: less than
/// shows at preview size.
const BEND_TOLERANCE: f32 = 0.35;
const OUTLINE: f32 = 1.2;
const OUTLINE_RADIUS: f32 = 14.0;
const OUTLINE_COLOR: Color = Color::new(1.0, 1.0, 1.0, 0.7);
/// The stretch handle: a thick arc round the outline's bottom right corner,
/// standing a little off it, its ends running on along the two edges.
const ARC_WIDTH: f32 = 4.0;
const ARC_GAP: f32 = 2.5;
const ARC_TAIL: f32 = 5.0;
/// How far a cubic's controls sit along a quarter circle's tangents.
const QUARTER_CIRCLE: f32 = 0.552_284_8;
const KNOB: f32 = 5.0;
const SHADE: Color = Color::new(0.0, 0.0, 0.0, 0.3);

/// `image`, stretched over `rect`, drawn where `map` sends each of its
/// points, then scaled by `to_canvas`: cut into as few cells as follow the
/// warp, each a rectangle of the image bent flat onto three of its corners.
pub fn warped_image(
    list: &mut DrawList,
    image: ImageId,
    rect: Rect,
    map: &Homography,
    to_canvas: f32,
) {
    let canvas = |point: Point| {
        map.apply([point.x, point.y])
            .map(|[x, y]| Point::new(x * to_canvas, y * to_canvas))
    };
    let cells = (1..GRID)
        .find(|cells| follows(rect, *cells, &canvas))
        .unwrap_or(GRID);
    for cell in 0..cells * cells {
        let [u, v] = [cell % cells, cell / cells].map(|side| side as f32);
        let corners = [[u, v], [u + 1.0, v], [u, v + 1.0]].map(|at| cell_point(rect, cells, at));
        let [Some(origin), Some(across), Some(down)] = corners.map(canvas) else {
            continue;
        };
        let Some(bend) = affine::between(corners, [origin, across, down]) else {
            continue;
        };
        let [top_left, top_right, bottom_left] = corners;
        let piece = Rect::from_xywh(
            top_left.x,
            top_left.y,
            top_right.x - top_left.x,
            bottom_left.y - top_left.y,
        );
        list.save();
        list.transform(bend);
        list.push(DrawCommand::FillRect(piece, Paint::image(image, rect)));
        list.restore();
    }
}

/// Whether `rect` cut into `cells` per side lands within [`BEND_TOLERANCE`]
/// of where `canvas` sends it: a cell bent onto three corners strays
/// furthest at its fourth.
fn follows(rect: Rect, cells: usize, canvas: &impl Fn(Point) -> Option<Point>) -> bool {
    let at = |u: f32, v: f32| canvas(cell_point(rect, cells, [u, v]));
    (0..cells * cells).all(|cell| {
        let [u, v] = [cell % cells, cell / cells].map(|side| side as f32);
        match (
            at(u, v),
            at(u + 1.0, v),
            at(u, v + 1.0),
            at(u + 1.0, v + 1.0),
        ) {
            (Some(origin), Some(across), Some(down), Some(far)) => {
                let flat = [across.x + down.x - origin.x, across.y + down.y - origin.y];
                (far.x - flat[0]).hypot(far.y - flat[1]) <= BEND_TOLERANCE
            }
            _ => false,
        }
    })
}

/// The point `[u, v]` cells into `rect` cut into `cells` per side.
fn cell_point(rect: Rect, cells: usize, [u, v]: [f32; 2]) -> Point {
    Point::new(
        rect.origin.x + rect.size.width * u / cells as f32,
        rect.origin.y + rect.size.height * v / cells as f32,
    )
}

/// `rect` where the affine `map` sends it, scaled by `to_canvas`.
pub fn mapped_rect(rect: Rect, map: &Homography, to_canvas: f32) -> Option<Rect> {
    let [left, top] = map.apply([rect.min_x(), rect.min_y()])?;
    let [right, bottom] = map.apply([rect.max_x(), rect.max_y()])?;
    Some(Rect::from_xywh(
        left * to_canvas,
        top * to_canvas,
        (right - left) * to_canvas,
        (bottom - top) * to_canvas,
    ))
}

/// The handles `mode` drags by, over the clock.
pub fn handles(list: &mut DrawList, edit: &ClockEdit, mode: EditMode, to_canvas: f32) {
    let canvas =
        |corners: [[f32; 2]; 4]| corners.map(|[x, y]| Point::new(x * to_canvas, y * to_canvas));
    match mode {
        EditMode::Move => {
            let around = edit.outline(OUTLINE_PAD / to_canvas);
            let around = Rect::from_xywh(
                around.x * to_canvas,
                around.y * to_canvas,
                around.width * to_canvas,
                around.height * to_canvas,
            );
            rounded_outline(list, around);
            stretch_arc(list, around);
        }
        EditMode::Warp => {
            let corners = canvas(edit.corners());
            quad_outline(list, corners);
            for corner in corners {
                knob(list, corner);
            }
        }
        EditMode::Split => {
            for group in [ClockGroup::Hours, ClockGroup::Minutes] {
                if let Some(corners) = edit.group_corners(group) {
                    quad_outline(list, canvas(corners));
                }
            }
        }
    }
}

fn rounded_outline(list: &mut DrawList, around: Rect) {
    let radii = CornerRadii::uniform(OUTLINE_RADIUS);
    list.stroke(around, radii, OUTLINE + 1.0, SHADE);
    list.stroke(around, radii, OUTLINE, OUTLINE_COLOR);
}

fn stretch_arc(list: &mut DrawList, around: Rect) {
    let centre = Point::new(
        around.max_x() - OUTLINE_RADIUS,
        around.max_y() - OUTLINE_RADIUS,
    );
    let reach = OUTLINE_RADIUS + ARC_GAP;
    let pull = reach * QUARTER_CIRCLE;
    let path = Path::new()
        .move_to(Point::new(centre.x + reach, centre.y - ARC_TAIL))
        .line_to(Point::new(centre.x + reach, centre.y))
        .cubic_to(
            Point::new(centre.x + reach, centre.y + pull),
            Point::new(centre.x + pull, centre.y + reach),
            Point::new(centre.x, centre.y + reach),
        )
        .line_to(Point::new(centre.x - ARC_TAIL, centre.y + reach));
    list.push(DrawCommand::StrokePath(
        path.clone(),
        Stroke::rounded(ARC_WIDTH + 1.5),
        Paint::solid(SHADE),
    ));
    list.push(DrawCommand::StrokePath(
        path,
        Stroke::rounded(ARC_WIDTH),
        Paint::solid(WHITE),
    ));
}

fn quad_outline(list: &mut DrawList, [a, b, c, d]: [Point; 4]) {
    let path = Path::new()
        .move_to(a)
        .line_to(b)
        .line_to(c)
        .line_to(d)
        .close();
    list.push(DrawCommand::StrokePath(
        path.clone(),
        Stroke::new(OUTLINE + 1.0),
        Paint::solid(SHADE),
    ));
    list.push(DrawCommand::StrokePath(
        path,
        Stroke::new(OUTLINE),
        Paint::solid(OUTLINE_COLOR),
    ));
}

fn knob(list: &mut DrawList, centre: Point) {
    let ring = Rect::from_xywh(
        centre.x - KNOB - 1.0,
        centre.y - KNOB - 1.0,
        2.0 * KNOB + 2.0,
        2.0 * KNOB + 2.0,
    );
    list.fill(ring, CornerRadii::uniform(KNOB + 1.0), SHADE);
    let dot = Rect::from_xywh(centre.x - KNOB, centre.y - KNOB, 2.0 * KNOB, 2.0 * KNOB);
    list.fill(dot, CornerRadii::uniform(KNOB), WHITE);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cells_for(map: &Homography) -> usize {
        let rect = Rect::from_xywh(100.0, 100.0, 60.0, 120.0);
        let canvas = |point: Point| map.apply([point.x, point.y]).map(|[x, y]| Point::new(x, y));
        (1..GRID)
            .find(|cells| follows(rect, *cells, &canvas))
            .unwrap_or(GRID)
    }

    #[test]
    fn a_warp_is_cut_into_as_many_cells_as_it_bends() {
        assert_eq!(cells_for(&Homography::affine([1.5; 2], [20.0, -8.0])), 1);
        let bend = |lift: f32| {
            Homography::box_to_quad(
                [0.0, 0.0],
                [400.0, 200.0],
                [
                    [0.0, lift],
                    [400.0, 0.0],
                    [400.0, 200.0],
                    [0.0, 200.0 - lift],
                ],
            )
            .expect("a quad")
        };
        let (gentle, strong) = (cells_for(&bend(10.0)), cells_for(&bend(80.0)));
        assert!(gentle > 1 && gentle < strong, "{gentle} then {strong}");
    }
}
