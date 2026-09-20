mod store;

use std::collections::HashSet;
use std::env;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use toml_edit::{DocumentMut, Item};

use crate::discovery;
use crate::i18n;
use crate::profiles::{self, ProfileIdentity};

pub use store::{ConfigurationStore, SaveConflictPolicy, StoreStatus};

pub const CURRENT_VERSION: u32 = 1;

#[derive(Clone, Debug)]
pub struct BrowserDestination {
    pub id: String,
    pub label: String,
    pub application_label: String,
    pub profile_label: Option<String>,
    pub icon_name: Option<String>,
    pub launch: DestinationLaunch,
    pub unavailable_reason: Option<String>,
}

#[derive(Clone, Debug)]
pub enum DestinationLaunch {
    Manual {
        executable: String,
        arguments: Vec<String>,
        private_arguments: Option<Vec<String>>,
    },
    Discovered {
        desktop_id: String,
    },
    FirefoxProfile {
        desktop_id: String,
        name: String,
        path: String,
    },
    ChromiumProfile {
        desktop_id: String,
        user_data_dir: String,
        profile_directory: String,
    },
}

impl DestinationLaunch {
    pub fn desktop_id(&self) -> Option<&str> {
        match self {
            Self::Discovered { desktop_id }
            | Self::FirefoxProfile { desktop_id, .. }
            | Self::ChromiumProfile { desktop_id, .. } => Some(desktop_id),
            Self::Manual { .. } => None,
        }
    }

