use std::env;

use gtk::gio;
use gtk::gio::prelude::AppInfoExt;

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
    AssociationReport {
        http: default_for(HTTP),
        https: default_for(HTTPS),
        html: default_for(HTML),
        xhtml: default_for(XHTML),
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

fn default_for(content_type: &str) -> DefaultHandler {
    match gio::AppInfo::default_for_type(content_type, false) {
        Some(application) if discovery::is_browser_picker(&application) => {
            DefaultHandler::BrowserPicker
        }
        Some(application) => DefaultHandler::Other {
            name: application.display_name().to_string(),
        },
        None => DefaultHandler::Unset,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

        let generic = instructions_for("");
        assert!(
            generic.contains("desktop's default-application settings"),
            "{generic}"
        );
        assert!(!generic.to_ascii_lowercase().contains("set as default"));
        assert!(!generic.contains("xdg-settings"));
        assert!(!generic.contains("xdg-mime"));
    }
}
