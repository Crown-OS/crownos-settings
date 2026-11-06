//! Small pictures of the clock for the Clock & colour sheet, drawn by the
//! lock screen's own layout and paint in one ink: each layout's clock and
//! date in one window, placed as the layout places them, and each font as
//! the hours alone.

use chrono::{Local, TimeZone};
use crownos_config::schema::ClockLayout;
use lockscreen::clock::ClockFace;
use lockscreen::layout::{ClockGroup, Content, Live, Part, Rect, Sprite};
use lockscreen::lockscene::LockScene;
use lockscreen::tiny_skia::{Pixmap, PixmapPaint, Transform};

use super::{Picture, Studio, StudioEvent};

/// Of the hours' height: the room left around them on a font tile.
const FONT_MARGIN: f32 = 0.12;
/// Of the tallest clock's height: the room left around the clocks on the
/// layout cards.
const LAYOUT_MARGIN: f32 = 0.16;
/// The moment every thumbnail reads, as ColorOS's own cards do: 09:41 on a
/// Monday the 12th.
const SAMPLE_DAY: (i32, u32, u32) = (2026, 10, 12);
const SAMPLE_TIME: (u32, u32) = (9, 41);

/// The two rows of pictures the sheet shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shelf {
    /// The clock and date where each layout puts them on the screen.
    Layouts,
    /// The hours alone, cropped to them.
    Fonts,
}

impl Shelf {
    pub const ALL: [Self; 2] = [Self::Layouts, Self::Fonts];

