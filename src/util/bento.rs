//! Bento tiling: where a tile sits inside one block of a uniform cell grid.
//!
//! A block is [`BLOCK_ROWS`] rows tall. It leads with a feature tile spanning
//! [`FEATURE_SPAN`] cells both ways and fills the rest of its rows with unit
//! tiles; consecutive blocks put the feature on opposite sides, so a long
//! gallery alternates instead of striping.
//!
//! Only cell *units* live here — a cell's pixel size is the grid's business,
//! which is what keeps the pattern independent of how wide the pane is.

/// One tile's place within its block, in cell units.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cell {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

/// The feature tile's span on both axes.
const FEATURE_SPAN: i32 = 2;
/// Rows one block occupies — the feature tile's height.
pub const BLOCK_ROWS: i32 = FEATURE_SPAN;

/// The pattern needs at least one free column beside the feature tile.
fn usable_columns(columns: i32) -> i32 {
    columns.max(FEATURE_SPAN + 1)
}

/// Tiles one block holds: the feature, plus the unit tiles beside it.
pub fn block_size(columns: i32) -> usize {
    let free = usable_columns(columns) - FEATURE_SPAN;
    (BLOCK_ROWS * free + 1).max(1) as usize
}

/// Where the `slot`-th tile of `block` sits, relative to that block's own
/// first row.
pub fn cell(block: usize, slot: usize, columns: i32) -> Cell {
    let columns = usable_columns(columns);
    let feature_leads = block.is_multiple_of(2);

    let Some(slot) = slot.checked_sub(1) else {
        let x = if feature_leads { 0 } else { columns - FEATURE_SPAN };
        return Cell { x, y: 0, width: FEATURE_SPAN, height: FEATURE_SPAN };
    };

    let free_columns = columns - FEATURE_SPAN;
    let first_free = if feature_leads { FEATURE_SPAN } else { 0 };
    let slot = i32::try_from(slot).unwrap_or(0);
    Cell {
        x: first_free + slot % free_columns,
        y: slot / free_columns,
        width: 1,
        height: 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block(block: usize, columns: i32) -> Vec<Cell> {
        (0..block_size(columns))
            .map(|slot| cell(block, slot, columns))
            .collect()
    }

    #[test]
    fn a_block_leads_with_the_feature_and_fills_the_rest() {
        assert_eq!(block_size(4), 5);
        assert_eq!(
            block(0, 4),
            [
                Cell { x: 0, y: 0, width: 2, height: 2 },
                Cell { x: 2, y: 0, width: 1, height: 1 },
                Cell { x: 3, y: 0, width: 1, height: 1 },
                Cell { x: 2, y: 1, width: 1, height: 1 },
                Cell { x: 3, y: 1, width: 1, height: 1 },
            ]
        );
    }

    #[test]
    fn the_next_block_puts_the_feature_on_the_other_side() {
        assert_eq!(
            block(1, 4),
            [
                Cell { x: 2, y: 0, width: 2, height: 2 },
                Cell { x: 0, y: 0, width: 1, height: 1 },
                Cell { x: 1, y: 0, width: 1, height: 1 },
                Cell { x: 0, y: 1, width: 1, height: 1 },
                Cell { x: 1, y: 1, width: 1, height: 1 },
            ]
        );
        assert_eq!(block(2, 4), block(0, 4), "and back again");
    }

    #[test]
    fn tiles_never_overlap() {
        for index in 0..4 {
            let mut taken = Vec::new();
            for cell in block(index, 5) {
                for y in cell.y..cell.y + cell.height {
                    for x in cell.x..cell.x + cell.width {
                        assert!(!taken.contains(&(x, y)), "cell ({x}, {y}) claimed twice");
                        taken.push((x, y));
                    }
                }
            }
            assert_eq!(taken.len() as i32, BLOCK_ROWS * 5, "and leave no cell empty");
        }
    }

    #[test]
    fn a_grid_too_narrow_for_the_pattern_still_tiles() {
        assert_eq!(block_size(1), 3);
        assert_eq!(
            block(0, 1),
            [
                Cell { x: 0, y: 0, width: 2, height: 2 },
                Cell { x: 2, y: 0, width: 1, height: 1 },
                Cell { x: 2, y: 1, width: 1, height: 1 },
            ]
        );
    }
}
