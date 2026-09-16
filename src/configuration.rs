use std::collections::HashSet;
use std::env;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use serde::Deserialize;

#[derive(Debug)]
pub struct LaunchDefinition {
    pub destination_id: String,
    pub executable: String,
    pub arguments: Vec<String>,
}

pub enum Error {
    ConfigHomeNotAbsolute,
    HomeNotSet,
    Read(ErrorKind),
    InvalidToml,
    UnsupportedVersion(u32),
    InvalidId(String),
    DuplicateId(String),
    UnknownFallback(String),
    InvalidExecutable(String),
    InvalidTargetTemplate(String),
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
    #[serde(rename = "label")]
    _label: String,
    application: ApplicationFile,
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "lowercase", deny_unknown_fields)]
enum ApplicationFile {
    Manual {
        executable: String,
        args: Vec<String>,
    },
}

#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "lowercase", deny_unknown_fields)]
enum FallbackFile {
    Open { destination: String },
}

pub fn load_fallback() -> Result<LaunchDefinition, Error> {
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

fn validate(file: ConfigFile) -> Result<LaunchDefinition, Error> {
    if file.version != 1 {
        return Err(Error::UnsupportedVersion(file.version));
    }

    let mut ids = HashSet::new();
    for destination in &file.destinations {
        if destination.id.is_empty()
            || !destination
                .id
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        {
            return Err(Error::InvalidId(destination.id.clone()));
        }
        if !ids.insert(destination.id.as_str()) {
            return Err(Error::DuplicateId(destination.id.clone()));
        }
    }

    let fallback_id = match file.fallback {
        FallbackFile::Open { destination } => destination,
    };
    let mut fallback = None;
    for destination in file.destinations {
        let definition = validate_destination(destination)?;
        if definition.destination_id == fallback_id {
            fallback = Some(definition);
        }
    }
    fallback.ok_or(Error::UnknownFallback(fallback_id))
}

fn validate_destination(destination: DestinationFile) -> Result<LaunchDefinition, Error> {
    match destination.application {
        ApplicationFile::Manual { executable, args } => {
            let path = Path::new(&executable);
            let bare_name = !executable.is_empty() && !executable.contains('/');
            if !path.is_absolute() && !bare_name {
                return Err(Error::InvalidExecutable(destination.id));
            }

            let placeholders = args
                .iter()
                .filter(|argument| argument.as_str() == "{target}")
                .count();
            if placeholders != 1
                || args.iter().any(|argument| {
                    argument.contains("{target}") && argument.as_str() != "{target}"
                })
            {
                return Err(Error::InvalidTargetTemplate(destination.id));
            }

            Ok(LaunchDefinition {
                destination_id: destination.id,
                executable,
                arguments: args,
            })
        }
    }
}
