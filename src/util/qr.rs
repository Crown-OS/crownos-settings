//! QR codes as a grid of modules, ready to be painted as rectangles.

use qrcode::types::QrError;
use qrcode::{Color, QrCode};

/// The blank border scanners need around the symbol, in modules.
const QUIET_ZONE: usize = 4;

/// A QR symbol including its quiet zone: `side × side` modules, row-major.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QrMatrix {
    side: usize,
    dark: Vec<bool>,
}

/// One horizontal run of dark modules, in module units.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DarkRun {
    pub row: usize,
    pub start: usize,
    pub length: usize,
}

impl QrMatrix {
    /// Encode `text`, framed by the quiet zone.
    pub fn encode(text: &str) -> Result<Self, QrError> {
        let code = QrCode::new(text)?;
        let modules = code.width();
        let side = modules + 2 * QUIET_ZONE;
        let module_at = |offset: usize| {
            offset
                .checked_sub(QUIET_ZONE)
                .filter(|&inner| inner < modules)
        };
        let dark = (0..side * side)
            .map(|index| {
                module_at(index % side)
                    .zip(module_at(index / side))
                    .is_some_and(|position| code[position] == Color::Dark)
            })
            .collect();
        Ok(Self { side, dark })
    }

    /// Modules along one edge, quiet zone included.
    pub fn side(&self) -> usize {
        self.side
    }

    /// The dark modules merged into horizontal runs, so painting a symbol is
    /// a few hundred rectangles rather than a few thousand.
    pub fn dark_runs(&self) -> impl Iterator<Item = DarkRun> + '_ {
        self.dark
            .chunks(self.side)
            .enumerate()
            .flat_map(|(row, modules)| {
                let mut column = 0;
                std::iter::from_fn(move || {
                    let start = column + modules[column..].iter().position(|dark| *dark)?;
                    let length = modules[start..]
                        .iter()
                        .position(|dark| !*dark)
                        .unwrap_or(modules.len() - start);
                    column = start + length;
                    Some(DarkRun { row, start, length })
                })
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn symbol() -> QrMatrix {
        QrMatrix::encode("crownconnect://pair?k=abc").expect("short text always encodes")
    }

    #[test]
    fn the_symbol_sits_inside_a_light_quiet_zone() {
        let matrix = symbol();
        assert!(matrix.side() >= 21 + 2 * QUIET_ZONE);
        let first = matrix
            .dark_runs()
            .next()
            .expect("a symbol has dark modules");
        assert_eq!(
            (first.row, first.start, first.length),
            (QUIET_ZONE, QUIET_ZONE, 7),
            "the top-left finder pattern starts right after the quiet zone"
        );
        assert!(
            matrix
                .dark_runs()
                .all(|run| run.start + run.length <= matrix.side() - QUIET_ZONE),
            "nothing dark reaches the right-hand quiet zone"
        );
    }

    #[test]
    fn runs_cover_exactly_the_dark_modules() {
        let matrix = symbol();
        let painted: usize = matrix.dark_runs().map(|run| run.length).sum();
        assert_eq!(painted, matrix.dark.iter().filter(|dark| **dark).count());
    }
}
