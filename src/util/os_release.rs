//! The installed release as `/etc/os-release` names it, for when the update
//! service cannot tell.

use std::sync::OnceLock;

const OS_RELEASE: &str = "/etc/os-release";
const VERSION_ID: &str = "VERSION_ID=";

/// Read once for each process: the file changes only with an update, and an
/// update restarts this app.
pub fn installed_version_id() -> Option<&'static str> {
    static VERSION: OnceLock<Option<String>> = OnceLock::new();
    VERSION
        .get_or_init(|| {
            let text = std::fs::read_to_string(OS_RELEASE).ok()?;
            version_id(&text).map(str::to_owned)
        })
        .as_deref()
}

fn version_id(text: &str) -> Option<&str> {
    text.lines()
        .find_map(|line| line.strip_prefix(VERSION_ID))
        .map(|value| value.trim().trim_matches(['"', '\'']))
        .filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_plain_and_quoted_values() {
        assert_eq!(
            version_id("NAME=CrownOS\nVERSION_ID=0.4.0\n"),
            Some("0.4.0")
        );
        assert_eq!(version_id("VERSION_ID=\"0.4.1\"\n"), Some("0.4.1"));
        assert_eq!(version_id("NAME=Arch Linux\nBUILD_ID=rolling\n"), None);
        assert_eq!(version_id("VERSION_ID=\n"), None);
    }
}
