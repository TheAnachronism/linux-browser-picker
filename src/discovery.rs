use std::collections::{HashMap, HashSet};

use gtk::gio;
use gtk::gio::prelude::AppInfoExt;

use crate::application::ID;

pub const HTTP: &str = "x-scheme-handler/http";
pub const HTTPS: &str = "x-scheme-handler/https";

#[derive(Clone, Debug)]
pub struct BrowserCandidate {
    pub desktop_id: String,
    pub name: String,
    pub icon: Option<gio::Icon>,
}

#[derive(Clone, Debug)]
pub struct PartialHandler {
    pub candidate: BrowserCandidate,
    pub http: bool,
    pub https: bool,
}

#[derive(Clone, Debug)]
pub struct Discovery {
    pub ordinary: Vec<BrowserCandidate>,
    pub partial: Vec<PartialHandler>,
}

pub fn discover() -> Discovery {
    let http = apps_by_id(gio::AppInfo::all_for_type(HTTP));
    let https = apps_by_id(gio::AppInfo::all_for_type(HTTPS));
    let mut ordinary = Vec::new();
    let mut partial = Vec::new();
    let mut seen = HashSet::new();

    for id in http.keys().chain(https.keys()) {
        if !seen.insert(id.clone()) {
            continue;
        }
        let Some(application) = http.get(id).or_else(|| https.get(id)) else {
            continue;
        };
        if is_browser_picker(application) || !application.should_show() {
            continue;
        }
        let candidate = candidate_from(application, id);
        let has_http = http.contains_key(id);
        let has_https = https.contains_key(id);
        if has_http && has_https {
            ordinary.push(candidate);
        } else {
            partial.push(PartialHandler {
                candidate,
                http: has_http,
                https: has_https,
            });
        }
    }

    ordinary.sort_by(|left, right| {
        left.name
            .cmp(&right.name)
            .then(left.desktop_id.cmp(&right.desktop_id))
    });
    partial.sort_by(|left, right| {
        left.candidate
            .name
            .cmp(&right.candidate.name)
            .then(left.candidate.desktop_id.cmp(&right.candidate.desktop_id))
    });
    Discovery { ordinary, partial }
}

pub fn app_info(desktop_id: &str) -> Option<gio::AppInfo> {
    let mut applications = gio::AppInfo::all_for_type(HTTP);
    applications.extend(gio::AppInfo::all_for_type(HTTPS));
    if let Some(application) = applications
        .into_iter()
        .find(|application| application.id().as_deref() == Some(desktop_id))
    {
        return Some(application);
    }
    gio::AppInfo::all()
        .into_iter()
        .find(|application| application.id().as_deref() == Some(desktop_id))
}

pub fn icon(desktop_id: &str) -> Option<gio::Icon> {
    app_info(desktop_id).and_then(|application| application.icon())
}

pub fn suggested_slug(desktop_id: &str) -> String {
    let stem = desktop_id.strip_suffix(".desktop").unwrap_or(desktop_id);
    let last = stem.rsplit('.').next().unwrap_or(stem);
    let mut slug = String::new();
    for character in last.chars() {
        if character.is_ascii_lowercase() || character.is_ascii_digit() {
            slug.push(character);
        } else if character.is_ascii_uppercase() {
            slug.push(character.to_ascii_lowercase());
        } else if (character == '_' || character == '-') && !slug.is_empty() && !slug.ends_with('-')
        {
            slug.push('-');
        }
    }
    let slug = slug.trim_matches('-');
    if slug.is_empty() {
        "destination".to_owned()
    } else {
        slug.to_owned()
    }
}

pub fn unique_slug(base: &str, used: &mut HashSet<String>) -> String {
    let slug = if base.is_empty() {
        "destination".to_owned()
    } else {
        base.to_owned()
    };
    if used.insert(slug.clone()) {
        return slug;
    }
    let mut suffix = 2;
    loop {
        let candidate = format!("{slug}-{suffix}");
        if used.insert(candidate.clone()) {
            return candidate;
        }
        suffix += 1;
    }
}

fn apps_by_id(applications: Vec<gio::AppInfo>) -> HashMap<String, gio::AppInfo> {
    let mut map = HashMap::new();
    for application in applications {
        if let Some(id) = application.id() {
            map.entry(id.to_string()).or_insert(application);
        }
    }
    map
}

pub(crate) fn is_picker_desktop_id(desktop_id: &str) -> bool {
    desktop_id == ID || desktop_id == format!("{ID}.desktop")
}

pub(crate) fn is_browser_picker(application: &gio::AppInfo) -> bool {
    application.id().is_some_and(|id| is_picker_desktop_id(&id))
        || application
            .executable()
            .file_name()
            .is_some_and(|name| name == "browser-picker")
}

fn candidate_from(application: &gio::AppInfo, id: &str) -> BrowserCandidate {
    BrowserCandidate {
        desktop_id: id.to_owned(),
        name: application.display_name().to_string(),
        icon: application.icon(),
    }
}
