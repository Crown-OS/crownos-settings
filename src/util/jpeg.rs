//! JPEG decoding at a fraction of full size, for thumbnails, through
//! libjpeg-turbo — the decoder crownpaper itself uses.
//!
//! The inverse DCT can produce an image at 1/8 to 7/8 of its size directly,
//! so a 7680×4320 wallpaper becomes a 960×540 buffer without the 133 MB one in
//! between, in a fraction of the time.

use std::path::Path;

use image::{DynamicImage, RgbImage};
use turbojpeg::{Decompressor, Image, PixelFormat, ScalingFactor};

const JPEG_MAGIC: [u8; 3] = [0xFF, 0xD8, 0xFF];
const RGB_CHANNELS: usize = 3;

/// `path` decoded at the smallest scale whose longest edge still covers
/// `edge`, or `None` when it is not a JPEG libjpeg-turbo can read.
pub fn decode_scaled(path: &Path, edge: u32) -> Option<DynamicImage> {
    let jpeg = std::fs::read(path).ok()?;
    if !jpeg.starts_with(&JPEG_MAGIC) {
        return None;
    }
    let mut decompressor = Decompressor::new().ok()?;
    let header = decompressor.read_header(&jpeg).ok()?;
    let factor = smallest_covering_factor(header.width.max(header.height), edge as usize);
    decompressor.set_scaling_factor(factor).ok()?;
    let scaled = header.scaled(factor);
    let pitch = scaled.width * RGB_CHANNELS;
    let mut pixels = vec![0; pitch * scaled.height];
    decompressor
        .decompress(
            &jpeg,
            Image {
                pixels: pixels.as_mut_slice(),
                width: scaled.width,
                pitch,
                height: scaled.height,
                format: PixelFormat::RGB,
            },
        )
        .ok()?;
    RgbImage::from_raw(
        u32::try_from(scaled.width).ok()?,
        u32::try_from(scaled.height).ok()?,
        pixels,
    )
    .map(DynamicImage::from)
}

/// The most aggressive DCT scaling that leaves `longest` at least `edge` long.
fn smallest_covering_factor(longest: usize, edge: usize) -> ScalingFactor {
    Decompressor::supported_scaling_factors()
        .into_iter()
        .filter(|factor| factor.num() <= factor.denom() && factor.scale(longest) >= edge)
        .min_by(|a, b| (a.num() * b.denom()).cmp(&(b.num() * a.denom())))
        .unwrap_or(ScalingFactor::ONE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scales_down_as_far_as_the_edge_allows() {
        let factor = smallest_covering_factor(7680, 480);
        assert_eq!((factor.num(), factor.denom()), (1, 8));
        let factor = smallest_covering_factor(3000, 480);
        assert!(factor.scale(3000) >= 480 && factor.num() < factor.denom());
    }

    #[test]
    fn a_small_image_is_decoded_whole() {
        let factor = smallest_covering_factor(400, 480);
        assert_eq!(factor.num(), factor.denom());
    }
}
