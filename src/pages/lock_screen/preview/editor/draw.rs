//! Drawing for the editor: images bent through the clock's warp, and the
//! handles over the clock.

use crownui::ext::{DrawCommand, DrawList, ImageId, Paint, Path, Stroke};
use crownui::prelude::{Color, CornerRadii, Point, Rect, WHITE};
use lockscreen::clock::Homography;

use super::EditMode;
use super::geometry::ClockEdit;
use crate::util::affine;
use lockscreen::layout::ClockGroup;

/// Cells per side an image is cut into to follow a warp: enough that the
/// straight edges between them do not show at preview size.
const GRID: usize = 6;
const OUTLINE: f32 = 1.5;
const KNOB: f32 = 5.0;
const OUTLINE_COLOR: Color = Color::new(1.0, 1.0, 1.0, 0.85);
const SHADE: Color = Color::new(0.0, 0.0, 0.0, 0.35);

/// `image`, stretched over `rect`, drawn where `map` sends each of its
/// points, then scaled by `to_canvas`. Affine maps draw it whole; anything
/// else is cut into triangles, each bent on its own.
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
    let corner = |u: usize, v: usize| {
        Point::new(
            rect.origin.x + rect.size.width * u as f32 / GRID as f32,
            rect.origin.y + rect.size.height * v as f32 / GRID as f32,
        )
    };
    for v in 0..GRID {
        for u in 0..GRID {
            let quad = [
                corner(u, v),
                corner(u + 1, v),
                corner(u + 1, v + 1),
                corner(u, v + 1),
            ];
            for triangle in [[quad[0], quad[1], quad[2]], [quad[0], quad[2], quad[3]]] {
                let Some(landed) = triangle
                    .iter()
                    .map(|point| canvas(*point))
                    .collect::<Option<Vec<_>>>()
                else {
                    continue;
                };
                let Some(bend) = affine::between(triangle, [landed[0], landed[1], landed[2]])
                else {
                    continue;
                };
                let path = Path::new()
                    .move_to(triangle[0])
                    .line_to(triangle[1])
                    .line_to(triangle[2])
                    .close();
                list.save();
                list.transform(bend);
                list.push(DrawCommand::FillPath(path, Paint::image(image, rect)));
                list.restore();
            }
        }
    }
}

/// The outline and knobs `mode` drags by, over the clock.
pub fn handles(list: &mut DrawList, edit: &ClockEdit, mode: EditMode, to_canvas: f32) {
    let canvas =
        |corners: [[f32; 2]; 4]| corners.map(|[x, y]| Point::new(x * to_canvas, y * to_canvas));
    match mode {
        EditMode::Move | EditMode::Warp => {
            let corners = canvas(edit.corners());
            outline(list, corners);
            for corner in corners {
                knob(list, corner);
            }
        }
        EditMode::Split => {
            for group in [ClockGroup::Hours, ClockGroup::Minutes] {
                if let Some(corners) = edit.group_corners(group) {
                    outline(list, canvas(corners));
                }
            }
        }
    }
}

fn outline(list: &mut DrawList, [a, b, c, d]: [Point; 4]) {
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
