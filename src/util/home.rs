//! `~` in paths a person typed.

use std::borrow::Cow;

/// `path` with a leading `~` or `~/` replaced by `$HOME`; untouched when it
/// has neither or there is no home to expand to.
pub fn expand_home(path: &str) -> Cow<'_, str> {
    expand_with(path, std::env::var("HOME").ok().as_deref())
}

fn expand_with<'a>(path: &'a str, home: Option<&str>) -> Cow<'a, str> {
    let Some(home) = home else {
        return Cow::Borrowed(path);
    };
    match path.strip_prefix('~') {
        Some("") => Cow::Owned(home.to_owned()),
        Some(rest) if rest.starts_with('/') => Cow::Owned(format!("{home}{rest}")),
        _ => Cow::Borrowed(path),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_leading_tilde_expands() {
        let home = Some("/home/crown");
        assert_eq!(
            expand_with("~/plugins/clock", home),
            "/home/crown/plugins/clock"
        );
        assert_eq!(expand_with("~", home), "/home/crown");
        assert_eq!(expand_with("~other/x", home), "~other/x");
        assert_eq!(expand_with("/opt/x~", home), "/opt/x~");
        assert_eq!(expand_with("~/x", None), "~/x");
    }
}
