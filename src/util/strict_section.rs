//! Sections that decide who may see the screen. They are read strictly: a
//! file that does not parse is shown as a problem and never replaced with
//! defaults or with an edit made over them.

use serde::Serialize;
use serde::de::DeserializeOwned;

/// `agents`, `visual_context` and `context_access`.
pub const STRICT_SECTIONS: [&str; 3] = ["agents", "visual_context", "context_access"];

pub fn is_strict(section: &str) -> bool {
    STRICT_SECTIONS.contains(&section)
}

/// Reads a section: strictly for the sections above, so a broken file gives
/// the defaults in memory and is left alone on disk.
pub fn load_section<T: DeserializeOwned + Serialize + Default>(section: &str) -> T {
    if is_strict(section) {
        crownos_config::load_strict(section).unwrap_or_default()
    } else {
        crownos_config::load(section)
    }
}

/// Why a strict section's file cannot be used, or `None` when it can. A test
/// without a scratch `CROWN_CONFIG_DIR` never reads the user's files.
pub fn problem_with<T: DeserializeOwned + Serialize + Default>(section: &str) -> Option<String> {
    if cfg!(test) && std::env::var_os(crownos_config::CONFIG_DIR_ENV).is_none() {
        return None;
    }
    crownos_config::load_strict::<T>(section)
        .err()
        .map(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_access_sections_are_strict() {
        assert!(is_strict("context_access"));
        assert!(is_strict("agents"));
        assert!(!is_strict("appearance"));
    }
}