    pub fn profile_identity(&self) -> Option<ProfileIdentity> {
        match self {
            Self::FirefoxProfile { name, path, .. } => Some(ProfileIdentity::Firefox {
                name: name.clone(),
                path: path.into(),
            }),
            Self::ChromiumProfile {
                user_data_dir,
                profile_directory,
                ..
            } => Some(ProfileIdentity::Chromium {
                user_data_dir: user_data_dir.into(),
                profile_directory: profile_directory.clone(),
            }),
            Self::Manual { .. } | Self::Discovered { .. } => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FallbackAction {
    Open {
        destination: String,
        mode: LaunchMode,
    },
    ShowPicker,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum LaunchMode {
    #[default]
    Normal,
    Private,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "kebab-case", deny_unknown_fields)]
pub enum RoutingAction {
    Open {
        destination: String,
        #[serde(default)]
        mode: LaunchMode,
    },
    Preselect {
        destination: String,
        #[serde(default)]
        mode: LaunchMode,
    },
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PathComparison {
    #[default]
    Exact,
    Prefix,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "kebab-case", deny_unknown_fields)]
pub enum UrlCondition {
    Scheme {
        value: String,
        #[serde(default)]
        negate: bool,
    },
    Host {
        value: String,
        #[serde(default)]
        include_subdomains: bool,
        #[serde(default)]
        negate: bool,
    },
    Port {
        value: u16,
        #[serde(default)]
        negate: bool,
    },
    Path {
        value: String,
        #[serde(default)]
        comparison: PathComparison,
        #[serde(default)]
        case_insensitive: bool,
        #[serde(default)]
        negate: bool,
    },
    QueryKey {
        key: String,
        #[serde(default)]
        case_insensitive: bool,
        #[serde(default)]
        negate: bool,
    },
    QueryValue {
        key: String,
        value: String,
        #[serde(default)]
        case_insensitive: bool,
        #[serde(default)]
        negate: bool,
    },
    Glob {
        value: String,
        #[serde(default)]
        case_insensitive: bool,
        #[serde(default)]
        negate: bool,
    },
    Regex {
        value: String,
        #[serde(default)]
        case_insensitive: bool,
        #[serde(default)]
        negate: bool,
    },
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ConditionGroup {
    pub conditions: Vec<UrlCondition>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RoutingRule {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub groups: Vec<ConditionGroup>,
    pub action: RoutingAction,
}

#[derive(Clone, Debug)]
pub struct Configuration {
    pub destinations: Vec<BrowserDestination>,
    pub rules: Vec<RoutingRule>,
    pub fallback: FallbackAction,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MigrationPreview {
    pub from: u32,
    pub to: u32,
    pub changes: Vec<String>,
}

impl MigrationPreview {
    pub fn message(&self) -> String {
        let changes = self.changes.join("; ");
        i18n::text_with(
            "Migrate configuration schema from version {from} to {to}: {changes}",
            &[
                ("{from}", &self.from.to_string()),
                ("{to}", &self.to.to_string()),
                ("{changes}", &changes),
            ],
        )
    }
}

#[derive(Clone, Debug)]
pub enum Inspected {
    Missing,
    Ready(Configuration),
    Invalid(Error),
    Migratable {
        configuration: Configuration,
        preview: MigrationPreview,
    },
}

#[derive(Clone, Debug)]
pub enum Error {
    ConfigHomeNotAbsolute,
    HomeNotSet,
    Read(ErrorKind),
    Save(ErrorKind),
    InvalidToml,
    UnknownKey { key: String, line: Option<usize> },
    DuplicateField { key: String, line: Option<usize> },
    InvalidType { key: String, line: Option<usize> },
    MissingField(String),
    UnsupportedVersion(u32),
    MigrationRequired { from: u32, to: u32 },
    InvalidId(String),
    DuplicateId(String),
    InvalidLabel(String),
    DuplicateLabel(String),
    InvalidRuleId(String),
    DuplicateRuleId(String),
    InvalidRuleName(String),
    DuplicateRuleName(String),
    InvalidRuleScheme(String),
    InvalidRuleHost(String),
    EmptyRuleGroups(String),
    EmptyConditionGroup(String),
    InvalidRuleGlob(String, String),
    InvalidRuleRegex(String, String),
    UnknownDestination(String),
    UnsupportedPrivateMode(String),
    UnknownFallback(String),
    InvalidExecutable(String),
    InvalidTargetTemplate(String),
    InvalidPrivateTargetTemplate(String),
    InvalidDesktopId(String),
    InvalidProfileIdentity(String),
    Conflict,
    NotRegularFile,
    NotOwned,
    BrokenLink,
    Backup(ErrorKind),
}

impl BrowserDestination {
    pub fn supports_private(&self) -> bool {
        match &self.launch {
            DestinationLaunch::Manual {
                private_arguments: Some(_),
                ..
            }
            | DestinationLaunch::FirefoxProfile { .. }
            | DestinationLaunch::ChromiumProfile { .. } => true,
            DestinationLaunch::Manual {
                private_arguments: None,
                ..
            }
            | DestinationLaunch::Discovered { .. } => false,
        }
    }

    pub fn desktop_id(&self) -> Option<&str> {
        self.launch.desktop_id()
    }

    pub fn is_available(&self) -> bool {
        self.unavailable_reason.is_none()
    }
}

impl Error {
    pub fn message(&self) -> String {
        match self {
            Self::ConfigHomeNotAbsolute => i18n::text("XDG_CONFIG_HOME must be an absolute path"),
            Self::HomeNotSet => i18n::text("HOME is not set"),
            Self::Read(ErrorKind::NotFound) => {
                i18n::text("Browser Picker configuration was not found")
            }
            Self::Read(ErrorKind::PermissionDenied) => {
                i18n::text("Browser Picker configuration permission was denied")
            }
            Self::Read(_) => i18n::text("Browser Picker configuration could not be read"),
            Self::Save(ErrorKind::PermissionDenied) => {
                i18n::text("Browser Picker configuration permission was denied")
            }
            Self::Save(_) => i18n::text("Browser Picker configuration could not be saved"),
            Self::InvalidToml => i18n::text("Configuration is not valid versioned TOML"),
            Self::UnknownKey {
                key,
                line: Some(line),
            } => i18n::text_with(
                "Unknown configuration key '{key}' at line {line}",
                &[("{key}", key), ("{line}", &line.to_string())],
            ),
            Self::UnknownKey { key, line: None } => {
                i18n::text_with("Unknown configuration key '{key}'", &[("{key}", key)])
            }
            Self::DuplicateField {
                key,
                line: Some(line),
            } => i18n::text_with(
                "Duplicate configuration key '{key}' at line {line}",
                &[("{key}", key), ("{line}", &line.to_string())],
            ),
            Self::DuplicateField { key, line: None } => {
                i18n::text_with("Duplicate configuration key '{key}'", &[("{key}", key)])
            }
            Self::InvalidType {
                key,
                line: Some(line),
            } if !key.is_empty() => i18n::text_with(
                "Configuration key '{key}' has an invalid type at line {line}",
                &[("{key}", key), ("{line}", &line.to_string())],
            ),
            Self::InvalidType {
                key: _,
                line: Some(line),
            } => i18n::text_with(
                "Configuration has an invalid type at line {line}",
                &[("{line}", &line.to_string())],
            ),
            Self::InvalidType { key, line: None } if !key.is_empty() => i18n::text_with(
                "Configuration key '{key}' has an invalid type",
                &[("{key}", key)],
            ),
            Self::InvalidType { .. } => i18n::text("Configuration has an invalid type"),
            Self::MissingField(key) => {
                i18n::text_with("Missing configuration field '{key}'", &[("{key}", key)])
            }
            Self::UnsupportedVersion(version) => i18n::text_with(
                "Unsupported configuration version {version}; expected version {expected}",
                &[
                    ("{version}", &version.to_string()),
                    ("{expected}", &CURRENT_VERSION.to_string()),
                ],
            ),
            Self::MigrationRequired { from, to } => i18n::text_with(
                "Configuration schema version {from} requires a confirmed migration to version {to}",
                &[("{from}", &from.to_string()), ("{to}", &to.to_string())],
            ),
            Self::InvalidId(id) => i18n::text_with(
                "Browser Destination ID '{id}' must be a lowercase slug",
                &[("{id}", id)],
            ),
            Self::DuplicateId(id) => i18n::text_with(
                "Browser Destination ID '{id}' is not unique",
                &[("{id}", id)],
            ),
            Self::InvalidLabel(id) => i18n::text_with(
                "Browser Destination '{id}' must have a non-empty display label",
                &[("{id}", id)],
            ),
            Self::DuplicateLabel(label) => i18n::text_with(
                "Browser Destination display label '{label}' is not unique",
                &[("{label}", label)],
            ),
            Self::InvalidRuleId(id) => i18n::text_with(
                "Routing Rule ID '{id}' must be a lowercase slug",
                &[("{id}", id)],
            ),
            Self::DuplicateRuleId(id) => {
                i18n::text_with("Routing Rule ID '{id}' is not unique", &[("{id}", id)])
            }
            Self::InvalidRuleName(id) => i18n::text_with(
                "Routing Rule '{id}' must have a non-empty unique name",
                &[("{id}", id)],
            ),
            Self::DuplicateRuleName(name) => i18n::text_with(
                "Routing Rule name '{name}' is not unique",
                &[("{name}", name)],
            ),
            Self::InvalidRuleScheme(id) => i18n::text_with(
                "Routing Rule '{id}' scheme must be HTTP or HTTPS",
                &[("{id}", id)],
            ),
            Self::InvalidRuleHost(id) => i18n::text_with(
                "Routing Rule '{id}' host must be a valid domain or IP address",
                &[("{id}", id)],
            ),
            Self::EmptyRuleGroups(id) => i18n::text_with(
                "Routing Rule '{id}' must contain at least one condition group",
                &[("{id}", id)],
            ),
            Self::EmptyConditionGroup(id) => i18n::text_with(
                "Routing Rule '{id}' condition groups must not be empty",
                &[("{id}", id)],
            ),
            Self::InvalidRuleGlob(id, reason) => i18n::text_with(
                "Routing Rule '{id}' glob is invalid: {reason}",
                &[("{id}", id), ("{reason}", reason)],
            ),
            Self::InvalidRuleRegex(id, reason) => i18n::text_with(
                "Routing Rule '{id}' regular expression is invalid: {reason}",
                &[("{id}", id), ("{reason}", reason)],
            ),
            Self::UnknownDestination(id) => i18n::text_with(
                "Routing action references unknown destination '{id}'",
                &[("{id}", id)],
            ),
            Self::UnsupportedPrivateMode(id) => i18n::text_with(
                "Browser Destination '{id}' does not support private Launch Mode",
                &[("{id}", id)],
            ),
            Self::UnknownFallback(id) => i18n::text_with(
                "Fallback references unknown destination '{id}'",
                &[("{id}", id)],
            ),
            Self::InvalidExecutable(id) => i18n::text_with(
                "Manual destination '{id}' executable must be an absolute path or a PATH-resolved name",
                &[("{id}", id)],
            ),
            Self::InvalidTargetTemplate(id) => i18n::text_with(
                "Manual destination '{id}' must contain exactly one '{placeholder}' argument",
                &[("{id}", id), ("{placeholder}", "{target}")],
            ),
            Self::InvalidPrivateTargetTemplate(id) => i18n::text_with(
                "Manual destination '{id}' private arguments must contain exactly one '{placeholder}' argument",
                &[("{id}", id), ("{placeholder}", "{target}")],
            ),
            Self::InvalidDesktopId(id) => i18n::text_with(
                "Discovered destination '{id}' must have a desktop application ID",
                &[("{id}", id)],
            ),
            Self::InvalidProfileIdentity(id) => i18n::text_with(
                "Profile destination '{id}' must persist a native profile identity",
                &[("{id}", id)],
            ),
            Self::Conflict => i18n::text(
                "Configuration was changed in another editor. Overwrite to keep this draft after creating a backup.",
            ),
            Self::NotRegularFile => i18n::text("Configuration must be a regular file"),
            Self::NotOwned => i18n::text("Configuration is not owned by the current user"),
            Self::BrokenLink => i18n::text("Configuration symlink is broken"),
            Self::Backup(ErrorKind::PermissionDenied) => {
                i18n::text("Configuration backup permission was denied")
            }
            Self::Backup(_) => i18n::text("Configuration backup could not be created"),
        }
    }
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ConfigFile {
    version: u32,
    destinations: Vec<DestinationFile>,
    fallback: FallbackFile,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    rules: Vec<RoutingRule>,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct DestinationFile {
    id: String,
    label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    profile_label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    icon: Option<String>,
    application: ApplicationFile,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "lowercase", deny_unknown_fields)]
enum ApplicationFile {
    Manual {
        #[serde(skip_serializing_if = "Option::is_none")]
        label: Option<String>,
        executable: String,
        args: Vec<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        private_args: Option<Vec<String>>,
    },
    Discovered {
        desktop_id: String,
    },
    #[serde(rename = "firefox-profile")]
    FirefoxProfile {
        desktop_id: String,
        name: String,
        path: String,
    },
    #[serde(rename = "chromium-profile")]
    ChromiumProfile {
        desktop_id: String,
        user_data_dir: String,
        profile_directory: String,
    },
}

#[derive(Deserialize, Serialize)]
#[serde(tag = "action", rename_all = "kebab-case", deny_unknown_fields)]
enum FallbackFile {
    Open {
        destination: String,
        #[serde(default)]
        mode: LaunchMode,
    },
    ShowPicker,
}

#[allow(dead_code)]
pub fn load() -> Result<Configuration, Error> {
    store::load_from(&config_path()?)
}

#[allow(dead_code)]
pub fn load_optional() -> Result<Option<Configuration>, Error> {
    match load() {
        Err(Error::Read(ErrorKind::NotFound)) => Ok(None),
        result => result.map(Some),
    }
}

pub fn default_path() -> Result<PathBuf, Error> {
    config_path()
}

pub(crate) fn files_match(left: &ConfigFile, right: &ConfigFile) -> bool {
    toml::to_string(left).ok() == toml::to_string(right).ok()
}

pub fn semantically_equal(left: &Configuration, right: &Configuration) -> bool {
    files_match(&to_file(left), &to_file(right))
}

pub fn assemble(
    destinations: Vec<BrowserDestination>,
    rules: Vec<RoutingRule>,
    fallback: FallbackAction,
) -> Result<Configuration, Error> {
    validate(to_file(&Configuration {
        destinations,
        rules,
        fallback,
    }))
}

pub(super) fn config_path() -> Result<PathBuf, Error> {
    if let Some(directory) = env::var_os("XDG_CONFIG_HOME") {
        let directory = PathBuf::from(directory);
        if !directory.is_absolute() {
            return Err(Error::ConfigHomeNotAbsolute);
        }
        return Ok(directory.join("browser-picker/config.toml"));
    }

    let home = env::var_os("HOME").ok_or(Error::HomeNotSet)?;
    Ok(Path::new(&home).join(".config/browser-picker/config.toml"))
}
pub fn inspect() -> Result<Inspected, Error> {
    inspect_path(&config_path()?)
}

pub fn inspect_path(path: &Path) -> Result<Inspected, Error> {
    Ok(ConfigurationStore::inspect_path(path)?.into_inspected())
}

pub fn migration_preview(from: u32) -> Option<MigrationPreview> {
    if from >= CURRENT_VERSION {
        return None;
    }
    if from == 0 {
        return Some(MigrationPreview {
            from,
            to: CURRENT_VERSION,
            changes: vec![i18n::text("Update schema version from 0 to 1")],
        });
    }
    None
}

pub(super) fn inspect_source(source: &str) -> Inspected {
    let document = match source.parse::<DocumentMut>() {
        Ok(document) => document,
        Err(_) => return Inspected::Invalid(Error::InvalidToml),
    };
    let version = match read_version(source, &document) {
        Ok(version) => version,
        Err(error) => return Inspected::Invalid(error),
    };
    if version > CURRENT_VERSION {
        return Inspected::Invalid(Error::UnsupportedVersion(version));
    }
    let file = match parse_file(source) {
        Ok(file) => file,
        Err(error) => return Inspected::Invalid(error),
    };
    match validate_body(file) {
        Ok(configuration) if version == CURRENT_VERSION => Inspected::Ready(configuration),
        Ok(configuration) => match migration_preview(version) {
            Some(preview) => Inspected::Migratable {
                configuration,
                preview,
            },
            None => Inspected::Invalid(Error::UnsupportedVersion(version)),
        },
        Err(error) => Inspected::Invalid(error),
    }
}

fn read_version(source: &str, document: &DocumentMut) -> Result<u32, Error> {
    match document.get("version") {
        None => Err(Error::MissingField("version".to_owned())),
        Some(item) => integer_version(item).ok_or_else(|| Error::InvalidType {
            key: "version".to_owned(),
            line: item.span().map(|span| diagnostic_line(source, span.start)),
        }),
    }
}

fn integer_version(item: &Item) -> Option<u32> {
    u32::try_from(item.as_integer()?).ok()
}

pub(super) fn parse_file(source: &str) -> Result<ConfigFile, Error> {
    toml::from_str(source).map_err(|error| diagnostic(source, error))
}

fn diagnostic(source: &str, error: toml::de::Error) -> Error {
    let message = error.message();
    let line = error.span().map(|span| diagnostic_line(source, span.start));
    if let Some(key) = quoted_token(message, "unknown field `") {
        return Error::UnknownKey { key, line };
    }
    if let Some(key) = quoted_token(message, "duplicate key `") {
        return Error::DuplicateField { key, line };
    }
    if let Some(key) = quoted_token(message, "missing field `") {
        return Error::MissingField(key);
    }
    if message.contains("invalid type:") {
        let key = quoted_token(message, "for key `")
            .or_else(|| path_key(message))
            .unwrap_or_default();
        return Error::InvalidType { key, line };
    }
    Error::InvalidToml
}

fn quoted_token(message: &str, prefix: &str) -> Option<String> {
    let rest = message.split_once(prefix)?.1;
    let key = rest.split('`').next()?.trim();
    (!key.is_empty()).then(|| key.to_owned())
}

fn path_key(message: &str) -> Option<String> {
    let rest = message.split_once("in `")?.1;
    let path = rest.split('`').next()?.trim();
    let key = path.rsplit(['.', ']']).find(|part| !part.is_empty())?;
    let key = key.trim_start_matches('[');
    (!key.is_empty()).then(|| key.to_owned())
}

fn diagnostic_line(source: &str, offset: usize) -> usize {
    source
        .get(..offset.min(source.len()))
        .unwrap_or(source)
        .bytes()
        .filter(|byte| *byte == b'\n')
        .count()
        + 1
}

pub(super) fn validate(file: ConfigFile) -> Result<Configuration, Error> {
    if file.version != CURRENT_VERSION {
        return Err(Error::UnsupportedVersion(file.version));
    }
    validate_body(file)
}

pub(super) fn validate_body(file: ConfigFile) -> Result<Configuration, Error> {
    let mut ids = HashSet::new();
    let mut labels = HashSet::new();
    let mut destinations = Vec::with_capacity(file.destinations.len());
    for destination in file.destinations {
        if !is_valid_id(&destination.id) {
            return Err(Error::InvalidId(destination.id));
        }
        if !ids.insert(destination.id.clone()) {
            return Err(Error::DuplicateId(destination.id));
        }
        if destination.label.trim().is_empty() {
            return Err(Error::InvalidLabel(destination.id));
        }
        if !labels.insert(destination.label.clone()) {
            return Err(Error::DuplicateLabel(destination.label));
        }
        destinations.push(validate_destination(destination)?);
    }

    let mut rule_ids = HashSet::new();
    let mut rule_names = HashSet::new();
    let mut rules = Vec::with_capacity(file.rules.len());
    for rule in file.rules {
        rules.push(validate_rule(
            rule,
            &destinations,
            &mut rule_ids,
            &mut rule_names,
        )?);
    }

    let fallback = match file.fallback {
        FallbackFile::Open { destination, mode } => {
            if !ids.contains(&destination) {
                return Err(Error::UnknownFallback(destination));
            }
            let destination_ref = destinations
                .iter()
                .find(|candidate| candidate.id == destination)
                .ok_or_else(|| Error::UnknownFallback(destination.clone()))?;
            if mode == LaunchMode::Private && !destination_ref.supports_private() {
                return Err(Error::UnsupportedPrivateMode(destination));
            }
            FallbackAction::Open { destination, mode }
        }
        FallbackFile::ShowPicker => FallbackAction::ShowPicker,
    };
    Ok(Configuration {
        destinations,
        rules,
        fallback,
    })
}

fn validate_rule(
    mut rule: RoutingRule,
    destinations: &[BrowserDestination],
    ids: &mut HashSet<String>,
    names: &mut HashSet<String>,
) -> Result<RoutingRule, Error> {
    if !is_valid_id(&rule.id) {
        return Err(Error::InvalidRuleId(rule.id));
    }
    if !ids.insert(rule.id.clone()) {
        return Err(Error::DuplicateRuleId(rule.id));
    }
    if rule.name.trim().is_empty() {
        return Err(Error::InvalidRuleName(rule.id));
    }
    if !names.insert(rule.name.clone()) {
        return Err(Error::DuplicateRuleName(rule.name));
    }
    if rule.groups.is_empty() {
        return Err(Error::EmptyRuleGroups(rule.id));
    }
    for group in &mut rule.groups {
        if group.conditions.is_empty() {
            return Err(Error::EmptyConditionGroup(rule.id));
        }
        for condition in &mut group.conditions {
            match condition {
                UrlCondition::Scheme { value, .. } => {
                    *value = value.to_ascii_lowercase();
                    if !matches!(value.as_str(), "http" | "https") {
                        return Err(Error::InvalidRuleScheme(rule.id));
                    }
                }
                UrlCondition::Host { value, .. } => {
                    let candidate = value.trim().trim_start_matches('[').trim_end_matches(']');
                    *value = if let Ok(address) = candidate.parse::<std::net::Ipv6Addr>() {
                        address.to_string()
                    } else if let Ok(address) = candidate.parse::<std::net::Ipv4Addr>() {
                        address.to_string()
                    } else {
                        let ascii = idna::domain_to_ascii(candidate)
                            .map_err(|_| Error::InvalidRuleHost(rule.id.clone()))?
                            .to_ascii_lowercase();
                        if ascii.is_empty() || url::Host::parse(&ascii).is_err() {
                            return Err(Error::InvalidRuleHost(rule.id));
                        }
                        ascii
                    };
                }
                UrlCondition::Glob { value, .. } => {
                    crate::url_pattern::validate_glob(value)
                        .map_err(|reason| Error::InvalidRuleGlob(rule.id.clone(), reason))?;
                }
                UrlCondition::Regex {
                    value,
                    case_insensitive,
                    ..
                } => {
                    crate::url_pattern::validate_regex(value, *case_insensitive)
                        .map_err(|reason| Error::InvalidRuleRegex(rule.id.clone(), reason))?;
                }
                _ => {}
            }
        }
    }
    let (destination_id, mode) = match &rule.action {
        RoutingAction::Open { destination, mode }
        | RoutingAction::Preselect { destination, mode } => (destination, mode),
    };
    let destination = destinations
        .iter()
        .find(|candidate| candidate.id == *destination_id)
        .ok_or_else(|| Error::UnknownDestination(destination_id.clone()))?;
    if *mode == LaunchMode::Private && !destination.supports_private() {
        return Err(Error::UnsupportedPrivateMode(destination_id.clone()));
    }
    Ok(rule)
}

fn validate_destination(destination: DestinationFile) -> Result<BrowserDestination, Error> {
    let DestinationFile {
        id,
        label,
        profile_label,
        icon,
        application,
    } = destination;
    match application {
        ApplicationFile::Manual {
            label: application_name,
            executable,
            args,
            private_args,
        } => {
            let path = Path::new(&executable);
            let bare_name = !executable.is_empty() && !executable.contains('/');
            if !path.is_absolute() && !bare_name {
                return Err(Error::InvalidExecutable(id));
            }
            validate_arguments(&args).map_err(|()| Error::InvalidTargetTemplate(id.clone()))?;
            if let Some(arguments) = &private_args {
                validate_arguments(arguments)
                    .map_err(|()| Error::InvalidPrivateTargetTemplate(id.clone()))?;
            }

            let application_label = application_name.unwrap_or_else(|| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or(&executable)
                    .to_owned()
            });
            let launch = DestinationLaunch::Manual {
                executable: executable.clone(),
                arguments: args,
                private_arguments: private_args,
            };
            Ok(BrowserDestination {
                unavailable_reason: manual_unavailability(&executable),
                id,
                label,
                application_label,
                profile_label,
                icon_name: icon,
                launch,
            })
        }
        ApplicationFile::Discovered { desktop_id } => {
            if desktop_id.trim().is_empty() {
                return Err(Error::InvalidDesktopId(id));
            }
            let application = discovery::application(&desktop_id);
            let application_label = application
                .as_ref()
                .map(|application| application.name.clone())
                .unwrap_or_else(|| desktop_id.clone());
            let unavailable_reason = if application.is_none() {
                Some(i18n::text("Browser Application is not installed"))
            } else {
                None
            };
            Ok(BrowserDestination {
                id,
                label,
                application_label,
                profile_label,
                icon_name: icon,
                launch: DestinationLaunch::Discovered { desktop_id },
                unavailable_reason,
            })
        }
        ApplicationFile::FirefoxProfile {
            desktop_id,
            name,
            path,
        } => {
            if desktop_id.trim().is_empty() {
                return Err(Error::InvalidDesktopId(id));
            }
            if name.trim().is_empty() || path.trim().is_empty() || !Path::new(&path).is_absolute() {
                return Err(Error::InvalidProfileIdentity(id));
            }
            let identity = ProfileIdentity::Firefox {
                name: name.clone(),
                path: PathBuf::from(&path),
            };
            Ok(profile_destination(
                id,
                label,
                profile_label,
                icon,
                desktop_id.clone(),
                DestinationLaunch::FirefoxProfile {
                    desktop_id,
                    name,
                    path,
                },
                &identity,
            ))
        }
        ApplicationFile::ChromiumProfile {
            desktop_id,
            user_data_dir,
            profile_directory,
        } => {
            if desktop_id.trim().is_empty() {
                return Err(Error::InvalidDesktopId(id));
            }
            if user_data_dir.trim().is_empty()
                || profile_directory.trim().is_empty()
                || !Path::new(&user_data_dir).is_absolute()
            {
                return Err(Error::InvalidProfileIdentity(id));
            }
            let identity = ProfileIdentity::Chromium {
                user_data_dir: PathBuf::from(&user_data_dir),
                profile_directory: profile_directory.clone(),
            };
            Ok(profile_destination(
                id,
                label,
                profile_label,
                icon,
                desktop_id.clone(),
                DestinationLaunch::ChromiumProfile {
                    desktop_id,
                    user_data_dir,
                    profile_directory,
                },
                &identity,
            ))
        }
    }
}

fn profile_destination(
    id: String,
    label: String,
    profile_label: Option<String>,
    icon: Option<String>,
    desktop_id: String,
    launch: DestinationLaunch,
    identity: &ProfileIdentity,
) -> BrowserDestination {
    let application_label = discovery::application(&desktop_id)
        .map(|application| application.name)
        .unwrap_or_else(|| desktop_id.clone());
    BrowserDestination {
        id,
        label,
        application_label,
        profile_label,
        icon_name: icon,
        launch,
        unavailable_reason: profile_unavailability(&desktop_id, identity),
    }
}

fn profile_unavailability(desktop_id: &str, identity: &ProfileIdentity) -> Option<String> {
    let executable = discovery::application(desktop_id).map(|application| application.executable);
    match profiles::capability(
        desktop_id,
        executable.as_deref(),
        identity,
        &profiles::DiscoveryPaths::from_env(),
    ) {
        profiles::ProfileCapability::Verified => None,
        profiles::ProfileCapability::MissingApplication => {
            Some(i18n::text("Browser Application is not installed"))
        }
        profiles::ProfileCapability::MissingProfile => {
            Some(i18n::text("Browser Profile is not available"))
        }
        profiles::ProfileCapability::UnsupportedPackaging => Some(i18n::text(
            "Unknown packaging or unsupported capability. This remains a generic Browser Application rather than a verified profile destination.",
        )),
    }
}

fn validate_arguments(arguments: &[String]) -> Result<(), ()> {
    let placeholders = arguments
        .iter()
        .filter(|argument| argument.as_str() == "{target}")
        .count();
    if placeholders != 1
        || arguments
            .iter()
            .any(|argument| argument.contains("{target}") && argument.as_str() != "{target}")
    {
        return Err(());
    }
    Ok(())
}

fn is_valid_id(id: &str) -> bool {
    !id.is_empty()
        && id
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

fn manual_unavailability(executable: &str) -> Option<String> {
    if executable_is_available(executable) {
        None
    } else {
        Some(i18n::text("Browser Application is not installed"))
    }
}

fn executable_is_available(executable: &str) -> bool {
    let path = Path::new(executable);
    if path.is_absolute() {
        path.is_file()
    } else if !executable.contains('/') {
        env::split_paths(&env::var_os("PATH").unwrap_or_default())
            .any(|directory| directory.join(executable).is_file())
    } else {
        false
    }
}

pub(super) fn to_file(configuration: &Configuration) -> ConfigFile {
    ConfigFile {
        version: CURRENT_VERSION,
        destinations: configuration
            .destinations
            .iter()
            .map(|destination| DestinationFile {
                id: destination.id.clone(),
                label: destination.label.clone(),
                profile_label: destination.profile_label.clone(),
                icon: destination.icon_name.clone(),
                application: match &destination.launch {
                    DestinationLaunch::Manual {
                        executable,
                        arguments,
                        private_arguments,
                    } => ApplicationFile::Manual {
                        label: Some(destination.application_label.clone()),
                        executable: executable.clone(),
                        args: arguments.clone(),
                        private_args: private_arguments.clone(),
                    },
                    DestinationLaunch::Discovered { desktop_id } => ApplicationFile::Discovered {
                        desktop_id: desktop_id.clone(),
                    },
                    DestinationLaunch::FirefoxProfile {
                        desktop_id,
                        name,
                        path,
                    } => ApplicationFile::FirefoxProfile {
                        desktop_id: desktop_id.clone(),
                        name: name.clone(),
                        path: path.clone(),
                    },
                    DestinationLaunch::ChromiumProfile {
                        desktop_id,
                        user_data_dir,
                        profile_directory,
                    } => ApplicationFile::ChromiumProfile {
                        desktop_id: desktop_id.clone(),
                        user_data_dir: user_data_dir.clone(),
                        profile_directory: profile_directory.clone(),
                    },
                },
            })
            .collect(),
        rules: configuration.rules.clone(),
        fallback: match &configuration.fallback {
            FallbackAction::Open { destination, mode } => FallbackFile::Open {
                destination: destination.clone(),
                mode: *mode,
            },
            FallbackAction::ShowPicker => FallbackFile::ShowPicker,
        },
    }
}