    fn shows(self, sprite: &Sprite) -> bool {
        let digit = matches!(&sprite.content, Content::Glyph(run) if run.paint.is_face());
        match self {
            Self::Layouts => digit || is_date(sprite),
            Self::Fonts => digit && sprite.clock_group() == Some(ClockGroup::Hours),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ThumbnailRequest {
    pub shelf: Shelf,
    pub scenes: Vec<LockScene>,
    /// The output's logical size, which everything is laid out over.
    pub output: [f32; 2],
    /// Thumbnail pixels across each picture.
    pub pixels: f32,
    /// Straight RGBA everything is drawn in.
    pub ink: [u8; 4],
}

/// Whether `sprite` is the clock's date.
pub fn is_date(sprite: &Sprite) -> bool {
    sprite.part == Some(Part::ClockDate)
}

impl Studio {
    pub(super) fn thumbnails(&mut self, request: &ThumbnailRequest) -> StudioEvent {
        let laid: Vec<_> = request
            .scenes
            .iter()
            .map(|scene| self.lay_out_thumbnail(request, scene))
            .collect();
        let window = common_window(&laid);
        let pictures = laid
            .iter()
            .zip(&request.scenes)
            .map(|(sprites, scene)| {
                let clock = bounds(sprites)?;
                let area = match request.shelf {
                    Shelf::Layouts => placed(window?, clock, scene.config.clock.layout),
                    Shelf::Fonts => grown(clock, clock.height * FONT_MARGIN),
                };
                self.draw_thumbnail(sprites, area, request)
                    .as_ref()
                    .map(Picture::of)
            })
            .collect();
        StudioEvent::Thumbnails(request.shelf, pictures)
    }

    fn lay_out_thumbnail(&mut self, request: &ThumbnailRequest, scene: &LockScene) -> Vec<Sprite> {
        let face = sample_face(scene);
        let live = Live {
            face: &face,
            user: None,
            password_chars: 0,
            checking: false,
            notice: None,
            caps_lock: false,
        };
        self.painter()
            .layout(scene, live, &(), request.output)
            .into_iter()
            .filter(|sprite| request.shelf.shows(sprite))
            .collect()
    }

    /// `sprites` over `area`, `request.pixels` across, in the request's ink.
    fn draw_thumbnail(
        &mut self,
        sprites: &[Sprite],
        area: Rect,
        request: &ThumbnailRequest,
    ) -> Option<Pixmap> {
        let scale = request.pixels / area.width;
        let mut canvas = Pixmap::new(
            request.pixels.ceil() as u32,
            (area.height * scale).ceil() as u32,
        )?;
        let painter = self.painter();
        for sprite in sprites {
            let pad = (sprite.reach() * scale).ceil();
            let Some(mut pixmap) = painter.paint(&sprite.as_coverage(), scale, pad as u32, &())
            else {
                continue;
            };
            ink(&mut pixmap, request.ink);
            canvas.draw_pixmap(
                ((sprite.rect.x - area.x) * scale - pad).round() as i32,
                ((sprite.rect.y - area.y) * scale - pad).round() as i32,
                pixmap.as_ref(),
                &PixmapPaint::default(),
                Transform::identity(),
                None,
            );
        }
        Some(canvas)
    }
}

/// One window every layout's clock fits in, with room around it, so the
/// cards share one shape and show the clocks at one size; and that room.
fn common_window(laid: &[Vec<Sprite>]) -> Option<(Rect, f32)> {
    let [width, height] = laid.iter().filter_map(|sprites| bounds(sprites)).fold(
        None,
        |widest: Option<[f32; 2]>, clock| {
            let [width, height] = widest.unwrap_or_default();
            Some([width.max(clock.width), height.max(clock.height)])
        },
    )?;
    let margin = height * LAYOUT_MARGIN;
    let window = Rect {
        width: width + 2.0 * margin,
        height: height + 2.0 * margin,
        ..Rect::default()
    };
    Some((window, margin))
}

/// `window` over `clock`, `margin` clear of its top and aligned as `layout`
/// aligns the clock on the screen.
fn placed((window, margin): (Rect, f32), clock: Rect, layout: ClockLayout) -> Rect {
    let x = match layout {
        ClockLayout::Centered => clock.x + (clock.width - window.width) / 2.0,
        ClockLayout::Left | ClockLayout::Stacked => clock.x - margin,
    };
    Rect {
        x,
        y: clock.y - margin,
        ..window
    }
}

/// The clock and date as `scene` would read them at the sample moment.
fn sample_face(scene: &LockScene) -> ClockFace {
    let (year, month, day) = SAMPLE_DAY;
    let (hour, minute) = SAMPLE_TIME;
    Local
        .with_ymd_and_hms(year, month, day, hour, minute, 0)
        .single()
        .map_or_else(
            || ClockFace::now(&scene.config),
            |sample| ClockFace::at(&scene.config, sample),
        )
}

/// Every premultiplied pixel of `pixmap` in `ink`, keeping its coverage.
fn ink(pixmap: &mut Pixmap, ink: [u8; 4]) {
    let opacity = u32::from(ink[3]);
    for pixel in pixmap.data_mut().chunks_exact_mut(4) {
        let alpha = (u32::from(pixel[3]) * opacity + 127) / 255;
        for (channel, color) in pixel.iter_mut().zip(ink) {
            *channel = ((u32::from(color) * alpha + 127) / 255) as u8;
        }
        pixel[3] = alpha as u8;
    }
}

fn bounds(sprites: &[Sprite]) -> Option<Rect> {
    let [left, top, right, bottom] = sprites.iter().fold(
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

#[cfg(test)]
mod tests {
    use crownos_config::schema::{ClockFont, ClockLayout, Lock};

    use super::*;

    fn scene(layout: ClockLayout, font: ClockFont) -> LockScene {
        let mut config = Lock::default();
        config.clock.layout = layout;
        config.clock.font = font;
        LockScene {
            config,
            ..LockScene::default()
        }
    }

    #[test]
    fn every_scene_gets_a_picture_cropped_for_its_shelf() {
        let mut studio = Studio::default();
        for shelf in Shelf::ALL {
            let request = ThumbnailRequest {
                shelf,
                scenes: vec![
                    scene(ClockLayout::Centered, ClockFont::Sans),
                    scene(ClockLayout::Stacked, ClockFont::Serif),
                ],
                output: [1920.0, 1080.0],
                pixels: 120.0,
                ink: [20, 20, 20, 255],
            };
            let StudioEvent::Thumbnails(answered, pictures) = studio.thumbnails(&request) else {
                panic!("thumbnails are answered with thumbnails");
            };
            assert_eq!(answered, shelf);
            assert_eq!(pictures.len(), 2);
            let picture = pictures[0].as_ref().expect("painted");
            let inked: Vec<_> = picture
                .rgba
                .chunks(4)
                .filter(|pixel| pixel[3] > 0)
                .collect();
            assert!(!inked.is_empty(), "{shelf:?} draws the clock");
            assert!(
                inked
                    .iter()
                    .filter(|pixel| pixel[3] == u8::MAX)
                    .all(|pixel| pixel[..3] == [20, 20, 20])
            );
            assert_eq!(picture.width, 120);
            if shelf == Shelf::Layouts {
                let stacked = pictures[1].as_ref().expect("painted");
                assert_eq!(picture.height, stacked.height, "the cards share one shape");
            }
        }
    }

    #[test]
    fn ink_keeps_coverage() {
        let mut pixmap = Pixmap::new(1, 1).expect("size");
        pixmap.data_mut().copy_from_slice(&[128, 128, 128, 128]);
        ink(&mut pixmap, [255, 0, 0, 255]);
        assert_eq!(pixmap.data(), &[128, 0, 0, 128]);
    }
}
