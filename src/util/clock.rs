//! Wall-clock time in the units the daemons speak.

use std::time::{SystemTime, UNIX_EPOCH};

/// Milliseconds since the Unix epoch, or zero on a clock set before it.
pub fn unix_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| {
            u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX)
        })
}

/// How long before `now_unix_ms` the moment `at_unix_ms` was, in the
/// largest whole unit: "just now", "12s ago", "3m ago", "2h ago", "4d ago".
pub fn ago(at_unix_ms: u64, now_unix_ms: u64) -> String {
    const UNITS: [(u64, &str); 3] = [(86_400, "d"), (3_600, "h"), (60, "m")];
    let seconds = now_unix_ms.saturating_sub(at_unix_ms) / 1000;
    if seconds == 0 {
        return "just now".to_owned();
    }
    UNITS.iter().find(|(size, _)| seconds >= *size).map_or_else(
        || format!("{seconds}s ago"),
        |(size, unit)| format!("{}{unit} ago", seconds / size),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ages_are_told_in_the_largest_whole_unit() {
        assert_eq!(ago(10_000, 10_500), "just now");
        assert_eq!(ago(0, 12_000), "12s ago");
        assert_eq!(ago(0, 185_000), "3m ago");
        assert_eq!(ago(0, 7_300_000), "2h ago");
        assert_eq!(ago(0, 4 * 86_400_000), "4d ago");
        assert_eq!(ago(5_000, 0), "just now", "a clock that went back");
    }
}
