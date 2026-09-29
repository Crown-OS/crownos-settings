//! QR codes as brushes the `image` view can draw.

use qrcode::types::QrError;
use qrcode::{Color, QrCode};
use xilem::masonry::peniko::{ImageAlphaType, ImageData, ImageQuality};
use xilem::{Blob, ImageBrush, ImageFormat};

/// The blank border scanners need around the symbol, in modules.
const QUIET_ZONE: usize = 4;
const DARK: [u8; 4] = [0, 0, 0, 255];
const LIGHT: [u8; 4] = [255, 255, 255, 255];

/// Encode `text` at one pixel per module, sampled nearest-neighbour so the
/// symbol stays sharp at whatever integer scale it is drawn.
pub fn qr_brush(text: &str) -> Result<ImageBrush, QrError> {
    let code = QrCode::new(text)?;
    let modules = code.width();
    let side = modules + 2 * QUIET_ZONE;
    let edge = u32::try_from(side).map_err(|_| QrError::DataTooLong)?;

    let module_at = |offset: usize| {
        offset
            .checked_sub(QUIET_ZONE)
            .filter(|&inner| inner < modules)
    };
    let pixels: Vec<u8> = (0..side * side)
        .flat_map(|index| {
            let dark = module_at(index % side)
                .zip(module_at(index / side))
                .is_some_and(|position| code[position] == Color::Dark);
            if dark { DARK } else { LIGHT }
        })
        .collect();

    Ok(ImageBrush::new(ImageData {
        data: Blob::from(pixels),
        format: ImageFormat::Rgba8,
        alpha_type: ImageAlphaType::Alpha,
        width: edge,
        height: edge,
    })
    .with_quality(ImageQuality::Low))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pixel(brush: &ImageBrush, x: usize, y: usize) -> &[u8] {
        let side = brush.image.width as usize;
        let start = (y * side + x) * 4;
        &brush.image.data.data()[start..start + 4]
    }

    #[test]
    fn the_symbol_sits_inside_a_light_quiet_zone() {
        let brush = qr_brush("crownconnect://pair?k=abc").expect("short text always encodes");
        assert_eq!(brush.image.width, brush.image.height);
        assert!(brush.image.width as usize >= 21 + 2 * QUIET_ZONE);
        assert_eq!(pixel(&brush, 0, 0), LIGHT);
        assert_eq!(
            pixel(&brush, QUIET_ZONE, QUIET_ZONE),
            DARK,
            "the top-left finder pattern starts right after the quiet zone"
        );
    }
}
