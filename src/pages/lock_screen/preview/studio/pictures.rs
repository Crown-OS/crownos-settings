//! The pictures under the lock: decoded once per size, and sent only when
//! what a slot shows has changed.

use std::io::Read;

use crownos_config::schema::LockBackground;
use lockscreen::image::{backdrop, cover, darken, decode};
use lockscreen::lockscene::Limits;
use lockscreen::tiny_skia::Pixmap;

use super::{BackdropRequest, Picture, Slot, Studio, StudioEvent};

/// A picture file decoded and covering the preview, or `None` when it could
/// not be read.
pub(super) struct Source {
    path: String,
    pixels: [u32; 2],
    pixmap: Option<Pixmap>,
}

/// Path, preview pixels, output size and blur strength.
pub(super) type SlotKey = (String, [u32; 2], [u32; 2], u32);

impl Studio {
    pub(super) fn backdrop(&mut self, request: &BackdropRequest, events: &mut Vec<StudioEvent>) {
        let BackdropRequest {
            desktop,
            background,
            pixels,
            output,
            blur,
            depth,
            dim,
        } = request;
        let desktop = named(desktop);
        let picture = match background {
            LockBackground::Image(path) => named(path),
            LockBackground::Desktop | LockBackground::Token(_) => None,
        };
        let blurred = match background {
            LockBackground::Desktop => desktop,
            LockBackground::Image(_) => picture,
            LockBackground::Token(_) => None,
        };
        let depth = depth.as_deref().and_then(named);
        self.sources.retain(|source| {
            source.pixels == *pixels
                && [desktop, picture, depth].contains(&Some(source.path.as_str()))
        });
        let wanted = [
            (desktop, 0.0),
            (picture, 0.0),
            (blurred, *blur),
            (depth, *dim),
        ];
        for (slot, (path, strength)) in Slot::ALL.into_iter().zip(wanted) {
            let key = path.map(|path| (path.to_owned(), *pixels, *output, strength.to_bits()));
            if self.sent[slot as usize] == key {
                continue;
            }
            let shown = path.and_then(|path| {
                let covered = self.covered(path, *pixels)?;
                match slot {
                    Slot::Blurred => backdrop(covered, output[0], output[1], strength)
                        .as_ref()
                        .map(Picture::of),
                    Slot::Desktop | Slot::Lock => Some(Picture::of(covered)),
                    Slot::Depth => {
                        let mut dimmed = covered.clone();
                        darken(&mut dimmed, strength);
                        Some(Picture::of(&dimmed))
                    }
                }
            });
            events.push(StudioEvent::Picture(slot, shown));
            self.sent[slot as usize] = key;
        }
        let unreadable = picture.is_some_and(|path| self.covered(path, *pixels).is_none());
        events.push(StudioEvent::PictureUnreadable(unreadable));
    }

    /// `path` decoded and cut to cover `pixels`, decoding it only the first
    /// time it is asked for at that size.
    fn covered(&mut self, path: &str, pixels: [u32; 2]) -> Option<&Pixmap> {
        let known = self
            .sources
            .iter()
            .position(|source| source.path == path && source.pixels == pixels);
        let index = known.unwrap_or_else(|| {
            let pixmap = read_limited(path)
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
fn named(path: &str) -> Option<&str> {
    Some(path).filter(|path| !path.is_empty())
}

/// The file at `path`, unless it is bigger than the lock screen would read.
fn read_limited(path: &str) -> Option<Vec<u8>> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .ok()?
        .take(Limits::ASSET_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    (bytes.len() <= Limits::ASSET_BYTES).then_some(bytes)
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
            output: [1920, 1080],
            blur: 1.0,
            depth: None,
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
