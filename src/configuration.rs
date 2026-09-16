use std::collections::HashSet;
use std::env;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use serde::Deserialize;

#[derive(Clone, Debug)]
pub struct BrowserDestination {
    pub id: String,
    pub label: String,
    pub application_label: String,
    pub profile_label: Option<String>,
    pub executable: String,
    pub arguments: Vec<String>,
    pub private_arguments: Option<Vec<String>>,
}

#[derive(Debug)]
pub enum FallbackAction {
    Open(String),
    ShowPicker,
}

#[derive(Debug)]
pub struct Configuration {
    pub destinations: Vec<BrowserDestination>,
    pub fallback: FallbackAction,
}

pub enum Error {
    ConfigHomeNotAbsolute,
    HomeNotSet,
    Read(ErrorKind),
    InvalidToml,
    UnsupportedVersion(u32),
    InvalidId(String),
    DuplicateId(String),
    InvalidLabel(String),
    DuplicateLabel(String),
    UnknownFallback(String),
    InvalidExecutable(String),
    InvalidTargetTemplate(String),
    InvalidPrivateTargetTemplate(String),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ConfigFile {
    version: u32,
    destinations: Vec<DestinationFile>,
    fallback: FallbackFile,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DestinationFile {
    id: String,
    label: String,
    profile_label: Option<String>,
    application: ApplicationFile,
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "lowercase", deny_unknown_fields)]
enum ApplicationFile {
    Manual {
        label: Option<String>,
        executable: String,
        args: Vec<String>,
        private_args: Option<Vec<String>>,
    },
}

#[derive(Deserialize)]
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
        if destination.id.is_empty()
            || !destination
                .id
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        {
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
        fallback,
    })
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
            Ok(BrowserDestination {
                id: destination.id,
                label: destination.label,
                application_label,
                profile_label: destination.profile_label,
                executable,
                arguments: args,
                private_arguments: private_args,
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
