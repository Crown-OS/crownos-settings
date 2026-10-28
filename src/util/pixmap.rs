//! tiny-skia pixmaps on their way to the renderer.

use lockscreen::tiny_skia::Pixmap;

/// `pixmap` as the straight-alpha RGBA8 the renderer takes: tiny-skia keeps
/// its colors multiplied by alpha.
pub fn straight_rgba(pixmap: &Pixmap) -> Vec<u8> {
    pixmap
        .pixels()
        .iter()
        .flat_map(|pixel| {
            let color = pixel.demultiply();
            [color.red(), color.green(), color.blue(), color.alpha()]
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use lockscreen::tiny_skia::Color;

    use super::*;

    #[test]
    fn half_transparent_red_comes_back_full_strength() {
        let mut pixmap = Pixmap::new(1, 1).expect("one pixel");
        pixmap.fill(Color::from_rgba8(255, 0, 0, 128));
        assert_eq!(pixmap.data(), [128, 0, 0, 128]);
        assert_eq!(straight_rgba(&pixmap), [255, 0, 0, 128]);
    }
}
