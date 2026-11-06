//! The preview, drawn: what the compositor stacks on an output, scaled to
//! the card — the desktop, the lock's background and its darkening, the
//! clock, the background's subject in front of it, and the prompt.

use std::time::Instant;

use crownui::ext::{DrawCommand, DrawList, ImageId, Transform};
use crownui::prelude::{BLACK, Color, CornerRadii, Point, Rect, Size, Vector};
use lockscreen::clock::MORPH_BLUR;
use lockscreen::flow::Frame;
use lockscreen::layout::{Layer, Part};
use lockscreen::transition::{LayerStyle, PROMPT_BLUR};

use super::backdrop::Backdrop;
use super::editor::{ClockEdit, EditMode, handles, mapped_rect, warped_image};
use super::sprites::{Placed, SpriteSet};
use super::studio::{DESKTOP_ZOOM, Slot};

pub const CORNER_RADIUS: f32 = 12.0;

/// What the lock's settings say about the background, apart from pictures.
pub struct Look {
    /// The lock has a background of its own, over the desktop.
    pub own_background: bool,
    /// That background is a flat color.
    pub fill: Option<Color>,
    pub dim: f32,
    /// The background's subject stands in front of the clock.
    pub depth: bool,
}

/// The clock as the editor has it.
pub struct Clock<'a> {
    /// The laid-out clock, shown with the look being edited.
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
    background(list, bounds, look, backdrop, frame);
    let to_preview = size.width / output[0];
    sprite_layer(
        list,
        bounds,
        Layer::Clock,
        sprites,
        frame,
        clock,
        to_preview,
    );
    if let Some(subject) = backdrop.get(Slot::Subject).filter(|_| look.depth) {
        let style = frame.subject();
        let fading = LayerStyle {
            opacity: style.opacity * (1.0 - blurred_share(&style)),
            ..style
        };
        wallpaper_layer(list, bounds, fading, |list| {
            list.image(subject.image, look.lock_area(bounds));
        });
    }
    sprite_layer(
        list,
        bounds,
        Layer::Prompt,
        sprites,
        frame,
        clock,
        to_preview,
    );
    if let (Some(edit), Some(mode)) = (clock.edit, clock.handles) {
        handles(list, edit, mode, to_preview);
    }
    list.restore();
}

/// What the lock is drawn over: black, as the compositor's floor, with the
/// desktop showing only as the lock dissolves into it; the lock's background
/// zoomed, faded and blurred as the frame has it; and the darkening over all.
fn background(list: &mut DrawList, bounds: Rect, look: &Look, backdrop: &Backdrop, frame: &Frame) {
    list.fill(bounds, CornerRadii::ZERO, BLACK);
    let desktop = backdrop.get(Slot::Desktop);
    if let Some(desktop) = desktop.filter(|_| frame.wallpaper.opacity < 1.0) {
        list.image(desktop.image, zoomed(bounds, DESKTOP_ZOOM));
    }
    let picture = match look.own_background {
        true => backdrop.get(Slot::Lock),
        false => desktop,
    };
    let wallpaper = frame.wallpaper;
    let blurred = blurred_share(&wallpaper);
    let area = look.lock_area(bounds);
    wallpaper_layer(list, bounds, wallpaper, |list| {
        match (look.fill, picture) {
            (Some(color), _) => list.fill(bounds, CornerRadii::ZERO, color),
            (None, Some(picture)) => list.image(picture.image, area),
            (None, None) => {}
        }
        if let Some(soft) = backdrop.get(Slot::Blurred).filter(|_| blurred > 0.0) {
            layer(list, blurred, |list| list.image(soft.image, area));
        }
    });
    list.fill(
        bounds,
        CornerRadii::ZERO,
        BLACK.with_alpha_factor(frame.darkening(look.dim)),
    );
}

impl Look {
    /// Where the lock's background and what is cut from it land: the
    /// desktop wallpaper zoomed as crownpaper zooms it, a picture of the
    /// lock's own covering the screen.
    fn lock_area(&self, bounds: Rect) -> Rect {
        match self.own_background {
            true => bounds,
            false => zoomed(bounds, DESKTOP_ZOOM),
        }
    }
}

/// `bounds` grown by `zoom` about its centre.
fn zoomed(bounds: Rect, zoom: f32) -> Rect {
    let size = Size::new(bounds.size.width * zoom, bounds.size.height * zoom);
    let centre = bounds.center();
    Rect::new(
        Point::new(centre.x - size.width / 2.0, centre.y - size.height / 2.0),
        size,
    )
}

/// How far `style` has blurred the background, as a share of the prompt's
/// blur: the sharp copy fades into the blurred one by it.
fn blurred_share(style: &LayerStyle) -> f32 {
    (style.blur / PROMPT_BLUR).clamp(0.0, 1.0)
}

/// `draw` at the background's place: zoomed about the output's centre and
/// faded as `style` has it.
fn wallpaper_layer(
    list: &mut DrawList,
    bounds: Rect,
    style: LayerStyle,
    draw: impl FnOnce(&mut DrawList),
) {
    if !style.is_visible() {
        return;
    }
    layer(list, style.opacity, |list| {
        list.transform(Transform::scale(style.scale, style.scale).about(bounds.center()));
        draw(list);
    });
}

fn sprite_layer(
    list: &mut DrawList,
    bounds: Rect,
    kind: Layer,
    sprites: &SpriteSet,
    frame: &Frame,
    clock: &Clock<'_>,
    to_preview: f32,
) {
    let style = match kind {
        Layer::Clock => frame.clock,
        Layer::Prompt => frame.prompt,
    };
    if !style.is_visible() {
        return;
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

/// `image` over `rect`, carried through the clock's edit when it is a clock
/// glyph or the clock's date: one image while the edit only moves and
/// scales them, cut to follow a warp.
fn draw_sprite(
    list: &mut DrawList,
    sprite: &Placed,
    image: ImageId,
    rect: Rect,
    shake: f32,
    clock: &Clock<'_>,
    to_preview: f32,
) {
    if let Some(edit) = clock.edit.filter(|edit| !edit.is_identity()) {
        let map = match sprite.clock_group() {
            Some(group) if edit.is_warped() => {
                return warped_image(list, image, rect, &edit.map(group), to_preview);
            }
            Some(group) => Some(edit.map(group)),
            None => sprite.date.then(|| edit.date_map()),
        };
        if let Some(moved) = map.and_then(|map| mapped_rect(rect, &map, to_preview)) {
            list.image(image, moved);
            return;
        }
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
