//! The preview, drawn: what the compositor stacks on an output, scaled to
//! the card.

use std::time::Instant;

use crownui::ext::{DrawCommand, DrawList, ImageId, Transform};
use crownui::prelude::{BLACK, Color, CornerRadii, Point, Rect, Size, Vector};
use lockscreen::clock::MORPH_BLUR;
use lockscreen::flow::Frame;
use lockscreen::layout::{Layer, Part};
use lockscreen::transition::LayerStyle;

use super::backdrop::Backdrop;
use super::editor::{ClockEdit, EditMode, handles, warped_image};
use super::sprites::{Placed, SpriteSet};
use super::studio::Slot;

pub const CORNER_RADIUS: f32 = 12.0;

/// What the lock's settings say about the background, apart from pictures.
pub struct Look {
    /// The lock has a background of its own, over the desktop.
    pub own_background: bool,
    /// That background is a flat color.
    pub fill: Option<Color>,
    pub dim: f32,
}

/// The clock as the editor has it.
pub struct Clock<'a> {
    /// The laid-out clock, shown with the frame being edited.
    pub edit: Option<&'a ClockEdit>,
    /// The handles to draw over it, if any.
    pub handles: Option<EditMode>,
    /// The instant changing digits are drawn at.
    pub now: Instant,
}

#[allow(clippy::too_many_arguments)]
pub fn compose(
    list: &mut DrawList,
    size: Size,
    output: [f32; 2],
    look: &Look,
    backdrop: &Backdrop,
    sprites: &SpriteSet,
    frame: &Frame,
    clock: &Clock<'_>,
) {
    let bounds = Rect::new(Point::ZERO, size);
    list.save();
    list.clip(bounds, CornerRadii::uniform(CORNER_RADIUS));
    list.fill(bounds, CornerRadii::ZERO, BLACK);
    if let Some(desktop) = backdrop.get(Slot::Desktop) {
        list.image(desktop.image, bounds);
    }
    if look.own_background {
        layer(list, frame.backdrop, |list| {
            match (look.fill, backdrop.get(Slot::Lock)) {
                (Some(color), _) => list.fill(bounds, CornerRadii::ZERO, color),
                (None, Some(picture)) => list.image(picture.image, bounds),
                (None, None) => {}
            }
        });
    }
    if let Some(blurred) = backdrop.get(Slot::Blurred) {
        layer(list, frame.backdrop, |list| {
            list.image(blurred.image, bounds)
        });
    }
    list.fill(
        bounds,
        CornerRadii::ZERO,
        BLACK.with_alpha_factor(look.dim.clamp(0.0, 1.0) * frame.backdrop),
    );

    let to_preview = size.width / output[0];
    for (kind, style) in [(Layer::Clock, frame.clock), (Layer::Prompt, frame.prompt)] {
        if kind == Layer::Prompt
            && frame.clock.is_visible()
            && let Some(depth) = backdrop.get(Slot::Depth)
        {
            layer(list, frame.clock.opacity, |list| {
                list.image(depth.image, bounds)
            });
        }
        if !style.is_visible() {
            continue;
        }
        layer(list, style.opacity, |list| {
            let [x, y] = style.offset.map(|offset| offset * to_preview);
            list.transform(
                Transform::scale(style.scale, style.scale)
                    .about(bounds.center())
                    .then(Transform::translate(Vector::new(x, y))),
            );
            for sprite in sprites.placed(kind, &style, clock.now) {
                let shake = match sprite.part {
                    Some(Part::SecureField) => frame.shake,
                    _ => 0.0,
                };
                if sprite.morph == LayerStyle::REST {
                    draw_sprite(
                        list,
                        &sprite,
                        sprite.image,
                        sprite.rect,
                        shake,
                        clock,
                        to_preview,
                    );
                    continue;
                }
                morphing(list, &sprite, clock, to_preview);
            }
        });
    }
    if let (Some(edit), Some(mode)) = (clock.edit, clock.handles) {
        handles(list, edit, mode, to_preview);
    }
    list.restore();
}

/// A digit mid-change: scaled about its own centre, and crossing from sharp
/// to soft as its blur grows.
fn morphing(list: &mut DrawList, sprite: &Placed, clock: &Clock<'_>, to_preview: f32) {
    let morph = sprite.morph;
    if !morph.is_visible() {
        return;
    }
    let softness = (morph.blur / MORPH_BLUR).clamp(0.0, 1.0);
    let centre = sprite.rect.center();
    let scaled = |rect: Rect| {
        Rect::from_xywh(
            centre.x + (rect.origin.x - centre.x) * morph.scale,
            centre.y + (rect.origin.y - centre.y) * morph.scale,
            rect.size.width * morph.scale,
            rect.size.height * morph.scale,
        )
    };
    let soft = sprite.soft.filter(|_| softness > 0.0);
    layer(
        list,
        morph.opacity * (1.0 - softness * f32::from(soft.is_some())),
        |list| {
            draw_sprite(
                list,
                sprite,
                sprite.image,
                scaled(sprite.rect),
                0.0,
                clock,
                to_preview,
            );
        },
    );
    if let Some((image, rect)) = soft {
        layer(list, morph.opacity * softness, |list| {
            draw_sprite(list, sprite, image, scaled(rect), 0.0, clock, to_preview);
        });
    }
}

/// `image` over `rect`, through the clock's edit when it is a clock glyph.
fn draw_sprite(
    list: &mut DrawList,
    sprite: &Placed,
    image: ImageId,
    rect: Rect,
    shake: f32,
    clock: &Clock<'_>,
    to_preview: f32,
) {
    if let (Some(group), Some(edit)) = (sprite.clock_group(), clock.edit)
        && !edit.is_identity()
    {
        warped_image(list, image, rect, &edit.map(group), to_preview);
        return;
    }
    list.image(
        image,
        Rect::from_xywh(
            (rect.origin.x + shake) * to_preview,
            rect.origin.y * to_preview,
            rect.size.width * to_preview,
            rect.size.height * to_preview,
        ),
    );
}

/// `draw` composited at `opacity`, as one layer so overlaps don't add up.
fn layer(list: &mut DrawList, opacity: f32, draw: impl FnOnce(&mut DrawList)) {
    if opacity >= 1.0 {
        list.save();
    } else {
        list.push(DrawCommand::SaveLayer {
            bounds: None,
            opacity: opacity.max(0.0),
            backdrop_blur: None,
        });
    }
    draw(list);
    list.restore();
}
