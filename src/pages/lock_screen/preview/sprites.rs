//! The lock screen's sprites on the GPU, and blurred copies of them made as
//! a transition reaches each blur radius.
//!
//! Radii are rounded to [`BLUR_STEP`] pixels, so an animation passes through
//! a handful of them and each is blurred once, not once per frame. A clock
//! digit that changes keeps one softened copy of itself and of the digit it
//! replaces, and the two cross between sharp and soft as they trade places.

use std::time::Instant;

use crownos_config::schema::ClockFrame;
use crownui::ext::ImageId;
use crownui::prelude::{Cx, Rect, Runtime};
use lockscreen::clock::{MORPH_BLUR, MORPH_DURATION, morph};
use lockscreen::flow::Frame;
use lockscreen::layout::{ClockGroup, Layer, Part};
use lockscreen::tiny_skia::{Pixmap, PixmapPaint, Transform};
use lockscreen::transition::LayerStyle;

use super::studio::Painted;
use crate::util::texture::Texture;

/// Blur radii, in preview pixels, are made in steps this big.
const BLUR_STEP: f32 = 2.0;
/// Three box-blur passes spread a sprite this many radii past its edge.
const BLUR_REACH: u32 = 3;

#[derive(Default)]
pub struct SpriteSet {
    /// Preview pixels per logical output pixel.
    scale: f32,
    /// What the clock was laid out with.
    pub frame: ClockFrame,
    sprites: Vec<Sprite>,
    /// Clock digits blurring away, and since when.
    leaving: Vec<(Sprite, Instant)>,
}

struct Sprite {
    id: u64,
    glyph: Option<char>,
    /// When this digit took the place of another, while it arrives.
    born: Option<Instant>,
    /// Softened by the morph's whole blur, while it morphs.
    soft: Option<Texture>,
    layer: Layer,
    part: Option<Part>,
    /// Where the content sits, without its padding, in logical output pixels.
    rect: lockscreen::layout::Rect,
    /// Snapped to whole preview pixels, in logical output pixels: the
    /// padded pixmap's corner.
    origin: [f32; 2],
    pixmap: Pixmap,
    sharp: Texture,
    blurred: Vec<(u32, Texture)>,
}

/// A blurred copy still to be uploaded: which sprite, how blurred, and the
/// pixels.
pub type Pending = (usize, u32, Pixmap);

/// One sprite as the canvas draws it.
pub struct Placed {
    pub image: ImageId,
    pub part: Option<Part>,
    /// Logical output pixels.
    pub rect: Rect,
    /// How a morphing digit is drawn on top of its layer's style.
    pub morph: LayerStyle,
    /// The softened copy a morphing digit crosses to, and where it lands.
    pub soft: Option<(ImageId, Rect)>,
}

impl Placed {
    pub fn clock_group(&self) -> Option<ClockGroup> {
        match self.part {
            Some(Part::ClockGlyph(group)) => Some(group),
            _ => None,
        }
    }
}

/// `blur` logical pixels as a whole number of preview pixels, rounded to the
/// step copies are made at.
pub fn quantized(blur: f32, scale: f32) -> u32 {
    ((blur * scale).max(0.0) / BLUR_STEP).round() as u32 * BLUR_STEP as u32
}

impl SpriteSet {
    pub fn upload(cx: &mut Cx, scale: f32, frame: ClockFrame, painted: Vec<Painted>) -> Self {
        let sprites = painted
            .into_iter()
            .map(|painted| Sprite {
                id: painted.id,
                glyph: painted.glyph,
                born: None,
                soft: None,
                layer: painted.layer,
                part: painted.part,
                rect: painted.rect,
                origin: [painted.rect.x, painted.rect.y]
                    .map(|at| ((at - painted.pad) * scale).round() / scale),
                sharp: Texture::from_pixmap(cx, &painted.pixmap),
                pixmap: painted.pixmap,
                blurred: Vec::new(),
            })
            .collect();
        Self {
            scale,
            frame,
            sprites,
            leaving: Vec::new(),
        }
    }

