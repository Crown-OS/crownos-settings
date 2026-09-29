//! Bento tiling: where a tile sits inside one section of a uniform cell grid.
//!
//! A block is two sections, [`COLUMNS`] cells wide:
//!
//! * the *showcase* — a hero two columns wide over a wide frame, beside two
//!   single columns each split at a different height. No two of the three
//!   columns break at the same row, which is what keeps the seams from lining
//!   up into stripes, and all three start and end together.
//! * the *band* under it — two single tiles and a double, a plainer rhythm
//!   that lets the showcase above it read as the composition.
//!
//! Consecutive blocks mirror, so a long gallery alternates rather than
//! repeating. The proportions are taken from a reference layout: its column
//! splits fall at rows 3, 5 and 6 of the showcase's 9, and its band is a third
//! of the showcase's height.
//!
//! Only cell *units* live here — a cell's pixel size is the grid's business,
//! which is what keeps the pattern independent of how wide the pane is.

/// Cell columns every section is laid out across.
pub const COLUMNS: i32 = 4;

/// One tile's place within its section, in cell units.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cell {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

impl Cell {
    /// The same tile in a section flipped left to right.
    fn mirrored(self) -> Self {
        Self {
            x: COLUMNS - self.x - self.width,
            ..self
        }
    }
}

pub const SHOWCASE_ROWS: i32 = 9;
pub const BAND_ROWS: i32 = 3;

/// Tiles in reading order: the top of all three columns first, then what sits
/// under each — so a short last section fills from the top rather than
/// leaving a hole in it.
static SHOWCASE: [Cell; 6] = [
    Cell { x: 0, y: 0, width: 2, height: 5 },
    Cell { x: 2, y: 0, width: 1, height: 3 },
    Cell { x: 3, y: 0, width: 1, height: 6 },
    Cell { x: 2, y: 3, width: 1, height: 6 },
    Cell { x: 0, y: 5, width: 2, height: 4 },
    Cell { x: 3, y: 6, width: 1, height: 3 },
];

static BAND: [Cell; 3] = [
    Cell { x: 0, y: 0, width: 1, height: 3 },
    Cell { x: 1, y: 0, width: 1, height: 3 },
    Cell { x: 2, y: 0, width: 2, height: 3 },
];

/// Tiles the showcase holds, and how many a block holds in total.
pub const SHOWCASE_TILES: usize = SHOWCASE.len();
pub const BLOCK_SIZE: usize = SHOWCASE.len() + BAND.len();

/// Where each tile of `block`'s showcase sits.
pub fn showcase(block: usize) -> impl Iterator<Item = Cell> {
    laid_out(&SHOWCASE, block)
}

/// Where each tile of `block`'s band sits, relative to the band's own first
/// row.
pub fn band(block: usize) -> impl Iterator<Item = Cell> {
    laid_out(&BAND, block)
}

fn laid_out(cells: &'static [Cell], block: usize) -> impl Iterator<Item = Cell> {
    let mirrored = !block.is_multiple_of(2);
    cells
        .iter()
        .map(move |cell| if mirrored { cell.mirrored() } else { *cell })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn packing(cells: impl Iterator<Item = Cell>, rows: i32) -> Vec<(i32, i32)> {
        let mut taken = Vec::new();
        for cell in cells {
            for y in cell.y..cell.y + cell.height {
                for x in cell.x..cell.x + cell.width {
                    assert!(!taken.contains(&(x, y)), "cell ({x}, {y}) claimed twice");
                    assert!(x < COLUMNS && y < rows, "cell ({x}, {y}) is outside the section");
                    taken.push((x, y));
                }
            }
        }
        assert_eq!(taken.len() as i32, COLUMNS * rows, "and no cell is left empty");
        taken
    }

    #[test]
    fn every_section_packs_its_grid_exactly() {
        for block in 0..4 {
            packing(showcase(block), SHOWCASE_ROWS);
            packing(band(block), BAND_ROWS);
        }
    }

    /// The point of the pattern: were two columns to break at the same row,
    /// the showcase would read as ordinary rows rather than as a composition.
    #[test]
    fn no_two_columns_of_the_showcase_break_at_the_same_row() {
        let mut breaks: Vec<i32> = showcase(0)
            .map(|cell| cell.y)
            .filter(|&y| y != 0)
            .collect();
        breaks.sort_unstable();
        let unique = {
            let mut seen = breaks.clone();
            seen.dedup();
            seen
        };
        assert_eq!(breaks, unique, "two columns share a break row");
        assert_eq!(breaks, [3, 5, 6]);
    }

    #[test]
    fn the_next_block_is_the_mirror_of_this_one() {
        assert_eq!(
            showcase(1).next(),
            Some(Cell { x: 2, y: 0, width: 2, height: 5 }),
            "the hero changes sides"
        );
        assert_eq!(
            band(1).last(),
            Some(Cell { x: 0, y: 0, width: 2, height: 3 }),
            "and so does the band's wide tile"
        );
        assert_eq!(
            showcase(2).collect::<Vec<_>>(),
            showcase(0).collect::<Vec<_>>(),
            "and back again"
        );
    }
}
