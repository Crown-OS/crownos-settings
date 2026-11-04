//! Small pictures of the clock in other looks, each drawn the way the lock
//! screen would draw it — warp included — in one picture per look.

use lockscreen::clock::{ClockFace, Homography};
use lockscreen::layout::{Layer, Live, Rect, Sprite};
use lockscreen::lockscene::LockScene;
use lockscreen::tiny_skia::{FilterQuality, Pixmap, PixmapPaint, Transform};

use super::{Picture, Studio, StudioEvent};

/// Of the clock's height: the room left around a face.
const FACE_MARGIN: f32 = 0.08;

/// The two rows of thumbnails the page shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shelf {
    /// The whole clock screen, as it sits over the background.
    Looks,
    /// The clock's digits alone, cropped to them.
    Faces,
}

impl Shelf {
    pub const ALL: [Self; 2] = [Self::Looks, Self::Faces];
}

#[derive(Debug, Clone, PartialEq)]
pub struct ThumbnailRequest {
    pub shelf: Shelf,
    pub scenes: Vec<LockScene>,
    /// The output's logical size, which everything is laid out over.
    pub output: [f32; 2],
    /// Thumbnail pixels per logical output pixel.
    pub scale: f32,
}

impl Studio {
    pub(super) fn thumbnails(&mut self, request: &ThumbnailRequest) -> StudioEvent {
        let painter = self.painter();
        let pictures = request
            .scenes
            .iter()
            .map(|scene| {
                let face = ClockFace::now(&scene.config);
                let live = Live {
                    face: &face,
                    user: None,
                    password_chars: 0,
                    checking: false,
                    notice: None,
                    caps_lock: false,
                };
                let sprites: Vec<_> = painter
                    .layout(scene, live, &(), request.output)
                    .into_iter()
                    .filter(|sprite| match request.shelf {
                        Shelf::Looks => sprite.layer == Layer::Clock,
                        Shelf::Faces => sprite.clock_group().is_some(),
                    })
                    .collect();
                let clock = clock_box(&sprites)?;
                let area = match request.shelf {
                    Shelf::Looks => Rect {
                        width: request.output[0],
                        height: request.output[1],
                        ..Rect::default()
                    },
                    Shelf::Faces => grown(clock, clock.height * FACE_MARGIN),
                };
                let warp = warp_of(clock, scene.config.clock.frame.warp);
                let mut canvas = Pixmap::new(
                    (area.width * request.scale).ceil() as u32,
                    (area.height * request.scale).ceil() as u32,
                )?;
                for sprite in &sprites {
                    let pad = (sprite.reach() * request.scale).ceil();
                    let Some(pixmap) = painter.paint(sprite, request.scale, pad as u32, &()) else {
                        continue;
                    };
                    let bend = warp.filter(|_| sprite.clock_group().is_some());
                    let Some(place) = placement(
                        sprite.rect,
                        pad / request.scale,
                        area,
                        request.scale,
                        &pixmap,
                        bend.as_ref(),
                    ) else {
                        continue;
                    };
                    canvas.draw_pixmap(0, 0, pixmap.as_ref(), &SMOOTH, place, None);
                }
                Some(Picture::of(&canvas))
            })
            .collect();
        StudioEvent::Thumbnails(request.shelf, pictures)
    }
}

const SMOOTH: PixmapPaint = PixmapPaint {
    opacity: 1.0,
    blend_mode: lockscreen::tiny_skia::BlendMode::SourceOver,
    quality: FilterQuality::Bilinear,
};