    /// Takes from `old` every digit this set shows another in place of, to
    /// blur away as the new one arrives.
    pub fn hand_over(&mut self, cx: &mut Cx, old: &mut SpriteSet, now: Instant) {
        let radius = (MORPH_BLUR * self.scale).round() as u32;
        self.leaving.append(&mut old.leaving);
        for sprite in &mut self.sprites {
            let Some(arriving) = sprite.glyph else {
                continue;
            };
            let Some(index) = old.sprites.iter().position(|going| {
                going.id == sprite.id && going.glyph.is_some_and(|going| going != arriving)
            }) else {
                continue;
            };
            let mut going = old.sprites.swap_remove(index);
            going.soften(cx, radius);
            self.leaving.push((going, now));
            sprite.soften(cx, radius);
            sprite.born = Some(now);
        }
    }

    pub fn is_morphing(&self, now: Instant) -> bool {
        let running = |since: Instant| now.saturating_duration_since(since) < MORPH_DURATION;
        self.leaving.iter().any(|(_, since)| running(*since))
            || self
                .sprites
                .iter()
                .any(|sprite| sprite.born.is_some_and(running))
    }

    /// Lets go of finished morphs, handing back their textures to free.
    pub fn settle(&mut self, now: Instant) -> Vec<Texture> {
        let running = |since: Instant| now.saturating_duration_since(since) < MORPH_DURATION;
        let (keep, gone): (Vec<_>, Vec<_>) = std::mem::take(&mut self.leaving)
            .into_iter()
            .partition(|(_, since)| running(*since));
        self.leaving = keep;
        let mut unused: Vec<Texture> = gone
            .into_iter()
            .flat_map(|(sprite, _)| sprite.textures())
            .collect();
        for sprite in &mut self.sprites {
            if sprite.born.is_some_and(|born| !running(born)) {
                sprite.born = None;
                unused.extend(sprite.soft.take());
            }
        }
        unused
    }

    /// Where each clock glyph's content sits, in logical output pixels.
    pub fn clock_glyphs(
        &self,
    ) -> impl Iterator<Item = (ClockGroup, lockscreen::layout::Rect)> + Clone + '_ {
        self.sprites.iter().filter_map(|sprite| match sprite.part {
            Some(Part::ClockGlyph(group)) => Some((group, sprite.rect)),
            _ => None,
        })
    }

    pub fn free(self, runtime: &mut Runtime) {
        let leaving = self.leaving.into_iter().map(|(sprite, _)| sprite);
        for texture in self
            .sprites
            .into_iter()
            .chain(leaving)
            .flat_map(Sprite::textures)
        {
            texture.free(runtime);
        }
    }

    /// The blurred copies `frame` shows that are not made yet.
    pub fn missing(&self, frame: &Frame) -> Vec<Pending> {
        self.sprites
            .iter()
            .enumerate()
            .filter_map(|(index, sprite)| {
                let radius = self.radius(style_of(frame, sprite.layer))?;
                if sprite.blurred.iter().any(|(made, _)| *made == radius) {
                    return None;
                }
                Some((index, radius, blurred(&sprite.pixmap, radius)?))
            })
            .collect()
    }

    pub fn adopt(&mut self, made: Vec<(usize, u32, Texture)>) {
        for (index, radius, texture) in made {
            if let Some(sprite) = self.sprites.get_mut(index) {
                sprite.blurred.push((radius, texture));
            }
        }
    }

    /// Takes out every blurred copy `frame` does not show, for freeing.
    pub fn unused(&mut self, frame: &Frame) -> Vec<Texture> {
        let scale = self.scale;
        let mut unused = Vec::new();
        for sprite in &mut self.sprites {
            let style = style_of(frame, sprite.layer);
            let kept = style
                .is_visible()
                .then(|| quantized(style.blur, scale))
                .filter(|radius| *radius > 0);
            let (keep, free) = std::mem::take(&mut sprite.blurred)
                .into_iter()
                .partition(|(radius, _)| Some(*radius) == kept);
            sprite.blurred = keep;
            unused.extend(free.into_iter().map(|(_, texture)| texture));
        }
        unused
    }

    /// `layer`'s sprites as blurred as `style` asks, or as close as is made,
    /// with any digits morphing `now`.
    pub fn placed(
        &self,
        layer: Layer,
        style: &LayerStyle,
        now: Instant,
    ) -> impl Iterator<Item = Placed> + '_ {
        let radius = quantized(style.blur, self.scale);
        let arriving = self
            .sprites
            .iter()
            .filter(move |sprite| sprite.layer == layer)
            .map(move |sprite| {
                let morph = sprite.born.map_or(LayerStyle::REST, |born| {
                    morph(now.saturating_duration_since(born)).1
                });
                self.place(sprite, radius, morph)
            });
        let leaving = self
            .leaving
            .iter()
            .filter(move |(sprite, _)| sprite.layer == layer)
            .map(move |(sprite, since)| {
                let morph = morph(now.saturating_duration_since(*since)).0;
                self.place(sprite, radius, morph)
            });
        arriving.chain(leaving)
    }

    fn place(&self, sprite: &Sprite, radius: u32, morph: LayerStyle) -> Placed {
        let (texture, pad) = sprite
            .blurred
            .iter()
            .find(|(made, _)| *made == radius)
            .map_or((&sprite.sharp, 0.0), |(made, texture)| {
                (texture, (made * BLUR_REACH) as f32)
            });
        let rect = |texture: &Texture, pad: f32| {
            let pad = pad / self.scale;
            Rect::from_xywh(
                sprite.origin[0] - pad,
                sprite.origin[1] - pad,
                texture.size.width / self.scale,
                texture.size.height / self.scale,
            )
        };
        let soft_pad = (MORPH_BLUR * self.scale).round() * BLUR_REACH as f32;
        Placed {
            image: texture.image,
            part: sprite.part,
            rect: rect(texture, pad),
            morph,
            soft: sprite
                .soft
                .as_ref()
                .map(|soft| (soft.image, rect(soft, soft_pad))),
        }
    }

    fn radius(&self, style: &LayerStyle) -> Option<u32> {
        let radius = quantized(style.blur, self.scale);
        (style.is_visible() && radius > 0).then_some(radius)
    }
}

