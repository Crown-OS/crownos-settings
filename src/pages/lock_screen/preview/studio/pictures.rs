//! The pictures under the lock: decoded once per size, and sent only when
//! what a slot shows has changed.

use std::io::Read;
use std::path::Path;

use crownos_config::schema::LockBackground;
use lockscreen::image::{Mask, blurred, cover, darken, decode, subject};
use lockscreen::lockscene::Limits;
use lockscreen::tiny_skia::{FilterQuality, Pixmap, PixmapPaint, Transform};
use lockscreen::transition::PROMPT_BLUR;

use super::{BackdropRequest, DepthState, Picture, Slot, Studio, StudioEvent};

/// A picture file decoded and covering the preview, or `None` when it could
/// not be read.
pub(super) struct Source {
    path: String,
    pixels: [u32; 2],
    pixmap: Option<Pixmap>,
}

/// What a slot was last sent: the file it shows, at what size, and how
/// strongly it is blurred or dimmed.
pub(super) type SlotKey = (String, [u32; 2], u32);

/// The desktop slot and everything cut from it are drawn at crownpaper's
/// zoom; a lock picture is not.
pub use lockscreen::image::DESKTOP_WALLPAPER_ZOOM as DESKTOP_ZOOM;

impl Studio {
    pub(super) fn backdrop(&mut self, request: &BackdropRequest, events: &mut Vec<StudioEvent>) {
        let BackdropRequest {
            desktop,
            background,
            pixels,
            output,
            depth,
            dim,
        } = request;
        let desktop = named(desktop);
        let picture = match background {
            LockBackground::Image(path) => named(path),
            LockBackground::Desktop | LockBackground::Token(_) => None,
        };
        let lock = match background {
            LockBackground::Desktop => desktop,
            LockBackground::Image(_) => picture,
            LockBackground::Token(_) => None,
        };
        let mask = self.depth_mask(lock, *depth);
        self.sources.retain(|source| {
            source.pixels == *pixels && [desktop, picture].contains(&Some(source.path.as_str()))
        });
        let prompt_blur = PROMPT_BLUR * pixels[0] as f32 / output[0].max(1) as f32;
        let key = |path: &str, strength: f32| (path.to_owned(), *pixels, strength.to_bits());
        for slot in Slot::ALL {
            let wanted = match slot {
                Slot::Desktop => desktop.map(|path| key(path, 0.0)),
                Slot::Lock => picture.map(|path| key(path, 0.0)),
                Slot::Blurred => lock.map(|path| key(path, prompt_blur)),
                Slot::Subject => mask
                    .as_deref()
                    .map(|mask| key(&mask.to_string_lossy(), *dim)),
            };
            if self.sent[slot as usize] == wanted {
                continue;
            }
            let shown = match slot {
                Slot::Desktop => self.picture_of(desktop, *pixels),
                Slot::Lock => self.picture_of(picture, *pixels),
                Slot::Blurred => lock
                    .and_then(|path| self.covered(path, *pixels))
                    .and_then(|covered| blurred(covered, pixels[0], pixels[1], prompt_blur))
                    .as_ref()
                    .map(Picture::of),
                Slot::Subject => {
                    self.subject = self.cut_subject(lock, mask.as_deref(), *pixels, *dim);
                    self.subject.as_ref().map(Picture::of)
                }
            };
            events.push(StudioEvent::Picture(slot, shown));
            self.sent[slot as usize] = wanted;
        }
        let unreadable = picture.is_some_and(|path| self.covered(path, *pixels).is_none());
        events.push(StudioEvent::PictureUnreadable(unreadable));
    }

    fn picture_of(&mut self, path: Option<&str>, pixels: [u32; 2]) -> Option<Picture> {
        self.covered(path?, pixels).map(Picture::of)
    }

