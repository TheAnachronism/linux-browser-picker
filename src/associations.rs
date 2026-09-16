use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use crate::discovery::{self, HTTP, HTTPS};
use crate::i18n;

pub const HTML: &str = "text/html";
pub const XHTML: &str = "application/xhtml+xml";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DefaultHandler {
    BrowserPicker,
    Other { name: String },
    Unset,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AssociationReport {
    pub http: DefaultHandler,
    pub https: DefaultHandler,
    pub html: DefaultHandler,
    pub xhtml: DefaultHandler,
}

pub fn report() -> AssociationReport {
    report_in(&config_home(), &data_directories())
}

pub(crate) fn report_in(config_home: &Path, data_directories: &[PathBuf]) -> AssociationReport {
    let (defaults, removed) = read_mimeapps(&config_home.join("mimeapps.list"));
    AssociationReport {
        http: handler_for(HTTP, &defaults, &removed, data_directories),
        https: handler_for(HTTPS, &defaults, &removed, data_directories),
        html: handler_for(HTML, &defaults, &removed, data_directories),
        xhtml: handler_for(XHTML, &defaults, &removed, data_directories),
    }
}

impl AssociationReport {
    pub fn lines(&self) -> Vec<String> {
        vec![
            status_line("HTTP", &self.http),
            status_line("HTTPS", &self.https),
            status_line("HTML", &self.html),
            status_line("XHTML", &self.xhtml),
        ]
    }
}

pub fn status_line(kind: &str, handler: &DefaultHandler) -> String {
    match handler {
        DefaultHandler::BrowserPicker => {
            i18n::text_with("{kind}: Browser Picker", &[("{kind}", kind)])
        }
        DefaultHandler::Other { name } => {
            i18n::text_with("{kind}: {name}", &[("{kind}", kind), ("{name}", name)])
        }
        DefaultHandler::Unset => i18n::text_with("{kind}: not set", &[("{kind}", kind)]),
    }
}

pub fn instructions() -> String {
    instructions_for(&env::var("XDG_CURRENT_DESKTOP").unwrap_or_default())
}

pub fn instructions_for(current_desktop: &str) -> String {
    let tokens = current_desktop
        .split(':')
        .map(|token| token.trim().to_ascii_lowercase())
        .collect::<Vec<_>>();
    if tokens.iter().any(|token| token == "gnome") {
        i18n::text(
            "Change these defaults in GNOME Settings \u{2192} Apps \u{2192} Default Apps. Browser Picker never changes system defaults itself.",
        )
    } else if tokens
        .iter()
        .any(|token| token == "kde" || token == "plasma")
    {
        i18n::text(
            "Change these defaults in System Settings \u{2192} Applications \u{2192} Default Applications. Browser Picker never changes system defaults itself.",
        )
    } else {
        i18n::text(
            "Change these defaults in your desktop's default-application settings. Browser Picker never changes system defaults itself.",
        )
    }
}

fn handler_for(
    mime: &str,
    defaults: &HashMap<String, String>,
    removed: &HashMap<String, String>,
    data_directories: &[PathBuf],
) -> DefaultHandler {
    if removed
        .get(mime)
        .is_some_and(|value| !value.trim().is_empty())
        && !defaults.contains_key(mime)
    {
        return DefaultHandler::Unset;
    }
    match defaults.get(mime).map(String::as_str) {
        Some(desktop_id) if discovery::is_picker_desktop_id(desktop_id) => {
            DefaultHandler::BrowserPicker
        }
        Some(desktop_id) => DefaultHandler::Other {
            name: display_name(desktop_id, data_directories),
        },
        None => DefaultHandler::Unset,
    }
}

fn display_name(desktop_id: &str, data_directories: &[PathBuf]) -> String {
    for directory in data_directories {
        let path = directory.join("applications").join(desktop_id);
        if let Some(name) = desktop_file_name(&path) {
            return name;
        }
    }
    desktop_id
        .strip_suffix(".desktop")
        .unwrap_or(desktop_id)
        .to_owned()
}

fn desktop_file_name(path: &Path) -> Option<String> {
    let source = fs::read_to_string(path).ok()?;
    let mut in_entry = false;
    for line in source.lines() {
        let line = line.trim();
        if line == "[Desktop Entry]" {
            in_entry = true;
            continue;
        }
        if line.starts_with('[') {
            in_entry = false;
            continue;
        }
        if in_entry && let Some(name) = line.strip_prefix("Name=") {
            return Some(name.to_owned());
        }
    }
    None
}

fn read_mimeapps(path: &Path) -> (HashMap<String, String>, HashMap<String, String>) {
    let Ok(source) = fs::read_to_string(path) else {
        return (HashMap::new(), HashMap::new());
    };
    let mut defaults = HashMap::new();
    let mut removed = HashMap::new();
    let mut section = "";
    for line in source.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(name) = line
            .strip_prefix('[')
            .and_then(|line| line.strip_suffix(']'))
        {
            section = name;
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let value = value
            .split(';')
            .map(str::trim)
            .find(|part| !part.is_empty())
            .unwrap_or("")
            .to_owned();
        match section {
            "Default Applications" => {
                if !value.is_empty() {
                    defaults.insert(key.to_owned(), value);
                }
            }
            "Removed Associations" => {
                removed.insert(key.to_owned(), value);
            }
            _ => {}
        }
    }
    (defaults, removed)
}

fn config_home() -> PathBuf {
    if let Some(directory) = env::var_os("XDG_CONFIG_HOME") {
        return PathBuf::from(directory);
    }
    env::var_os("HOME")
        .map(|home| PathBuf::from(home).join(".config"))
        .unwrap_or_else(|| PathBuf::from(".config"))
}

fn data_directories() -> Vec<PathBuf> {
    let mut directories = Vec::new();
    if let Some(directory) = env::var_os("XDG_DATA_HOME") {
        directories.push(PathBuf::from(directory));
    } else if let Some(home) = env::var_os("HOME") {
        directories.push(PathBuf::from(home).join(".local/share"));
    }
    if let Some(dirs) = env::var_os("XDG_DATA_DIRS") {
        directories.extend(env::split_paths(&dirs));
    }
    directories
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    use tempfile::TempDir;

    #[test]
    fn mixed_defaults_are_reported_separately_for_http_https_html_and_xhtml() {
        let home = TempDir::new().expect("isolated XDG home should be created");
        let data_home = home.path().join("share");
        let config_home = home.path().join("config");
        let applications = data_home.join("applications");
        fs::create_dir_all(&applications).expect("application directory should be created");
        fs::create_dir_all(&config_home).expect("config directory should be created");

        fs::write(
            applications.join("io.github.TheAnachronism.BrowserPicker.desktop"),
            "[Desktop Entry]\nType=Application\nName=Browser Picker\nExec=browser-picker %U\nMimeType=x-scheme-handler/http;x-scheme-handler/https;text/html;application/xhtml+xml;\n",
        )
        .expect("Browser Picker desktop entry should be writable");
        fs::write(
            applications.join("other-browser.desktop"),
            "[Desktop Entry]\nType=Application\nName=Other Browser\nExec=/bin/true %u\nMimeType=x-scheme-handler/https;text/html;application/xhtml+xml;\n",
        )
        .expect("other desktop entry should be writable");
        fs::write(
            config_home.join("mimeapps.list"),
            "[Default Applications]\nx-scheme-handler/http=io.github.TheAnachronism.BrowserPicker.desktop\nx-scheme-handler/https=other-browser.desktop\ntext/html=other-browser.desktop\n\n[Removed Associations]\napplication/xhtml+xml=io.github.TheAnachronism.BrowserPicker.desktop;other-browser.desktop;\n",
        )
        .expect("mimeapps.list should be writable");

        let report = report_in(&config_home, &[data_home]);
        assert_eq!(report.http, DefaultHandler::BrowserPicker);
        assert_eq!(
            report.https,
            DefaultHandler::Other {
                name: "Other Browser".to_owned()
            }
        );
        assert_eq!(
            report.html,
            DefaultHandler::Other {
                name: "Other Browser".to_owned()
            }
        );
        assert_eq!(report.xhtml, DefaultHandler::Unset);
        assert_eq!(
            report.lines(),
            [
                "HTTP: Browser Picker",
                "HTTPS: Other Browser",
                "HTML: Other Browser",
                "XHTML: not set"
            ]
        );
        let generic = instructions_for("");
        assert!(
            generic.contains("desktop's default-application settings"),
            "{generic}"
        );
        assert!(!generic.to_ascii_lowercase().contains("set as default"));
        assert!(!generic.contains("xdg-settings"));
        assert!(!generic.contains("xdg-mime"));
    }

    #[test]
    fn gnome_and_kde_receive_desktop_appropriate_instructions_without_a_takeover_action() {
        let gnome = instructions_for("GNOME:GNOME-Classic");
        assert!(gnome.contains("GNOME Settings"), "{gnome}");
        assert!(gnome.contains("never changes system defaults"), "{gnome}");
        assert!(!gnome.to_ascii_lowercase().contains("set as default"));

        let kde = instructions_for("KDE");
        assert!(kde.contains("System Settings"), "{kde}");
        assert!(kde.contains("never changes system defaults"), "{kde}");
        assert!(!kde.to_ascii_lowercase().contains("set as default"));

        let plasma = instructions_for("plasma");
        assert_eq!(plasma, kde);
    }
}
