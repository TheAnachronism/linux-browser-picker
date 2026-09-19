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

#[derive(Clone, Debug)]
pub(crate) struct AssociationSearch<'a> {
    pub config_home: &'a Path,
    pub config_dirs: &'a [PathBuf],
    pub data_home: &'a Path,
    pub data_dirs: &'a [PathBuf],
    pub current_desktop: &'a str,
}

pub fn report() -> AssociationReport {
    let config_home = config_home();
    let data_home = data_home();
    let config_dirs = config_dirs();
    let data_dirs = data_directories();
    let current_desktop = env::var("XDG_CURRENT_DESKTOP").unwrap_or_default();
    report_from(&AssociationSearch {
        config_home: &config_home,
        config_dirs: &config_dirs,
        data_home: &data_home,
        data_dirs: &data_dirs,
        current_desktop: &current_desktop,
    })
}

#[cfg(test)]
pub(crate) fn report_in(config_home: &Path, data_directories: &[PathBuf]) -> AssociationReport {
    let data_home = data_directories
        .first()
        .map_or(Path::new(""), PathBuf::as_path);
    report_from(&AssociationSearch {
        config_home,
        config_dirs: &[],
        data_home,
        data_dirs: data_directories,
        current_desktop: "",
    })
}

pub(crate) fn report_from(search: &AssociationSearch<'_>) -> AssociationReport {
    let data_directories = application_data_directories(search);
    let files = mimeapps_files(search);
    AssociationReport {
        http: default_for(HTTP, &files, &data_directories),
        https: default_for(HTTPS, &files, &data_directories),
        html: default_for(HTML, &files, &data_directories),
        xhtml: default_for(XHTML, &files, &data_directories),
    }
}

fn mimeapps_files(search: &AssociationSearch<'_>) -> Vec<PathBuf> {
    let desktops = desktop_names(search.current_desktop);
    let mut files = Vec::new();
    push_config_mimeapps(&mut files, search.config_home, &desktops);
    for directory in search.config_dirs {
        push_config_mimeapps(&mut files, directory, &desktops);
    }
    push_data_mimeapps(&mut files, search.data_home, &desktops);
    for directory in search.data_dirs {
        if directory == search.data_home {
            continue;
        }
        push_data_mimeapps(&mut files, directory, &desktops);
    }
    files
}

fn desktop_names(current_desktop: &str) -> Vec<String> {
    current_desktop
        .split(':')
        .map(|token| token.trim().to_ascii_lowercase())
        .filter(|token| !token.is_empty())
        .collect()
}

fn push_config_mimeapps(files: &mut Vec<PathBuf>, directory: &Path, desktops: &[String]) {
    push_mimeapps_in(files, directory, desktops);
}

fn push_data_mimeapps(files: &mut Vec<PathBuf>, directory: &Path, desktops: &[String]) {
    if directory.as_os_str().is_empty() {
        return;
    }
    push_mimeapps_in(files, &directory.join("applications"), desktops);
}

fn push_mimeapps_in(files: &mut Vec<PathBuf>, directory: &Path, desktops: &[String]) {
    if directory.as_os_str().is_empty() {
        return;
    }
    for desktop in desktops {
        files.push(directory.join(format!("{desktop}-mimeapps.list")));
    }
    files.push(directory.join("mimeapps.list"));
}

fn default_for(mime: &str, files: &[PathBuf], data_directories: &[PathBuf]) -> DefaultHandler {
    for path in files {
        let (defaults, removed) = read_mimeapps(path);
        if defaults.contains_key(mime) {
            return handler_for(mime, &defaults, &removed, data_directories);
        }
    }
    DefaultHandler::Unset
}