    /// The lock's background cut down to its subject by `mask`, dimmed as the
    /// background under it is.
    fn cut_subject(
        &mut self,
        lock: Option<&str>,
        mask: Option<&Path>,
        pixels: [u32; 2],
        dim: f32,
    ) -> Option<Pixmap> {
        let Some(mask) = read_limited(mask?).and_then(|bytes| Mask::decode(&bytes).ok()) else {
            self.depth = DepthState::Unavailable;
            return None;
        };
        let mut cut = subject(self.covered(lock?, pixels)?, &mask);
        darken(&mut cut, dim);
        Some(cut)
    }

    /// `path` decoded and cut to cover `pixels`, decoding it only the first
    /// time it is asked for at that size.
    pub(super) fn covered(&mut self, path: &str, pixels: [u32; 2]) -> Option<&Pixmap> {
        let known = self
            .sources
            .iter()
            .position(|source| source.path == path && source.pixels == pixels);
        let index = known.unwrap_or_else(|| {
            let pixmap = read_limited(Path::new(path))
                .and_then(|bytes| decode(&bytes).ok())
                .and_then(|picture| cover(&picture, pixels[0], pixels[1]));
            self.sources.push(Source {
                path: path.to_owned(),
                pixels,
                pixmap,
            });
            self.sources.len() - 1
        });
        self.sources[index].pixmap.as_ref()
    }
}

/// `path`, unless it names nothing.
pub(super) fn named(path: &str) -> Option<&str> {
    Some(path).filter(|path| !path.is_empty())
}

/// The file at `path`, unless it is bigger than the lock screen would read.
fn read_limited(path: &Path) -> Option<Vec<u8>> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .ok()?
        .take(Limits::ASSET_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    (bytes.len() <= Limits::ASSET_BYTES).then_some(bytes)
}

/// `pixmap` zoomed by `zoom` about its centre, cropped to its own size.
pub(super) fn zoomed(pixmap: &Pixmap, zoom: f32) -> Option<Pixmap> {
    let mut zoomed = Pixmap::new(pixmap.width(), pixmap.height())?;
    let [x, y] = [pixmap.width(), pixmap.height()].map(|side| side as f32 * (1.0 - zoom) / 2.0);
    let paint = PixmapPaint {
        quality: FilterQuality::Bilinear,
        ..PixmapPaint::default()
    };
    zoomed.draw_pixmap(
        0,
        0,
        pixmap.as_ref(),
        &paint,
        Transform::from_row(zoom, 0.0, 0.0, zoom, x, y),
        None,
    );
    Some(zoomed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pictures(events: &[StudioEvent]) -> Vec<(Slot, bool)> {
        events
            .iter()
            .filter_map(|event| match event {
                StudioEvent::Picture(slot, picture) => Some((*slot, picture.is_some())),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn only_slots_whose_picture_changed_are_sent_again() {
        let dir = std::env::temp_dir().join(format!("crownsettings-studio-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let wallpaper = dir.join("wallpaper.png");
        image::RgbaImage::from_pixel(32, 18, image::Rgba([40, 80, 160, 255]))
            .save(&wallpaper)
            .expect("writes a png");

        let mut studio = Studio::default();
        let request = BackdropRequest {
            desktop: wallpaper.to_string_lossy().into_owned(),
            background: LockBackground::Desktop,
            pixels: [64, 36],
            output: [192, 108],
            depth: false,
            dim: 0.0,
        };
        let mut events = Vec::new();
        studio.backdrop(&request, &mut events);
        assert_eq!(
            pictures(&events),
            [(Slot::Desktop, true), (Slot::Blurred, true)],
            "the lock's own slot starts empty and stays so"
        );

        events.clear();
        studio.backdrop(&request, &mut events);
        assert!(pictures(&events).is_empty());

        events.clear();
        let missing = BackdropRequest {
            background: LockBackground::Image("/nonexistent/lock.png".to_owned()),
            ..request
        };
        studio.backdrop(&missing, &mut events);
        assert_eq!(
            pictures(&events),
            [(Slot::Lock, false), (Slot::Blurred, false)]
        );
        assert!(matches!(
            events.last(),
            Some(StudioEvent::PictureUnreadable(true))
        ));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
