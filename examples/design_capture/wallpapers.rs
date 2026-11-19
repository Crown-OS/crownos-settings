//! The stock CrownOS wallpapers, as the picker's grid shows them.

use std::path::Path;
use std::sync::OnceLock;

use crownui::prelude::{Cx, Size};

use crate::net::wallpaper::{Thumbnail, WallpaperEntry};
use crate::util::jpeg::decode_scaled;

const WALLPAPER_DIR: &str = "/e/CrownOS/crownos-iso/packaging/crownos-base/backgrounds";
pub const CURRENT_WALLPAPER: &str = "amethyst-reverie.jpg";
const THUMBNAIL_EDGE: u32 = 480;

struct Decoded {
    path: String,
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

pub fn wallpaper_path(name: &str) -> String {
    format!("{WALLPAPER_DIR}/{name}")
}

/// Every stock wallpaper with its thumbnail uploaded for this window.
pub fn gallery(cx: &mut Cx) -> Vec<WallpaperEntry> {
    decoded()
        .iter()
        .map(|decoded| WallpaperEntry {
            path: decoded.path.clone(),
            thumbnail: Some(Thumbnail {
                image: cx.register_image(decoded.width, decoded.height, decoded.rgba.clone()),
                size: Size::new(decoded.width as f32, decoded.height as f32),
            }),
        })
        .collect()
}

fn decoded() -> &'static [Decoded] {
    static DECODED: OnceLock<Vec<Decoded>> = OnceLock::new();
    DECODED.get_or_init(|| {
        let mut paths: Vec<_> = std::fs::read_dir(WALLPAPER_DIR)
            .into_iter()
            .flatten()
            .flatten()
            .map(|entry| entry.path())
            .collect();
        paths.sort();
        paths.iter().filter_map(|path| decode(path)).collect()
    })
}

fn decode(path: &Path) -> Option<Decoded> {
    let image = match decode_scaled(path, THUMBNAIL_EDGE) {
        Some(image) => image,
        None => image::open(path).ok()?,
    };
    let scaled = image.thumbnail(THUMBNAIL_EDGE, THUMBNAIL_EDGE).into_rgba8();
    let (width, height) = scaled.dimensions();
    Some(Decoded {
        path: path.to_string_lossy().into_owned(),
        width,
        height,
        rgba: scaled.into_raw(),
    })
}