/// Where a sprite's padded pixmap lands on a canvas over `area`: bent through
/// `warp` by the affine its own corners follow, which is close to the warp
/// across anything as small as a digit.
fn placement(
    rect: Rect,
    pad: f32,
    area: Rect,
    scale: f32,
    pixmap: &Pixmap,
    warp: Option<&Homography>,
) -> Option<Transform> {
    let (left, top) = (rect.x - pad, rect.y - pad);
    let (right, bottom) = (rect.x + rect.width + pad, rect.y + rect.height + pad);
    let on_canvas = |point: [f32; 2]| -> Option<[f32; 2]> {
        let [x, y] = match warp {
            Some(warp) => warp.apply(point)?,
            None => point,
        };
        Some([(x - area.x) * scale, (y - area.y) * scale])
    };
    let origin = on_canvas([left, top])?;
    let across = on_canvas([right, top])?;
    let down = on_canvas([left, bottom])?;
    let (width, height) = (pixmap.width() as f32, pixmap.height() as f32);
    Some(Transform::from_row(
        (across[0] - origin[0]) / width,
        (across[1] - origin[1]) / width,
        (down[0] - origin[0]) / height,
        (down[1] - origin[1]) / height,
        origin[0],
        origin[1],
    ))
}

/// The box around the clock's digits, which the warp bends.
fn clock_box(sprites: &[Sprite]) -> Option<Rect> {
    let digits = sprites
        .iter()
        .filter(|sprite| sprite.clock_group().is_some());
    let [left, top, right, bottom] = digits.fold(
        [f32::MAX, f32::MAX, f32::MIN, f32::MIN],
        |[left, top, right, bottom], sprite| {
            let rect = sprite.rect;
            [
                left.min(rect.x),
                top.min(rect.y),
                right.max(rect.x + rect.width),
                bottom.max(rect.y + rect.height),
            ]
        },
    );
    (right > left && bottom > top).then_some(Rect {
        x: left,
        y: top,
        width: right - left,
        height: bottom - top,
    })
}

fn grown(rect: Rect, by: f32) -> Rect {
    Rect {
        x: rect.x - by,
        y: rect.y - by,
        width: rect.width + 2.0 * by,
        height: rect.height + 2.0 * by,
    }
}

/// The clock's box pulled by `warp`, as the compositor bends it.
fn warp_of(clock: Rect, warp: [[f32; 2]; 4]) -> Option<Homography> {
    if warp.iter().flatten().all(|pull| *pull == 0.0) {
        return None;
    }
    let corners = [
        [clock.x, clock.y],
        [clock.x + clock.width, clock.y],
        [clock.x + clock.width, clock.y + clock.height],
        [clock.x, clock.y + clock.height],
    ];
    let quad = std::array::from_fn(|index| {
        let ([x, y], [dx, dy]) = (corners[index], warp[index]);
        [x + dx * clock.width, y + dy * clock.height]
    });
    Homography::box_to_quad([clock.x, clock.y], [clock.width, clock.height], quad)
}

#[cfg(test)]
mod tests {
    use crownos_config::schema::{ClockFont, Lock};

    use super::*;

    fn scene(font: ClockFont, warp: [[f32; 2]; 4]) -> LockScene {
        let mut config = Lock::default();
        config.clock.font = font;
        config.clock.frame.warp = warp;
        LockScene {
            config,
            ..LockScene::default()
        }
    }

    #[test]
    fn every_scene_gets_a_picture_cropped_for_its_shelf() {
        let lean = [[0.14, 0.0], [0.14, 0.0], [0.0; 2], [0.0; 2]];
        let mut studio = Studio::default();
        for shelf in Shelf::ALL {
            let request = ThumbnailRequest {
                shelf,
                scenes: vec![
                    scene(ClockFont::Tall, [[0.0; 2]; 4]),
                    scene(ClockFont::Serif, lean),
                ],
                output: [1920.0, 1080.0],
                scale: 0.1,
            };
            let StudioEvent::Thumbnails(answered, pictures) = studio.thumbnails(&request) else {
                panic!("thumbnails are answered with thumbnails");
            };
            assert_eq!(answered, shelf);
            assert_eq!(pictures.len(), 2);
            let picture = pictures[0].as_ref().expect("painted");
            let inked = picture.rgba.chunks(4).filter(|pixel| pixel[3] > 0).count();
            assert!(inked > 0, "{shelf:?} draws the clock");
            match shelf {
                Shelf::Looks => assert_eq!(picture.width, 192),
                Shelf::Faces => assert!(picture.width < 192),
            }
        }
    }
}
