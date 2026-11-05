//! Byte counts as people read them.

const UNITS: [&str; 5] = ["bytes", "KB", "MB", "GB", "TB"];
const STEP: f64 = 1000.0;
/// The first unit shown with a decimal.
const GB: usize = 3;

/// `bytes` in decimal units, with one decimal from GB up: `"212 MB"`,
/// `"1.4 GB"`.
pub fn byte_size(bytes: u64) -> String {
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= STEP && unit + 1 < UNITS.len() {
        value /= STEP;
        unit += 1;
    }
    if unit < GB {
        format!("{value:.0} {}", UNITS[unit])
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_the_largest_unit_under_a_thousand() {
        assert_eq!(byte_size(0), "0 bytes");
        assert_eq!(byte_size(999), "999 bytes");
        assert_eq!(byte_size(48_200), "48 KB");
        assert_eq!(byte_size(212_000_000), "212 MB");
        assert_eq!(byte_size(1_437_000_000), "1.4 GB");
        assert_eq!(byte_size(2_500_000_000_000_000), "2500.0 TB");
    }
}