impl Sprite {
    fn soften(&mut self, cx: &mut Cx, radius: u32) {
        if self.soft.is_none()
            && let Some(soft) = blurred(&self.pixmap, radius)
        {
            self.soft = Some(Texture::from_pixmap(cx, &soft));
        }
    }

    fn textures(self) -> impl Iterator<Item = Texture> {
        std::iter::once(self.sharp)
            .chain(self.soft)
            .chain(self.blurred.into_iter().map(|(_, texture)| texture))
    }
}

fn style_of(frame: &Frame, layer: Layer) -> &LayerStyle {
    match layer {
        Layer::Clock => &frame.clock,
        Layer::Prompt => &frame.prompt,
    }
}

/// `pixmap` blurred by `radius` pixels, with room around it to spread into.
fn blurred(pixmap: &Pixmap, radius: u32) -> Option<Pixmap> {
    let pad = radius * BLUR_REACH;
    let mut spread = Pixmap::new(pixmap.width() + 2 * pad, pixmap.height() + 2 * pad)?;
    spread.draw_pixmap(
        pad as i32,
        pad as i32,
        pixmap.as_ref(),
        &PixmapPaint::default(),
        Transform::identity(),
        None,
    );
    lockscreen::image::blur(&mut spread, radius as usize);
    Some(spread)
}

#[cfg(test)]
mod tests {
    use lockscreen::tiny_skia::Color;

    use super::*;

    #[test]
    fn radii_snap_to_even_preview_pixels() {
        assert_eq!(quantized(0.0, 0.5), 0);
        assert_eq!(quantized(1.9, 0.5), 0);
        assert_eq!(quantized(2.0, 0.5), 2);
        assert_eq!(quantized(24.0, 0.35), 8);
        assert_eq!(quantized(-4.0, 1.0), 0);
    }

    #[test]
    fn a_blurred_copy_has_room_to_spread() {
        let mut dot = Pixmap::new(4, 4).expect("size");
        dot.fill(Color::WHITE);
        let spread = blurred(&dot, 2).expect("blurs");
        assert_eq!((spread.width(), spread.height()), (16, 16));
        let edge = spread.pixel(3, 8).expect("inside");
        assert!(edge.alpha() > 0, "the blur reaches into the padding");
    }
}
