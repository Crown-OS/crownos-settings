//! Glass digits, as the compositor fills them after ColorOS 17: the lock's
//! background blurred behind each glyph, brightened, with white added on top
//! — a plus-lighter blend on sRGB values, see [`Glass`] — kept to the glyph's
//! coverage.

use crownos_config::schema::{LockBackground, Theme};
use lockscreen::clock::{Glass, Homography, rim_at};
use lockscreen::image::{darken, gaussian_blur};
use lockscreen::palette::Palette;
use lockscreen::tiny_skia::{Color, Pixmap};
use lockscreen::transition::Moments;

use super::Studio;
use super::pictures::{DESKTOP_ZOOM, named, zoomed};

/// The background behind the glass: what it shows, at what size and how
/// darkened, and the sigma, in pixels, it is blurred by; the two `f32`s as
/// their bits.
pub(super) type GlassKey = ((LockBackground, String, [u32; 2], u32), u32);

/// Where a sprite's pixmap lies over the blurred background, where its glyph
/// runs down it, and the glass it is filled with.
pub(super) struct Pane {
    /// From the sprite's pixels to the backdrop's.
    pub to_backdrop: Homography,
    /// The sprite rows of the glyph's top and of its baseline.
    pub rows: [f32; 2],
    pub glass: Glass,
}

/// The darkening the frost carries for a lock dimmed by `dim`, at rest on
/// the clock, where the canvas shows the glass.
pub(super) fn frost_darkening(dim: f32) -> f32 {
    Moments::CLOCK.frame(0.0).frost_darkening(dim)
}

/// White `coverage` filled with its glass: per sRGB channel,
/// `min(gain · frost + white(down, rim), 1)` over the blurred `backdrop`,
/// `down` running from the glyph's top to its baseline and `rim` lighting
/// the glyph's edge as [`rim_at`] has the compositor light it.
pub(super) fn frost(coverage: &Pixmap, backdrop: &Pixmap, pane: &Pane) -> Option<Pixmap> {
    let mut frosted = Pixmap::new(coverage.width(), coverage.height())?;
    let row_bytes = coverage.width() as usize * 4;
    let backdrop_width = backdrop.width() as usize;
    let last = [backdrop.width(), backdrop.height()].map(|side| side.saturating_sub(1) as f32);
    let behind = backdrop.data();
    let [top, baseline] = pane.rows;
    let height = (baseline - top).max(1.0);
    let sample = |column: usize, row: usize| {
        let centre = [column as f32 + 0.5, row as f32 + 0.5];
        let [x, y] = pane.to_backdrop.apply(centre).unwrap_or(centre);
        y.clamp(0.0, last[1]) as usize * backdrop_width + x.clamp(0.0, last[0]) as usize
    };
    let rows = frosted
        .data_mut()
        .chunks_exact_mut(row_bytes)
        .zip(coverage.data().chunks_exact(row_bytes));
    for (row, (pixels, covered)) in rows.enumerate() {
        let down = ((row as f32 + 0.5 - top) / height).clamp(0.0, 1.0);
        let columns = pixels.chunks_exact_mut(4).zip(covered.chunks_exact(4));
        for (column, (pixel, covered)) in columns.enumerate() {
            let alpha = covered[3];
            if alpha == 0 {
                continue;
            }
            let rim = rim_at(coverage, [column, row], height);
            let seen = &behind[sample(column, row) * 4..][..3];
            for (channel, frost) in pixel.iter_mut().zip(seen) {
                let face = pane.glass.face(f32::from(*frost) / 255.0, down, rim);
                *channel = (face * f32::from(alpha)).round() as u8;
            }
            pixel[3] = alpha;
        }
    }
    Some(frosted)
}

impl Studio {
    /// Makes sure [`Studio::glass`] holds the lock's background blurred like
    /// a gaussian of `sigma` preview pixels and darkened as the frost is,
    /// made once per background, size, darkening and sigma; a `draft` makes
    /// do with the background blurred by any sigma.
    pub(super) fn ready_glass(&mut self, sigma: f32, theme: &Theme, draft: bool) {
        let Some(request) = &self.backdrop_request else {
            self.glass = None;
            return;
        };
        let key = (
            (
                request.background.clone(),
                request.desktop.clone(),
                request.pixels,
                frost_darkening(request.dim).to_bits(),
            ),
            sigma.to_bits(),
        );
        let made = |known: &GlassKey| known.0 == key.0 && (draft || known.1 == key.1);
        if self.glass.as_ref().is_some_and(|(known, _)| made(known)) {
            return;
        }
        self.glass = self
            .blurred_background(&key, theme)
            .map(|pixmap| (key, pixmap));
    }

