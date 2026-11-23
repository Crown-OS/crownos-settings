//! The installed desktop apps, each with the `AppKey` its program runs as,
//! for granting access before the app first asks.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crownos_config::schema::AppKey;

/// An app the user can pick by name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesktopApp {
    pub name: String,
    pub key: AppKey,
}

/// Every visible application entry under the XDG data directories whose
/// program can be found and keyed, by name.
pub fn installed_desktop_apps() -> Vec<DesktopApp> {
    let mut by_name = BTreeMap::new();
    for directory in application_directories() {
        let Ok(entries) = std::fs::read_dir(&directory) else {
            continue;
        };
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            if path
                .extension()
                .is_none_or(|extension| extension != "desktop")
            {
                continue;
            }
            let Some(app) = std::fs::read_to_string(&path)
                .ok()
                .and_then(|text| parse_desktop_entry(&text, &search_path()))
            else {
                continue;
            };
            by_name.entry(app.name.clone()).or_insert(app);
        }
    }
    by_name.into_values().collect()
}

fn application_directories() -> Vec<PathBuf> {
    let data_home = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share")));
    let data_dirs =
        std::env::var("XDG_DATA_DIRS").unwrap_or_else(|_| "/usr/local/share:/usr/share".to_owned());
    data_home
        .into_iter()
        .chain(data_dirs.split(':').map(PathBuf::from))
        .map(|directory| directory.join("applications"))
        .collect()
}

fn search_path() -> Vec<PathBuf> {
    std::env::var_os("PATH")
        .map(|path| std::env::split_paths(&path).collect())
        .unwrap_or_default()
}

/// The app an entry launches, if it is a visible application whose program
/// is found on `path`.
fn parse_desktop_entry(text: &str, path: &[PathBuf]) -> Option<DesktopApp> {
    let mut in_main_group = false;
    let mut name = None;
    let mut exec = None;
    for line in text.lines().map(str::trim) {
        if line.starts_with('[') {
            in_main_group = line == "[Desktop Entry]";
            continue;
        }
        if !in_main_group {
            continue;
        }
        let Some((field, value)) = line.split_once('=') else {
            continue;
        };
        match field.trim() {
            "Name" => name = Some(value.trim().to_owned()),
            "Exec" => exec = Some(value.trim().to_owned()),
            "Type" if value.trim() != "Application" => return None,
            "NoDisplay" | "Hidden" if value.trim() == "true" => return None,
            _ => {}
        }
    }
    let arguments: Vec<String> = exec?
        .split_whitespace()
        .filter(|argument| !argument.starts_with('%'))
        .map(|argument| argument.trim_matches('"').to_owned())
        .collect();
    let program = find_program(arguments.first()?, path)?;
    let key = AppKey::of_program(&program, &arguments, None)?;
    Some(DesktopApp { name: name?, key })
}

fn find_program(program: &str, path: &[PathBuf]) -> Option<PathBuf> {
    let candidate = Path::new(program);
    let found = if candidate.is_absolute() {
        Some(candidate.to_path_buf())
    } else {
        path.iter()
            .map(|directory| directory.join(program))
            .find(|full| full.is_file())
    }?;
    found.canonicalize().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_entry_names_its_program() {
        let entry = "[Desktop Entry]\nType=Application\nName=Environment\nExec=env %U\n";
        let path = [PathBuf::from("/bin"), PathBuf::from("/usr/bin")];
        let app = parse_desktop_entry(entry, &path);
        assert_eq!(
            app.as_ref().map(|app| app.name.as_str()),
            Some("Environment")
        );
    }

    #[test]
    fn hidden_entries_and_other_groups_are_left_out() {
        let path = [PathBuf::from("/usr/bin")];
        assert!(
            parse_desktop_entry("[Desktop Entry]\nName=A\nExec=sh\nNoDisplay=true\n", &path)
                .is_none()
        );
        assert!(
            parse_desktop_entry("[Desktop Entry]\nType=Link\nName=A\nExec=sh\n", &path).is_none()
        );
        assert!(parse_desktop_entry("[Desktop Action new]\nName=A\nExec=sh\n", &path).is_none());
        assert!(
            parse_desktop_entry("[Desktop Entry]\nName=A\nExec=no-such-program\n", &path).is_none()
        );
    }
}
