use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;

const KNOWN_WINDOWS: &[&str] = &["picker", "configuration", "setup", "migration"];

#[derive(Clone, Debug, Default, Serialize)]
struct WindowsFile {
    #[serde(flatten)]
    windows: BTreeMap<String, WindowSize>,
}

#[derive(Clone, Copy, Debug, Serialize)]
struct WindowSize {
    width: i32,
    height: i32,
}

pub fn load(name: &str) -> Option<(i32, i32)> {
    load_from(&state_path()?, name)
}

pub fn save(name: &str, width: i32, height: i32) {
    let Some(path) = state_path() else {
        return;
    };
    save_to(&path, name, width, height);
}

fn state_path() -> Option<PathBuf> {
    let directory = env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/state")))?;
    Some(directory.join("browser-picker/windows.toml"))
}

fn load_from(path: &Path, name: &str) -> Option<(i32, i32)> {
    if !KNOWN_WINDOWS.contains(&name) {
        return None;
    }
    let size = load_file(path)?.windows.get(name).copied()?;
    (size.width > 0 && size.height > 0).then_some((size.width, size.height))
}

fn save_to(path: &Path, name: &str, width: i32, height: i32) {
    if !KNOWN_WINDOWS.contains(&name) || width <= 0 || height <= 0 {
        return;
    }
    let mut file = load_file(path).unwrap_or_default();
    file.windows
        .retain(|window, _| KNOWN_WINDOWS.contains(&window.as_str()));
    file.windows
        .insert(name.to_owned(), WindowSize { width, height });
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let Ok(rendered) = toml::to_string_pretty(&file) else {
        return;
    };
    let _ = fs::write(path, rendered);
}

fn load_file(path: &Path) -> Option<WindowsFile> {
    let value: toml::Value = toml::from_str(&fs::read_to_string(path).ok()?).ok()?;
    let table = value.as_table()?;
    let mut windows = BTreeMap::new();
    for (name, value) in table {
        if !KNOWN_WINDOWS.contains(&name.as_str()) {
            continue;
        }
        let Some(window) = value.as_table() else {
            continue;
        };
        let (Some(width), Some(height)) = (
            window.get("width").and_then(toml::Value::as_integer),
            window.get("height").and_then(toml::Value::as_integer),
        ) else {
            continue;
        };
        if width > 0 && height > 0 {
            windows.insert(
                name.clone(),
                WindowSize {
                    width: width as i32,
                    height: height as i32,
                },
            );
        }
    }
    Some(WindowsFile { windows })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn save_round_trips_known_window_geometry() {
        let directory = TempDir::new().expect("state directory should be writable");
        let path = directory.path().join("windows.toml");
        save_to(&path, "picker", 800, 600);
        assert_eq!(load_from(&path, "picker"), Some((800, 600)));
        let stored = fs::read_to_string(&path).expect("state file should exist");
        assert!(stored.contains("width = 800"));
        assert!(stored.contains("height = 600"));
        assert!(!stored.contains("search"));
        assert!(!stored.contains("target"));
    }

    #[test]
    fn save_drops_sensitive_unknown_keys() {
        let directory = TempDir::new().expect("state directory should be writable");
        let path = directory.path().join("windows.toml");
        fs::write(
            &path,
            "[picker]\nwidth = 640\nheight = 480\n\n[search]\nquery = \"secret-token\"\n\n[queue]\ntarget = \"https://example.com/?token=do-not-print\"\n",
        )
        .expect("seeded state should be writable");
        save_to(&path, "configuration", 720, 480);
        let stored = fs::read_to_string(&path).expect("state file should exist");
        assert_eq!(load_from(&path, "picker"), Some((640, 480)));
        assert_eq!(load_from(&path, "configuration"), Some((720, 480)));
        assert!(!stored.contains("secret-token"));
        assert!(!stored.contains("do-not-print"));
        assert!(!stored.contains("search"));
        assert!(!stored.contains("queue"));
        assert!(!stored.contains("target"));
    }

    #[test]
    fn unknown_windows_are_ignored() {
        let directory = TempDir::new().expect("state directory should be writable");
        let path = directory.path().join("windows.toml");
        save_to(&path, "search", 10, 10);
        assert!(path.exists() == false);
        assert_eq!(load_from(&path, "search"), None);
    }
}
