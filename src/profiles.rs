use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BrowserFamily {
    Firefox,
    Chromium,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ProfileIdentity {
    Firefox {
        name: String,
        path: PathBuf,
    },
    Chromium {
        user_data_dir: PathBuf,
        profile_directory: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FamilyAssumptions {
    pub family: BrowserFamily,
    pub product: String,
    pub profile_root: PathBuf,
    pub private_flag: &'static str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiscoveryPaths {
    pub home: PathBuf,
    pub config_home: PathBuf,
}

impl DiscoveryPaths {
    pub fn from_env() -> Self {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_default();
        let config_home = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
            .unwrap_or_else(|| home.join(".config"));
        Self { home, config_home }
    }
}

pub fn classify(
    desktop_id: &str,
    executable: &Path,
    paths: &DiscoveryPaths,
) -> Option<FamilyAssumptions> {
    if is_sandboxed(executable) {
        return None;
    }
    let stem = desktop_stem(desktop_id);
    for candidate in FAMILY_CANDIDATES {
        if matches_desktop(&stem, candidate.ids) {
            return Some(FamilyAssumptions {
                family: candidate.family,
                product: candidate.product.to_owned(),
                profile_root: (candidate.profile_root)(paths),
                private_flag: candidate.private_flag,
            });
        }
    }
    None
}

struct FamilyCandidate {
    ids: &'static [&'static str],
    family: BrowserFamily,
    product: &'static str,
    private_flag: &'static str,
    profile_root: fn(&DiscoveryPaths) -> PathBuf,
}

const FAMILY_CANDIDATES: &[FamilyCandidate] = &[
    FamilyCandidate {
        ids: &["firefox", "firefox-default", "org.mozilla.firefox"],
        family: BrowserFamily::Firefox,
        product: "Firefox",
        private_flag: "--private-window",
        profile_root: |paths| paths.home.join(".mozilla/firefox"),
    },
    FamilyCandidate {
        ids: &["firefox-esr"],
        family: BrowserFamily::Firefox,
        product: "Firefox ESR",
        private_flag: "--private-window",
        profile_root: |paths| paths.home.join(".mozilla/firefox"),
    },
    FamilyCandidate {
        ids: &["firefox-developer-edition", "firefox-dev"],
        family: BrowserFamily::Firefox,
        product: "Firefox Developer Edition",
        private_flag: "--private-window",
        profile_root: |paths| paths.home.join(".mozilla/firefox"),
    },
    FamilyCandidate {
        ids: &[
            "librewolf",
            "io.gitlab.librewolf-community",
            "librewolf-community",
        ],
        family: BrowserFamily::Firefox,
        product: "LibreWolf",
        private_flag: "--private-window",
        profile_root: |paths| paths.home.join(".librewolf"),
    },
    FamilyCandidate {
        ids: &[
            "zen",
            "zen-browser",
            "zen-beta",
            "app.zen_browser.zen",
        ],
        family: BrowserFamily::Firefox,
        product: "Zen",
        private_flag: "--private-window",
        profile_root: |paths| paths.home.join(".zen"),
    },
    FamilyCandidate {
        ids: &["chromium", "chromium-browser", "org.chromium.chromium"],
        family: BrowserFamily::Chromium,
        product: "Chromium",
        private_flag: "--incognito",
        profile_root: |paths| paths.config_home.join("chromium"),
    },
    FamilyCandidate {
        ids: &[
            "google-chrome",
            "google-chrome-stable",
            "google-chrome-beta",
            "google-chrome-unstable",
            "com.google.chrome",
        ],
        family: BrowserFamily::Chromium,
        product: "Google Chrome",
        private_flag: "--incognito",
        profile_root: |paths| paths.config_home.join("google-chrome"),
    },
    FamilyCandidate {
        ids: &["brave-browser", "brave", "com.brave.browser"],
        family: BrowserFamily::Chromium,
        product: "Brave",
        private_flag: "--incognito",
        profile_root: |paths| paths.config_home.join("BraveSoftware/Brave-Browser"),
    },
    FamilyCandidate {
        ids: &["vivaldi-stable", "vivaldi", "com.vivaldi.vivaldi"],
        family: BrowserFamily::Chromium,
        product: "Vivaldi",
        private_flag: "--incognito",
        profile_root: |paths| paths.config_home.join("vivaldi"),
    },
    FamilyCandidate {
        ids: &[
            "microsoft-edge",
            "microsoft-edge-stable",
            "microsoft-edge-beta",
            "microsoft-edge-dev",
            "com.microsoft.edge",
        ],
        family: BrowserFamily::Chromium,
        product: "Microsoft Edge",
        private_flag: "--inprivate",
        profile_root: |paths| paths.config_home.join("microsoft-edge"),
    },
];

fn desktop_stem(desktop_id: &str) -> String {
    desktop_id
        .strip_suffix(".desktop")
        .unwrap_or(desktop_id)
        .to_ascii_lowercase()
}

fn matches_desktop(stem: &str, ids: &[&str]) -> bool {
    ids.iter()
        .any(|id| stem == *id || stem.ends_with(&format!(".{id}")))
}

fn is_sandboxed(executable: &Path) -> bool {
    let path = executable.to_string_lossy();
    let name = executable
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    name == "flatpak"
        || name == "snap"
        || name == "snap-exec"
        || path.contains("/snap/")
        || path.contains("/flatpak/")
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiscoveredProfile {
    pub identity: ProfileIdentity,
    pub display_name: String,
}

pub fn discover(assumptions: &FamilyAssumptions) -> Vec<DiscoveredProfile> {
    match assumptions.family {
        BrowserFamily::Firefox => discover_firefox(&assumptions.profile_root),
        BrowserFamily::Chromium => discover_chromium(&assumptions.profile_root),
    }
}

pub fn launch_arguments(
    identity: &ProfileIdentity,
    private: bool,
    private_flag: &str,
    target: &str,
) -> Vec<String> {
    let mut arguments = match identity {
        ProfileIdentity::Firefox { path, .. } => {
            vec!["--profile".to_owned(), path.to_string_lossy().into_owned()]
        }
        ProfileIdentity::Chromium {
            user_data_dir,
            profile_directory,
        } => vec![
            "--user-data-dir".to_owned(),
            user_data_dir.to_string_lossy().into_owned(),
            "--profile-directory".to_owned(),
            profile_directory.clone(),
        ],
    };
    if private {
        arguments.push(private_flag.to_owned());
    }
    arguments.push(target.to_owned());
    arguments
}

pub fn is_present(identity: &ProfileIdentity) -> bool {
    match identity {
        ProfileIdentity::Firefox { path, .. } => path.is_dir(),
        ProfileIdentity::Chromium {
            user_data_dir,
            profile_directory,
        } => user_data_dir.join(profile_directory).is_dir(),
    }
}

impl ProfileIdentity {
    pub fn family(&self) -> BrowserFamily {
        match self {
            Self::Firefox { .. } => BrowserFamily::Firefox,
            Self::Chromium { .. } => BrowserFamily::Chromium,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProfileCapability {
    Verified,
    MissingApplication,
    MissingProfile,
    UnsupportedPackaging,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CapabilityDecision {
    pub capability: ProfileCapability,
    pub assumptions: Option<FamilyAssumptions>,
    pub executable: Option<PathBuf>,
    pub identity: ProfileIdentity,
}

pub fn decide(
    desktop_id: &str,
    executable: Option<&Path>,
    identity: &ProfileIdentity,
    paths: &DiscoveryPaths,
) -> CapabilityDecision {
    let identity = identity.clone();
    let Some(executable) = executable.map(Path::to_path_buf) else {
        return CapabilityDecision {
            capability: ProfileCapability::MissingApplication,
            assumptions: None,
            executable: None,
            identity,
        };
    };
    let Some(assumptions) = classify(desktop_id, &executable, paths) else {
        return CapabilityDecision {
            capability: ProfileCapability::UnsupportedPackaging,
            assumptions: None,
            executable: Some(executable),
            identity,
        };
    };
    if executable.is_absolute() && !executable.is_file() {
        return CapabilityDecision {
            capability: ProfileCapability::MissingApplication,
            assumptions: Some(assumptions),
            executable: Some(executable),
            identity,
        };
    }
    if assumptions.family != identity.family() {
        return CapabilityDecision {
            capability: ProfileCapability::UnsupportedPackaging,
            assumptions: Some(assumptions),
            executable: Some(executable),
            identity,
        };
    }
    let capability = if is_present(&identity) {
        ProfileCapability::Verified
    } else {
        ProfileCapability::MissingProfile
    };
    CapabilityDecision {
        capability,
        assumptions: Some(assumptions),
        executable: Some(executable),
        identity,
    }
}

pub fn capability(
    desktop_id: &str,
    executable: Option<&Path>,
    identity: &ProfileIdentity,
    paths: &DiscoveryPaths,
) -> ProfileCapability {
    decide(desktop_id, executable, identity, paths).capability
}

fn discover_firefox(profile_root: &Path) -> Vec<DiscoveredProfile> {
    let Ok(source) = std::fs::read_to_string(profile_root.join("profiles.ini")) else {
        return Vec::new();
    };
    let mut profiles = Vec::new();
    let mut name = None;
    let mut path = None;
    let mut relative = true;
    let mut in_profile = false;

    let finish_section = |name: &mut Option<String>,
                          path: &mut Option<String>,
                          relative: bool,
                          in_profile: bool,
                          profiles: &mut Vec<DiscoveredProfile>| {
        if !in_profile {
            return;
        }
        let Some(path) = path.take() else {
            name.take();
            return;
        };
        let resolved = if relative {
            profile_root.join(&path)
        } else {
            PathBuf::from(&path)
        };
        let display_name = name.take().unwrap_or_else(|| path.clone());
        profiles.push(DiscoveredProfile {
            identity: ProfileIdentity::Firefox {
                name: display_name.clone(),
                path: resolved,
            },
            display_name,
        });
    };

    for line in source.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        if let Some(section) = line
            .strip_prefix('[')
            .and_then(|line| line.strip_suffix(']'))
        {
            finish_section(&mut name, &mut path, relative, in_profile, &mut profiles);
            in_profile = section.starts_with("Profile");
            name = None;
            path = None;
            relative = true;
            continue;
        }
        if !in_profile {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        match key.trim() {
            "Name" => name = Some(value.trim().to_owned()),
            "Path" => path = Some(value.trim().to_owned()),
            "IsRelative" => relative = value.trim() != "0",
            _ => {}
        }
    }
    finish_section(&mut name, &mut path, relative, in_profile, &mut profiles);
    profiles
}

fn discover_chromium(profile_root: &Path) -> Vec<DiscoveredProfile> {
    let Ok(source) = std::fs::read_to_string(profile_root.join("Local State")) else {
        return Vec::new();
    };
    let Ok(state) = serde_json::from_str::<LocalState>(&source) else {
        return Vec::new();
    };
    let Some(cache) = state.profile.and_then(|profile| profile.info_cache) else {
        return Vec::new();
    };
    cache
        .into_iter()
        .map(|(profile_directory, info)| {
            let display_name = info
                .name
                .filter(|name| !name.is_empty())
                .unwrap_or_else(|| profile_directory.clone());
            DiscoveredProfile {
                identity: ProfileIdentity::Chromium {
                    user_data_dir: profile_root.to_path_buf(),
                    profile_directory,
                },
                display_name,
            }
        })
        .collect()
}

#[derive(Deserialize)]
struct LocalState {
    #[serde(default)]
    profile: Option<LocalStateProfile>,
}

#[derive(Deserialize)]
struct LocalStateProfile {
    #[serde(default)]
    info_cache: Option<BTreeMap<String, ChromiumProfileInfo>>,
}

#[derive(Deserialize)]
struct ChromiumProfileInfo {
    #[serde(default)]
    name: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn firefox_profiles_ini_preserves_native_name_and_path() {
        let home = tempfile::TempDir::new().expect("temporary home should be created");
        let root = home.path().join(".mozilla/firefox");
        fs::create_dir_all(root.join("abcd1234.default")).expect("relative profile should exist");
        fs::write(
            root.join("profiles.ini"),
            "[General]\nStartWithLastProfile=1\n\n[InstallDEADBEEF]\nDefault=abcd1234.default\n\n[Profile0]\nName=default-release\nIsRelative=1\nPath=abcd1234.default\nDefault=1\n\n[Profile1]\nName=Work\nIsRelative=0\nPath=/srv/firefox/work\n",
        )
        .expect("profiles.ini should be writable");

        let profiles = discover(&FamilyAssumptions {
            family: BrowserFamily::Firefox,
            product: "Firefox".to_owned(),
            profile_root: root.clone(),
            private_flag: "--private-window",
        });

        assert_eq!(
            profiles,
            vec![
                DiscoveredProfile {
                    identity: ProfileIdentity::Firefox {
                        name: "default-release".to_owned(),
                        path: root.join("abcd1234.default"),
                    },
                    display_name: "default-release".to_owned(),
                },
                DiscoveredProfile {
                    identity: ProfileIdentity::Firefox {
                        name: "Work".to_owned(),
                        path: PathBuf::from("/srv/firefox/work"),
                    },
                    display_name: "Work".to_owned(),
                },
            ]
        );
    }

    #[test]
    fn chromium_local_state_preserves_user_data_root_and_profile_id() {
        let home = tempfile::TempDir::new().expect("temporary home should be created");
        let root = home.path().join(".config/chromium");
        fs::create_dir_all(root.join("Default")).expect("default profile should exist");
        fs::create_dir_all(root.join("Profile 1")).expect("named profile should exist");
        fs::write(
            root.join("Local State"),
            r#"{"profile":{"info_cache":{"Default":{"name":"Person 1"},"Profile 1":{"name":"Work"}}}}"#,
        )
        .expect("Local State should be writable");

        let profiles = discover(&FamilyAssumptions {
            family: BrowserFamily::Chromium,
            product: "Chromium".to_owned(),
            profile_root: root.clone(),
            private_flag: "--incognito",
        });

        assert_eq!(
            profiles,
            vec![
                DiscoveredProfile {
                    identity: ProfileIdentity::Chromium {
                        user_data_dir: root.clone(),
                        profile_directory: "Default".to_owned(),
                    },
                    display_name: "Person 1".to_owned(),
                },
                DiscoveredProfile {
                    identity: ProfileIdentity::Chromium {
                        user_data_dir: root,
                        profile_directory: "Profile 1".to_owned(),
                    },
                    display_name: "Work".to_owned(),
                },
            ]
        );
    }

    #[test]
    fn launch_arguments_use_reuse_safe_profile_flags() {
        let firefox = ProfileIdentity::Firefox {
            name: "Work".to_owned(),
            path: PathBuf::from("/srv/firefox/work"),
        };
        assert_eq!(
            launch_arguments(&firefox, false, "--private-window", "https://example.com/"),
            vec![
                "--profile".to_owned(),
                "/srv/firefox/work".to_owned(),
                "https://example.com/".to_owned(),
            ]
        );
        assert_eq!(
            launch_arguments(&firefox, true, "--private-window", "https://example.com/"),
            vec![
                "--profile".to_owned(),
                "/srv/firefox/work".to_owned(),
                "--private-window".to_owned(),
                "https://example.com/".to_owned(),
            ]
        );

        let chromium = ProfileIdentity::Chromium {
            user_data_dir: PathBuf::from("/home/user/.config/chromium"),
            profile_directory: "Profile 1".to_owned(),
        };
        assert_eq!(
            launch_arguments(&chromium, false, "--incognito", "https://example.com/"),
            vec![
                "--user-data-dir".to_owned(),
                "/home/user/.config/chromium".to_owned(),
                "--profile-directory".to_owned(),
                "Profile 1".to_owned(),
                "https://example.com/".to_owned(),
            ]
        );
        let edge = ProfileIdentity::Chromium {
            user_data_dir: PathBuf::from("/home/user/.config/microsoft-edge"),
            profile_directory: "Default".to_owned(),
        };
        assert_eq!(
            launch_arguments(&edge, true, "--inprivate", "https://example.com/"),
            vec![
                "--user-data-dir".to_owned(),
                "/home/user/.config/microsoft-edge".to_owned(),
                "--profile-directory".to_owned(),
                "Default".to_owned(),
                "--inprivate".to_owned(),
                "https://example.com/".to_owned(),
            ]
        );
        assert!(
            !launch_arguments(&firefox, false, "--private-window", "https://example.com/")
                .iter()
                .any(|argument| argument == "-no-remote" || argument == "--new-instance")
        );
    }

    #[test]
    fn discovery_does_not_create_or_repair_profile_files() {
        let home = tempfile::TempDir::new().expect("temporary home should be created");
        let root = home.path().join(".mozilla/firefox");
        fs::create_dir_all(&root).expect("profile root should exist");
        let profiles = discover(&FamilyAssumptions {
            family: BrowserFamily::Firefox,
            product: "Firefox".to_owned(),
            profile_root: root.clone(),
            private_flag: "--private-window",
        });
        assert!(profiles.is_empty());
        assert!(!root.join("profiles.ini").exists());
        assert_eq!(
            fs::read_dir(&root)
                .expect("root should remain readable")
                .count(),
            0
        );
    }

    #[test]
    fn known_families_map_desktop_ids_to_roots_and_private_flags() {
        let paths = DiscoveryPaths {
            home: PathBuf::from("/home/user"),
            config_home: PathBuf::from("/home/user/.config"),
        };
        let cases = [
            (
                "firefox.desktop",
                "/usr/bin/firefox",
                "Firefox",
                BrowserFamily::Firefox,
                "/home/user/.mozilla/firefox",
                "--private-window",
            ),
            (
                "firefox-esr.desktop",
                "/usr/bin/firefox-esr",
                "Firefox ESR",
                BrowserFamily::Firefox,
                "/home/user/.mozilla/firefox",
                "--private-window",
            ),
            (
                "firefox-developer-edition.desktop",
                "/usr/bin/firefox-developer-edition",
                "Firefox Developer Edition",
                BrowserFamily::Firefox,
                "/home/user/.mozilla/firefox",
                "--private-window",
            ),
            (
                "librewolf.desktop",
                "/usr/bin/librewolf",
                "LibreWolf",
                BrowserFamily::Firefox,
                "/home/user/.librewolf",
                "--private-window",
            ),
            (
                "zen.desktop",
                "/usr/bin/zen",
                "Zen",
                BrowserFamily::Firefox,
                "/home/user/.zen",
                "--private-window",
            ),
            (
                "zen-browser.desktop",
                "/usr/bin/zen-browser",
                "Zen",
                BrowserFamily::Firefox,
                "/home/user/.zen",
                "--private-window",
            ),
            (
                "zen-beta.desktop",
                "/usr/bin/zen-beta",
                "Zen",
                BrowserFamily::Firefox,
                "/home/user/.zen",
                "--private-window",
            ),
            (
                "chromium.desktop",
                "/usr/bin/chromium",
                "Chromium",
                BrowserFamily::Chromium,
                "/home/user/.config/chromium",
                "--incognito",
            ),
            (
                "google-chrome.desktop",
                "/usr/bin/google-chrome",
                "Google Chrome",
                BrowserFamily::Chromium,
                "/home/user/.config/google-chrome",
                "--incognito",
            ),
            (
                "brave-browser.desktop",
                "/usr/bin/brave-browser",
                "Brave",
                BrowserFamily::Chromium,
                "/home/user/.config/BraveSoftware/Brave-Browser",
                "--incognito",
            ),
            (
                "vivaldi-stable.desktop",
                "/usr/bin/vivaldi-stable",
                "Vivaldi",
                BrowserFamily::Chromium,
                "/home/user/.config/vivaldi",
                "--incognito",
            ),
            (
                "microsoft-edge.desktop",
                "/usr/bin/microsoft-edge",
                "Microsoft Edge",
                BrowserFamily::Chromium,
                "/home/user/.config/microsoft-edge",
                "--inprivate",
            ),
        ];
        for (desktop_id, executable, product, family, root, private_flag) in cases {
            let assumptions = classify(desktop_id, Path::new(executable), &paths)
                .unwrap_or_else(|| panic!("{desktop_id} should be a known family"));
            assert_eq!(assumptions.family, family, "{desktop_id}");
            assert_eq!(assumptions.product, product, "{desktop_id}");
            assert_eq!(assumptions.profile_root, Path::new(root), "{desktop_id}");
            assert_eq!(assumptions.private_flag, private_flag, "{desktop_id}");
        }
    }

    #[test]
    fn snap_flatpak_and_unknown_packaging_stay_generic() {
        let paths = DiscoveryPaths {
            home: PathBuf::from("/home/user"),
            config_home: PathBuf::from("/home/user/.config"),
        };
        assert_eq!(
            classify("firefox.desktop", Path::new("/usr/bin/flatpak"), &paths),
            None
        );
        assert_eq!(
            classify(
                "firefox_firefox.desktop",
                Path::new("/snap/bin/firefox"),
                &paths
            ),
            None
        );
        assert_eq!(
            classify(
                "org.mozilla.firefox.desktop",
                Path::new("/var/lib/flatpak/exports/bin/org.mozilla.firefox"),
                &paths
            ),
            None
        );
        assert_eq!(
            classify(
                "unknown-browser.desktop",
                Path::new("/usr/bin/unknown-browser"),
                &paths
            ),
            None
        );
    }

    #[test]
    fn sandboxed_packaging_is_unsupported_for_saved_profile_identities() {
        let paths = DiscoveryPaths {
            home: PathBuf::from("/home/user"),
            config_home: PathBuf::from("/home/user/.config"),
        };
        let identity = ProfileIdentity::Firefox {
            name: "Work".to_owned(),
            path: PathBuf::from("/srv/firefox/work"),
        };
        assert_eq!(
            capability(
                "firefox.desktop",
                Some(Path::new("/snap/bin/firefox")),
                &identity,
                &paths,
            ),
            ProfileCapability::UnsupportedPackaging
        );
        assert_eq!(
            capability("firefox.desktop", None, &identity, &paths),
            ProfileCapability::MissingApplication
        );
    }

    #[test]
    fn current_zen_desktop_ids_use_firefox_family_without_unknown_derivatives() {
        let paths = DiscoveryPaths {
            home: PathBuf::from("/home/user"),
            config_home: PathBuf::from("/home/user/.config"),
        };
        let assumptions = classify("zen-beta.desktop", Path::new("/usr/bin/zen-beta"), &paths)
            .expect("current primary Zen should use the Firefox-family adapter");
        assert_eq!(assumptions.family, BrowserFamily::Firefox);
        assert_eq!(assumptions.product, "Zen");
        assert_eq!(assumptions.profile_root, Path::new("/home/user/.zen"));
        assert_eq!(assumptions.private_flag, "--private-window");
        assert_eq!(
            classify(
                "zen-secondary.desktop",
                Path::new("/usr/bin/zen-secondary"),
                &paths
            ),
            None,
            "custom-profile Zen wrappers stay generic destinations"
        );
        assert_eq!(
            classify(
                "zen-twilight.desktop",
                Path::new("/usr/bin/zen-twilight"),
                &paths
            ),
            None,
            "unknown Zen derivatives must stay generic destinations"
        );
    }

    #[test]
    fn stale_executable_is_a_missing_application_not_a_verified_profile() {
        let home = tempfile::TempDir::new().expect("temporary home should be created");
        let profile = home.path().join("work");
        std::fs::create_dir(&profile).expect("profile should exist");
        let paths = DiscoveryPaths {
            home: home.path().to_path_buf(),
            config_home: home.path().join(".config"),
        };
        let identity = ProfileIdentity::Firefox {
            name: "Work".to_owned(),
            path: profile,
        };
        let missing = home.path().join("gone-firefox");
        assert_eq!(
            capability(
                "firefox.desktop",
                Some(missing.as_path()),
                &identity,
                &paths,
            ),
            ProfileCapability::MissingApplication
        );
    }

    #[test]
    fn setup_and_dispatch_share_family_packaging_executable_locator_and_private_capability() {
        let home = tempfile::TempDir::new().expect("temporary home should be created");
        let profile = home.path().join("work");
        std::fs::create_dir(&profile).expect("profile should exist");
        let executable = home.path().join("firefox");
        std::fs::write(&executable, b"#!/bin/sh\n").expect("executable should be writable");
        let paths = DiscoveryPaths {
            home: home.path().to_path_buf(),
            config_home: home.path().join(".config"),
        };
        let identity = ProfileIdentity::Firefox {
            name: "Work".to_owned(),
            path: profile.clone(),
        };

        let verified = decide(
            "firefox.desktop",
            Some(executable.as_path()),
            &identity,
            &paths,
        );
        assert_eq!(verified.capability, ProfileCapability::Verified);
        let assumptions = verified
            .assumptions
            .expect("verified profiles expose family assumptions");
        assert_eq!(assumptions.family, BrowserFamily::Firefox);
        assert_eq!(assumptions.private_flag, "--private-window");
        assert_eq!(verified.executable.as_deref(), Some(executable.as_path()));
        assert_eq!(verified.identity, identity);

        let stale = decide(
            "firefox.desktop",
            Some(home.path().join("gone").as_path()),
            &identity,
            &paths,
        );
        assert_eq!(stale.capability, ProfileCapability::MissingApplication);
        assert_eq!(
            stale.assumptions.as_ref().map(|item| item.family),
            Some(BrowserFamily::Firefox)
        );

        let sandboxed = decide(
            "firefox.desktop",
            Some(Path::new("/snap/bin/firefox")),
            &identity,
            &paths,
        );
        assert_eq!(
            sandboxed.capability,
            ProfileCapability::UnsupportedPackaging
        );
        assert!(sandboxed.assumptions.is_none());

        let zen = decide(
            "zen-beta.desktop",
            Some(executable.as_path()),
            &identity,
            &paths,
        );
        assert_eq!(zen.capability, ProfileCapability::Verified);
        assert_eq!(
            zen.assumptions.as_ref().map(|item| item.product.as_str()),
            Some("Zen")
        );
        assert_eq!(
            zen.assumptions.as_ref().map(|item| item.private_flag),
            Some("--private-window")
        );
    }
}
