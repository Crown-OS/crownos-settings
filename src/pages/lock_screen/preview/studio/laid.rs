//! The layout the canvas has, as the studio remembers it: which pictures it
//! already holds, so a layout that changes only the clock paints only the
//! clock, and the coverage of each glass face, so the glass is frosted again
//! wherever the clock is shown without painting it again.

use crownos_config::schema::ClockFrame;
use lockscreen::clock::{Glass, Homography};
use lockscreen::layout::{Content, Rect, Sprite};
use lockscreen::paint::Painter;
use lockscreen::tiny_skia::Pixmap;

use super::super::editor::{ClockEdit, corners};
use super::glass::{Pane, frost};
use super::thumbnails::is_date;
use super::{LayoutRequest, Painted};

pub(super) struct Laid {
    /// Counts layouts from the studio's first, so the canvas can tell
    /// which one a frost is for.
    pub layout: u64,
    pub frame: ClockFrame,
    scale: f32,
    output: [f32; 2],
    sprites: Vec<Sprite>,
    faces: Vec<Face>,
}

/// A glass face: which sprite, its padding in preview pixels, and its
/// white coverage, which the glass is filled into.
struct Face {
    index: usize,
    pad: f32,
    glass: Glass,
    coverage: Pixmap,
}

/// What the last layout painted of a sprite that has not changed since.
enum Painting {
    /// The picture the canvas holds.
    Held,
    /// A glass face's coverage.
    Coverage(Pixmap),
}

impl Laid {
    pub fn new(layout: u64, request: &LayoutRequest, sprites: Vec<Sprite>) -> Self {
        Self {
            layout,
            frame: request.scene.config.clock.frame,
            scale: request.scale,
            output: request.output,
            sprites,
            faces: Vec::new(),
        }
    }

    /// How many glass faces are frosted.
    #[cfg(test)]
    pub fn frosted_count(&self) -> usize {
        self.faces.len()
    }

    /// Every sprite rasterised, but those `previous` painted the same: the
    /// canvas keeps its own picture of them. Glass faces are painted as
    /// their coverage, for [`Laid::frosted`] to fill, when there is a
    /// backdrop to frost them with.
    pub fn paint(
        &mut self,
        painter: &mut Painter,
        mut previous: Option<Laid>,
        frosting: bool,
    ) -> Vec<Painted> {
        let scale = self.scale;
        let mut painted = Vec::with_capacity(self.sprites.len());
        for (index, sprite) in self.sprites.iter().enumerate() {
            let pad = (sprite.reach() * scale).ceil();
            let kept = previous
                .as_mut()
                .and_then(|previous| previous.take_same(sprite, scale));
            let pixmap = match (glass_of(sprite).filter(|_| frosting), kept) {
                (Some(glass), kept) => {
                    let coverage = match kept {
                        Some(Painting::Coverage(coverage)) => Some(coverage),
                        _ => painter.paint(&sprite.as_coverage(), scale, pad as u32, &()),
                    };
                    let Some(coverage) = coverage else {
                        continue;
                    };
                    self.faces.push(Face {
                        index,
                        pad,
                        glass,
                        coverage,
                    });
                    None
                }
                (None, Some(Painting::Held)) => None,
                (None, _) => {
                    let Some(pixmap) = painter.paint(sprite, scale, pad as u32, &()) else {
                        continue;
                    };
                    Some(pixmap)
                }
            };
            painted.push(Painted {
                id: sprite.id,
                layer: sprite.layer,
                part: sprite.part,
                date: is_date(sprite),
                rect: sprite.rect,
                pad: pad / scale,
                glyph: match &sprite.content {
                    Content::Glyph(run) => Some(run.ch),
                    _ => None,
                },
                pixmap,
            });
        }
        painted
    }

    /// What this layout painted of `sprite`, if it laid out the same sprite
    /// at `scale`: only its place may have changed.
    fn take_same(&mut self, sprite: &Sprite, scale: f32) -> Option<Painting> {
        if self.scale != scale {
            return None;
        }
        let index = self.sprites.iter().position(|known| {
            known.id == sprite.id
                && known.content == sprite.content
                && [known.rect.width, known.rect.height] == [sprite.rect.width, sprite.rect.height]
        })?;
        match self.faces.iter().position(|face| face.index == index) {
            Some(face) => Some(Painting::Coverage(self.faces.swap_remove(face).coverage)),
            None => glass_of(sprite).is_none().then_some(Painting::Held),
        }
    }

    /// The glass faces filled with `backdrop` where the clock stands when
    /// shown with `shown`.
    pub fn frosted(&self, backdrop: &Pixmap, shown: ClockFrame) -> Vec<(u64, Pixmap)> {
        let edit = self.edit(shown);
        let to_backdrop = backdrop.width() as f32 / self.output[0].max(1.0);
        self.faces
            .iter()
            .filter_map(|face| {
                let sprite = &self.sprites[face.index];
                let corner = sprite
                    .rect
                    .padded_origin(self.scale, face.pad)
                    .map(|at| at / self.scale);
                let pane = Pane {
                    to_backdrop: Homography::affine([to_backdrop; 2], [0.0; 2])
                        .after(self.map_of(edit.as_ref(), sprite))
                        .after(Homography::affine([1.0 / self.scale; 2], corner)),
                    rows: [face.pad, face.pad + sprite.rect.height * self.scale],
                    glass: face.glass,
                };
                Some((sprite.id, frost(&face.coverage, backdrop, &pane)?))
            })
            .collect()
    }

    /// The box around the glass digits shown with `shown`, in logical pixels.
    pub fn clock_area(&self, shown: ClockFrame) -> Option<Rect> {
        let edit = self.edit(shown);
        let corners = self
            .sprites
            .iter()
            .filter(|sprite| glass_of(sprite).is_some())
            .flat_map(|sprite| {
                let map = self.map_of(edit.as_ref(), sprite);
                corners(sprite.rect).map(|corner| map.apply(corner).unwrap_or(corner))
            });
        let [left, top, right, bottom] = corners.fold(
            [f32::MAX, f32::MAX, f32::MIN, f32::MIN],
            |[l, t, r, b], [x, y]| [l.min(x), t.min(y), r.max(x), b.max(y)],
        );
        (left < right && top < bottom).then_some(Rect {
            x: left,
            y: top,
            width: right - left,
            height: bottom - top,
        })
    }

    fn edit(&self, shown: ClockFrame) -> Option<ClockEdit> {
        let glyphs = self
            .sprites
            .iter()
            .filter_map(|sprite| Some((sprite.clock_group()?, sprite.rect)));
        ClockEdit::new(self.frame, shown, glyphs, None)
    }

    /// Where `sprite`, laid out, is shown.
    fn map_of(&self, edit: Option<&ClockEdit>, sprite: &Sprite) -> Homography {
        edit.zip(sprite.clock_group())
            .map_or(Homography::IDENTITY, |(edit, group)| edit.map(group))
    }
}

/// The tallest glass digit, in logical pixels.
pub(super) fn tallest_glass(sprites: &[Sprite]) -> f32 {
    sprites
        .iter()
        .filter(|sprite| glass_of(sprite).is_some())
        .map(|sprite| sprite.rect.height)
        .fold(0.0, f32::max)
}

/// The glass a glass digit is filled with.
fn glass_of(sprite: &Sprite) -> Option<Glass> {
    match &sprite.content {
        Content::Glyph(run) => run.paint.glass(),
        _ => None,
    }
}