    fn blurred_background(&mut self, key: &GlassKey, theme: &Theme) -> Option<Pixmap> {
        let ((background, desktop, pixels, darkening), sigma) = key;
        let darkening = f32::from_bits(*darkening);
        let (path, zoom) = match background {
            LockBackground::Token(token) => return flat(theme, token, darkening),
            LockBackground::Desktop => (named(desktop), DESKTOP_ZOOM),
            LockBackground::Image(path) => (named(path), 1.0),
        };
        let mut frost = zoomed(self.covered(path?, *pixels)?, zoom)?;
        gaussian_blur(&mut frost, f32::from_bits(*sigma));
        darken(&mut frost, darkening);
        Some(frost)
    }
}

/// One pixel of `token`'s color, as darkened as the frost: a flat
/// background blurs to itself.
fn flat(theme: &Theme, token: &str, darkening: f32) -> Option<Pixmap> {
    let [red, green, blue, alpha] = Palette::new(theme).color(token);
    let mut pixel = Pixmap::new(1, 1)?;
    pixel.fill(Color::from_rgba(red, green, blue, alpha)?);
    darken(&mut pixel, darkening);
    Some(pixel)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn white_square(side: u32) -> Pixmap {
        let mut pixmap = Pixmap::new(side, side).expect("size");
        pixmap.fill(Color::WHITE);
        pixmap
    }

    #[test]
    fn clear_glass_shows_the_background_and_solid_white_hides_it() {
        let mut backdrop = Pixmap::new(8, 8).expect("size");
        backdrop.fill(Color::from_rgba8(0, 0, 200, 255));
        let pane = |whiteness| Pane {
            to_backdrop: Homography::affine([1.0; 2], [2.0; 2]),
            rows: [0.0, 40.0],
            glass: Glass::at(whiteness),
        };
        let clear = frost(&white_square(40), &backdrop, &pane(0.0)).expect("frosted");
        let middle = &clear.data()[(20 * 40 + 20) * 4..][..4];
        let added = (0.03 * 255.0_f32).round() as u8;
        assert_eq!(middle, &[added, added, 200 + added, 255]);
        let white = frost(&white_square(40), &backdrop, &pane(1.0)).expect("frosted");
        assert_eq!(&white.data()[..4], &[255, 255, 255, 255]);
    }

    #[test]
    fn the_top_shines_the_edge_glows_and_the_coverage_holds() {
        let mut backdrop = Pixmap::new(4, 4).expect("size");
        backdrop.fill(Color::BLACK);
        let mut glyph = white_square(40);
        glyph.data_mut()[40 * 4 * 30..].fill(0);
        let pane = Pane {
            to_backdrop: Homography::affine([0.1; 2], [0.0; 2]),
            rows: [0.0, 30.0],
            glass: Glass::at(0.5),
        };
        let glyph = frost(&glyph, &backdrop, &pane).expect("frosted");
        let red = |row: usize, column: usize| glyph.data()[(row * 40 + column) * 4];
        assert!(red(1, 20) > red(20, 20), "the top shines");
        assert!(red(20, 0) > red(20, 20), "the edge glows");
        assert_eq!(
            &glyph.data()[(35 * 40) * 4..][..4],
            &[0, 0, 0, 0],
            "outside stays clear"
        );
    }

    #[test]
    fn a_moved_clock_sees_the_background_where_it_now_stands() {
        let mut backdrop = Pixmap::new(80, 8).expect("size");
        backdrop.fill(Color::BLACK);
        for row in backdrop.data_mut().chunks_exact_mut(80 * 4) {
            row[40 * 4..].copy_from_slice(&[0, 0, 200, 255].repeat(40));
        }
        let at = |x: f32| Pane {
            to_backdrop: Homography::affine([1.0; 2], [x, 0.0]),
            rows: [0.0, 8.0],
            glass: Glass::at(0.0),
        };
        let blue = |pane: &Pane| {
            frost(&white_square(8), &backdrop, pane)
                .expect("frosted")
                .data()[(4 * 8 + 4) * 4 + 2]
        };
        assert!(blue(&at(50.0)) > blue(&at(10.0)) + 100);
    }
}