fn application_data_directories(search: &AssociationSearch<'_>) -> Vec<PathBuf> {
    let mut directories = Vec::new();
    if !search.data_home.as_os_str().is_empty() {
        directories.push(search.data_home.to_path_buf());
    }
    for directory in search.data_dirs {
        if directories.iter().any(|existing| existing == directory) {
            continue;
        }
        directories.push(directory.clone());
    }
    directories
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

fn config_dirs() -> Vec<PathBuf> {
    split_env_paths("XDG_CONFIG_DIRS").unwrap_or_else(|| vec![PathBuf::from("/etc/xdg")])
}

fn data_home() -> PathBuf {
    if let Some(directory) = env::var_os("XDG_DATA_HOME") {
        return PathBuf::from(directory);
    }
    env::var_os("HOME")
        .map(|home| PathBuf::from(home).join(".local/share"))
        .unwrap_or_else(|| PathBuf::from(".local/share"))
}

fn data_directories() -> Vec<PathBuf> {
    let mut directories = vec![data_home()];
    directories.extend(split_env_paths("XDG_DATA_DIRS").unwrap_or_else(|| {
        vec![
            PathBuf::from("/usr/local/share"),
            PathBuf::from("/usr/share"),
        ]
    }));
    directories
}

fn split_env_paths(key: &str) -> Option<Vec<PathBuf>> {
    let value = env::var_os(key)?;
    let paths: Vec<_> = env::split_paths(&value)
        .filter(|path| path.as_os_str().is_empty() == false)
        .collect();
    (!paths.is_empty()).then_some(paths)
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
    fn system_config_mimeapps_are_used_when_the_user_file_has_no_default() {
        let home = TempDir::new().expect("isolated XDG home should be created");
        let data_home = home.path().join("share");
        let config_home = home.path().join("config");
        let config_dir = home.path().join("etc/xdg");
        let applications = data_home.join("applications");
        fs::create_dir_all(&applications).expect("application directory should be created");
        fs::create_dir_all(&config_home).expect("config directory should be created");
        fs::create_dir_all(&config_dir).expect("system config directory should be created");
        fs::write(
            applications.join("other-browser.desktop"),
            "[Desktop Entry]\nType=Application\nName=Other Browser\nExec=/bin/true %u\nMimeType=x-scheme-handler/http;x-scheme-handler/https;text/html;application/xhtml+xml;\n",
        )
        .expect("other desktop entry should be writable");
        fs::write(
            config_dir.join("mimeapps.list"),
            "[Default Applications]\nx-scheme-handler/http=other-browser.desktop\n",
        )
        .expect("system mimeapps.list should be writable");

        let report = report_from(&AssociationSearch {
            config_home: &config_home,
            config_dirs: &[config_dir],
            data_home: &data_home,
            data_dirs: &[],
            current_desktop: "GNOME",
        });
        assert_eq!(
            report.http,
            DefaultHandler::Other {
                name: "Other Browser".to_owned()
            }
        );
        assert_eq!(report.https, DefaultHandler::Unset);
    }

    fn write_desktop(applications: &Path, desktop_id: &str, name: &str) {
        fs::write(
            applications.join(desktop_id),
            format!(
                "[Desktop Entry]\nType=Application\nName={name}\nExec=/bin/true %u\nMimeType=x-scheme-handler/http;x-scheme-handler/https;text/html;application/xhtml+xml;\n"
            ),
        )
        .expect("desktop entry should be writable");
    }

    fn isolated_search(
        home: &TempDir,
    ) -> (
        std::path::PathBuf,
        std::path::PathBuf,
        std::path::PathBuf,
        std::path::PathBuf,
    ) {
        let data_home = home.path().join("share");
        let config_home = home.path().join("config");
        let config_dir = home.path().join("etc/xdg");
        let applications = data_home.join("applications");
        fs::create_dir_all(&applications).expect("application directory should be created");
        fs::create_dir_all(&config_home).expect("config directory should be created");
        fs::create_dir_all(&config_dir).expect("system config directory should be created");
        write_desktop(
            &applications,
            "io.github.TheAnachronism.BrowserPicker.desktop",
            "Browser Picker",
        );
        write_desktop(&applications, "other-browser.desktop", "Other Browser");
        (config_home, config_dir, data_home, applications)
    }

    #[test]
    fn user_mimeapps_override_system_and_desktop_specific_files_win_for_that_desktop() {
        let home = TempDir::new().expect("isolated XDG home should be created");
        let (config_home, config_dir, data_home, applications) = isolated_search(&home);
        fs::write(
            config_dir.join("mimeapps.list"),
            "[Default Applications]\nx-scheme-handler/http=other-browser.desktop\nx-scheme-handler/https=other-browser.desktop\ntext/html=other-browser.desktop\napplication/xhtml+xml=other-browser.desktop\n",
        )
        .expect("system mimeapps.list should be writable");
        fs::write(
            config_home.join("mimeapps.list"),
            "[Default Applications]\nx-scheme-handler/https=io.github.TheAnachronism.BrowserPicker.desktop\ntext/html=other-browser.desktop\n",
        )
        .expect("user mimeapps.list should be writable");
        fs::write(
            config_home.join("gnome-mimeapps.list"),
            "[Default Applications]\nx-scheme-handler/http=io.github.TheAnachronism.BrowserPicker.desktop\n",
        )
        .expect("desktop-specific mimeapps.list should be writable");
        fs::write(
            applications.join("mimeapps.list"),
            "[Default Applications]\napplication/xhtml+xml=other-browser.desktop\n",
        )
        .expect("data mimeapps.list should be writable");

        let gnome = report_from(&AssociationSearch {
            config_home: &config_home,
            config_dirs: &[config_dir.clone()],
            data_home: &data_home,
            data_dirs: &[],
            current_desktop: "GNOME:GNOME-Classic",
        });
        assert_eq!(gnome.http, DefaultHandler::BrowserPicker);
        assert_eq!(gnome.https, DefaultHandler::BrowserPicker);
        assert_eq!(
            gnome.html,
            DefaultHandler::Other {
                name: "Other Browser".to_owned()
            }
        );
        assert_eq!(
            gnome.xhtml,
            DefaultHandler::Other {
                name: "Other Browser".to_owned()
            }
        );
        assert_eq!(
            gnome.lines(),
            [
                "HTTP: Browser Picker",
                "HTTPS: Browser Picker",
                "HTML: Other Browser",
                "XHTML: Other Browser"
            ]
        );

        let other_desktop = report_from(&AssociationSearch {
            config_home: &config_home,
            config_dirs: &[config_dir],
            data_home: &data_home,
            data_dirs: &[],
            current_desktop: "KDE",
        });
        assert_eq!(
            other_desktop.http,
            DefaultHandler::Other {
                name: "Other Browser".to_owned()
            },
            "a GNOME-only override must not apply on KDE"
        );
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
