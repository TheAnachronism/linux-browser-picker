use std::collections::HashSet;
use std::env;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use gtk::gio::prelude::AppInfoExt;
use serde::{Deserialize, Serialize};

use crate::discovery;
use crate::i18n;

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
}

#[derive(Clone, Debug)]
pub enum FallbackAction {
    Open(String),
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

pub enum Error {
    ConfigHomeNotAbsolute,
    HomeNotSet,
    Read(ErrorKind),
    Save(ErrorKind),
    InvalidToml,
    UnsupportedVersion(u32),
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
    UnknownDestination(String),
    UnsupportedPrivateMode(String),
    UnknownFallback(String),
    InvalidExecutable(String),
    InvalidTargetTemplate(String),
    InvalidPrivateTargetTemplate(String),
    InvalidDesktopId(String),
}

impl BrowserDestination {
    pub fn supports_private(&self) -> bool {
        matches!(
            self.launch,
            DestinationLaunch::Manual {
                private_arguments: Some(_),
                ..
            }
        )
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
            Self::UnsupportedVersion(version) => i18n::text_with(
                "Unsupported configuration version {version}; expected version 1",
                &[("{version}", &version.to_string())],
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
        }
    }
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ConfigFile {
    version: u32,
    destinations: Vec<DestinationFile>,
    fallback: FallbackFile,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    rules: Vec<RoutingRule>,
}

#[derive(Deserialize, Serialize)]
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

#[derive(Deserialize, Serialize)]
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
}

#[derive(Deserialize, Serialize)]
#[serde(tag = "action", rename_all = "kebab-case", deny_unknown_fields)]
enum FallbackFile {
    Open { destination: String },
    ShowPicker,
}

pub fn load() -> Result<Configuration, Error> {
    let source =
        std::fs::read_to_string(config_path()?).map_err(|error| Error::Read(error.kind()))?;
    let file: ConfigFile = toml::from_str(&source).map_err(|_| Error::InvalidToml)?;
    validate(file)
}

pub fn load_optional() -> Result<Option<Configuration>, Error> {
    match load() {
        Err(Error::Read(ErrorKind::NotFound)) => Ok(None),
        result => result.map(Some),
    }
}

pub fn save(configuration: &Configuration) -> Result<(), Error> {
    let path = config_path()?;
    let directory = path
        .parent()
        .expect("configuration path should have a parent");
    std::fs::create_dir_all(directory).map_err(|error| Error::Save(error.kind()))?;
    let serialized =
        toml::to_string_pretty(&to_file(configuration)).map_err(|_| Error::InvalidToml)?;
    let temporary = directory.join("config.toml.tmp");
    std::fs::write(&temporary, serialized).map_err(|error| Error::Save(error.kind()))?;
    std::fs::rename(temporary, path).map_err(|error| Error::Save(error.kind()))?;
    Ok(())
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

fn config_path() -> Result<PathBuf, Error> {
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

fn validate(file: ConfigFile) -> Result<Configuration, Error> {
    if file.version != 1 {
        return Err(Error::UnsupportedVersion(file.version));
    }

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
        FallbackFile::Open { destination } => {
            if !ids.contains(&destination) {
                return Err(Error::UnknownFallback(destination));
            }
            FallbackAction::Open(destination)
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
                    let ascii = idna::domain_to_ascii(value)
                        .map_err(|_| Error::InvalidRuleHost(rule.id.clone()))?
                        .to_ascii_lowercase();
                    if ascii.is_empty() || url::Host::parse(&ascii).is_err() {
                        return Err(Error::InvalidRuleHost(rule.id));
                    }
                    *value = ascii;
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
    match destination.application {
        ApplicationFile::Manual {
            label,
            executable,
            args,
            private_args,
        } => {
            let path = Path::new(&executable);
            let bare_name = !executable.is_empty() && !executable.contains('/');
            if !path.is_absolute() && !bare_name {
                return Err(Error::InvalidExecutable(destination.id));
            }
            validate_arguments(&args)
                .map_err(|()| Error::InvalidTargetTemplate(destination.id.clone()))?;
            if let Some(arguments) = &private_args {
                validate_arguments(arguments)
                    .map_err(|()| Error::InvalidPrivateTargetTemplate(destination.id.clone()))?;
            }

            let application_label = label.unwrap_or_else(|| {
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
                id: destination.id,
                label: destination.label,
                application_label,
                profile_label: destination.profile_label,
                icon_name: destination.icon,
                launch,
            })
        }
        ApplicationFile::Discovered { desktop_id } => {
            if desktop_id.trim().is_empty() {
                return Err(Error::InvalidDesktopId(destination.id));
            }
            let application_label = discovery::app_info(&desktop_id)
                .map(|application| application.display_name().to_string())
                .unwrap_or_else(|| desktop_id.clone());
            let unavailable_reason = if discovery::app_info(&desktop_id).is_none() {
                Some(i18n::text("Browser Application is not installed"))
            } else {
                None
            };
            Ok(BrowserDestination {
                id: destination.id,
                label: destination.label,
                application_label,
                profile_label: destination.profile_label,
                icon_name: destination.icon,
                launch: DestinationLaunch::Discovered { desktop_id },
                unavailable_reason,
            })
        }
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

fn to_file(configuration: &Configuration) -> ConfigFile {
    ConfigFile {
        version: 1,
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
                },
            })
            .collect(),
        rules: configuration.rules.clone(),
        fallback: match &configuration.fallback {
            FallbackAction::Open(destination) => FallbackFile::Open {
                destination: destination.clone(),
            },
            FallbackAction::ShowPicker => FallbackFile::ShowPicker,
        },
    }
}
