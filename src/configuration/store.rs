use std::collections::HashSet;
use std::fs::{self, File, OpenOptions};
use std::io::{ErrorKind, Write};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use toml_edit::{ArrayOfTables, DocumentMut, Item, Table};

use super::{
    ConfigFile, Configuration, Error, Inspected, MigrationPreview, inspect_source, to_file,
    validate,
};
use crate::i18n;

const MAX_BACKUPS: usize = 5;
const OWNER_READ_WRITE: u32 = 0o600;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SaveConflictPolicy {
    Abort,
    Overwrite,
}

#[derive(Clone, Debug)]
pub enum StoreStatus {
    Missing,
    Current,
    Invalid(Error),
    Migratable(MigrationPreview),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SaveResult {
    pub backup_path: Option<PathBuf>,
    pub warnings: Vec<String>,
}

pub struct ConfigurationStore {
    path: PathBuf,
    original_bytes: Option<Vec<u8>>,
    document: DocumentMut,
    pub configuration: Option<Configuration>,
    pub warnings: Vec<String>,
    pub status: StoreStatus,
}

impl ConfigurationStore {
    pub fn inspect_path(path: &Path) -> Result<Self, Error> {
        match fs::symlink_metadata(path) {
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(Self::empty(path)),
            Err(error) => Err(Error::Read(error.kind())),
            Ok(_) => {
                let bytes =
                    fs::read(resolve_path(path)?).map_err(|error| Error::Read(error.kind()))?;
                Self::from_bytes(path, bytes)
            }
        }
    }

    #[allow(dead_code)]
    pub fn open_path(path: &Path) -> Result<Self, Error> {
        let store = Self::inspect_path(path)?;
        match &store.status {
            StoreStatus::Current => Ok(store),
            StoreStatus::Missing => Err(Error::Read(ErrorKind::NotFound)),
            StoreStatus::Invalid(error) => Err(error.clone()),
            StoreStatus::Migratable(preview) => Err(Error::MigrationRequired {
                from: preview.from,
                to: preview.to,
            }),
        }
    }

    #[allow(dead_code)]
    pub fn create_path(path: &Path) -> Result<Self, Error> {
        Self::inspect_path(path)
    }

    pub fn into_inspected(self) -> Inspected {
        match self.status {
            StoreStatus::Missing => Inspected::Missing,
            StoreStatus::Current => self
                .configuration
                .map(Inspected::Ready)
                .unwrap_or(Inspected::Invalid(Error::InvalidToml)),
            StoreStatus::Invalid(error) => Inspected::Invalid(error),
            StoreStatus::Migratable(preview) => match self.configuration {
                Some(configuration) => Inspected::Migratable {
                    configuration,
                    preview,
                },
                None => Inspected::Invalid(Error::InvalidToml),
            },
        }
    }

    fn empty(path: &Path) -> Self {
        Self {
            path: path.to_path_buf(),
            original_bytes: None,
            document: DocumentMut::new(),
            configuration: None,
            warnings: Vec::new(),
            status: StoreStatus::Missing,
        }
    }

    fn from_bytes(path: &Path, bytes: Vec<u8>) -> Result<Self, Error> {
        let target = resolve_path(path)?;
        let mut warnings = Vec::new();
        if let Some(warning) = mode_warning(&target) {
            warnings.push(warning);
        }
        let Ok(source) = String::from_utf8(bytes.clone()) else {
            return Ok(Self {
                path: path.to_path_buf(),
                original_bytes: Some(bytes),
                document: DocumentMut::new(),
                configuration: None,
                warnings,
                status: StoreStatus::Invalid(Error::InvalidToml),
            });
        };
        let document = source
            .parse::<DocumentMut>()
            .unwrap_or_else(|_| DocumentMut::new());
        let (configuration, status) = match inspect_source(&source) {
            Inspected::Ready(configuration) => (Some(configuration), StoreStatus::Current),
            Inspected::Migratable {
                configuration,
                preview,
            } => (Some(configuration), StoreStatus::Migratable(preview)),
            Inspected::Invalid(error) => (None, StoreStatus::Invalid(error)),
            Inspected::Missing => (None, StoreStatus::Missing),
        };
        Ok(Self {
            path: path.to_path_buf(),
            original_bytes: Some(bytes),
            document,
            configuration,
            warnings,
            status,
        })
    }

    pub fn save(
        &mut self,
        configuration: &Configuration,
        policy: SaveConflictPolicy,
    ) -> Result<SaveResult, Error> {
        self.persist(configuration, policy, false)
    }

    pub fn migrate(&mut self) -> Result<SaveResult, Error> {
        let configuration = self.configuration.clone().ok_or(Error::InvalidToml)?;
        if !matches!(self.status, StoreStatus::Migratable(_)) {
            return Err(Error::InvalidToml);
        }
        self.persist(&configuration, SaveConflictPolicy::Abort, true)
    }

    fn persist(
        &mut self,
        configuration: &Configuration,
        policy: SaveConflictPolicy,
        backup_first: bool,
    ) -> Result<SaveResult, Error> {
        let current = read_optional(&self.path)?;
        if current.as_deref() != self.original_bytes.as_deref()
            && policy == SaveConflictPolicy::Abort
        {
            return Err(Error::Conflict);
        }

        let target = resolve_path(&self.path)?;
        let mut warnings = Vec::new();
        if target.exists()
            && let Some(warning) = mode_warning(&target)
        {
            warnings.push(warning);
        }

        let rendered = self.render(configuration)?;
        let parsed: ConfigFile = toml::from_str(&rendered).map_err(|_| Error::InvalidToml)?;
        validate(parsed)?;

        let needs_backup = backup_first
            || matches!(self.status, StoreStatus::Migratable(_))
            || (policy == SaveConflictPolicy::Overwrite
                && current.as_deref() != self.original_bytes.as_deref());
        let backup_path = if needs_backup {
            if !target.exists() {
                return Err(Error::Conflict);
            }
            Some(create_backup(&target)?)
        } else {
            None
        };

        atomic_replace(&target, rendered.as_bytes())?;
        prune_backups(&target)?;

        self.original_bytes = Some(rendered.into_bytes());
        self.document = String::from_utf8(self.original_bytes.clone().unwrap())
            .expect("saved configuration is UTF-8")
            .parse()
            .expect("saved configuration is TOML");
        self.configuration = Some(configuration.clone());
        self.status = StoreStatus::Current;
        self.warnings = warnings.clone();
        Ok(SaveResult {
            backup_path,
            warnings,
        })
    }

    fn render(&self, configuration: &Configuration) -> Result<String, Error> {
        let file = to_file(configuration);
        if let Some(original) = &self.original_bytes {
            let original_text = String::from_utf8_lossy(original);
            if let Some(existing) = &self.configuration
                && matches!(self.status, StoreStatus::Current)
                && files_match(&to_file(existing), &file)
            {
                return Ok(original_text.into_owned());
            }
            let canonical = toml::to_string_pretty(&file).map_err(|_| Error::InvalidToml)?;
            let mut document = canonical
                .parse::<DocumentMut>()
                .map_err(|_| Error::InvalidToml)?;
            restore_table(document.as_table_mut(), self.document.as_table());
            Ok(preserve_leading_comments(
                &original_text,
                document.to_string(),
            ))
        } else {
            toml::to_string_pretty(&file).map_err(|_| Error::InvalidToml)
        }
    }
}

#[allow(dead_code)]
pub fn load_from(path: &Path) -> Result<Configuration, Error> {
    ConfigurationStore::open_path(path)?
        .configuration
        .ok_or(Error::InvalidToml)
}

fn files_match(left: &ConfigFile, right: &ConfigFile) -> bool {
    super::files_match(left, right)
}

fn preserve_leading_comments(original: &str, rendered: String) -> String {
    let mut leading = String::new();
    for line in original.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('#') || trimmed.is_empty() {
            leading.push_str(line);
            leading.push('\n');
        } else {
            break;
        }
    }
    if leading.trim().is_empty() || rendered.starts_with(leading.trim_start()) {
        rendered
    } else {
        format!("{leading}{rendered}")
    }
}

fn restore_table(new: &mut Table, old: &Table) {
    *new.decor_mut() = old.decor().clone();
    let keys: Vec<String> = new.iter().map(|(key, _)| key.to_owned()).collect();
    for key in keys {
        let Some(old_item) = old.get(&key) else {
            continue;
        };
        if let Some(new_item) = new.get_mut(&key) {
            restore_item(new_item, old_item);
        }
    }
}

fn restore_item(new: &mut Item, old: &Item) {
    if new.as_table().is_some() && old.as_table().is_some() {
        restore_table(
            new.as_table_mut().expect("table"),
            old.as_table().expect("table"),
        );
        return;
    }
    if new.as_array_of_tables().is_some() && old.as_array_of_tables().is_some() {
        restore_array_of_tables(
            new.as_array_of_tables_mut().expect("array of tables"),
            old.as_array_of_tables().expect("array of tables"),
        );
        return;
    }
    if let (Some(new_value), Some(old_value)) = (toml_item_value(new), toml_item_value(old))
        && new_value == old_value
    {
        *new = old.clone();
    }
}

fn restore_array_of_tables(new: &mut ArrayOfTables, old: &ArrayOfTables) {
    let mut unused: Vec<Table> = old.iter().cloned().collect();
    for table in new.iter_mut() {
        let id = table.get("id").and_then(Item::as_str);
        let matched = if let Some(id) = id {
            unused
                .iter()
                .position(|candidate| candidate.get("id").and_then(Item::as_str) == Some(id))
        } else if unused.is_empty() {
            None
        } else {
            Some(0)
        };
        if let Some(index) = matched {
            let old_table = unused.remove(index);
            restore_table(table, &old_table);
        }
    }
}

fn toml_item_value(item: &Item) -> Option<toml::Value> {
    let text = item.to_string();
    toml::from_str(&format!("wrapper = {text}"))
        .ok()
        .or_else(|| toml::from_str(&text).ok())
}

fn resolve_path(path: &Path) -> Result<PathBuf, Error> {
    let mut current = path.to_path_buf();
    let mut seen = HashSet::new();
    loop {
        if !seen.insert(current.clone()) {
            return Err(Error::NotRegularFile);
        }
        match fs::symlink_metadata(&current) {
            Err(error) if error.kind() == ErrorKind::NotFound => {
                if let Some(parent) = current.parent() {
                    fs::create_dir_all(parent).map_err(|error| Error::Save(error.kind()))?;
                }
                return Ok(current);
            }
            Err(error) => return Err(Error::Read(error.kind())),
            Ok(metadata) if metadata.file_type().is_symlink() => {
                let next = fs::read_link(&current).map_err(|error| Error::Read(error.kind()))?;
                current = if next.is_absolute() {
                    next
                } else {
                    current
                        .parent()
                        .unwrap_or_else(|| Path::new("/"))
                        .join(next)
                };
            }
            Ok(metadata) if metadata.file_type().is_file() => {
                if metadata.uid() != current_uid() {
                    return Err(Error::NotOwned);
                }
                return Ok(current);
            }
            Ok(_) => return Err(Error::NotRegularFile),
        }
    }
}

fn current_uid() -> u32 {
    fs::metadata("/proc/self")
        .map(|metadata| metadata.uid())
        .unwrap_or(0)
}

fn mode_warning(path: &Path) -> Option<String> {
    let mode = fs::metadata(path).ok()?.permissions().mode() & 0o777;
    (mode & 0o077 != 0).then(|| {
        i18n::text("Configuration is readable by group or others. Exact query values are stored as plain text.")
    })
}

fn read_optional(path: &Path) -> Result<Option<Vec<u8>>, Error> {
    match fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
        Err(error) => Err(Error::Read(error.kind())),
    }
}

fn create_backup(target: &Path) -> Result<PathBuf, Error> {
    let bytes = fs::read(target).map_err(|error| Error::Backup(error.kind()))?;
    let path = unique_backup_path(target);
    write_owner_only(&path, &bytes).map_err(Error::Backup)?;
    Ok(path)
}

fn unique_backup_path(target: &Path) -> PathBuf {
    let directory = target.parent().unwrap_or_else(|| Path::new("."));
    let name = target.file_name().unwrap_or_default().to_string_lossy();
    let stamp = utc_stamp();
    let mut path = directory.join(format!("{name}.bak-{stamp}"));
    let mut extra = 2;
    while path.exists() {
        path = directory.join(format!("{name}.bak-{stamp}-{extra}"));
        extra += 1;
    }
    path
}

fn utc_stamp() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let days = i32::try_from(secs / 86400).unwrap_or(0);
    let rem = secs % 86400;
    let (year, month, day) = civil_from_unix_days(days);
    format!(
        "{year:04}{month:02}{day:02}T{:02}{:02}{:02}Z",
        rem / 3600,
        (rem % 3600) / 60,
        rem % 60
    )
}

fn civil_from_unix_days(days: i32) -> (i32, u32, u32) {
    let z = i64::from(days) + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let year = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if month <= 2 { year + 1 } else { year };
    (year as i32, month as u32, day as u32)
}

fn prune_backups(target: &Path) -> Result<(), Error> {
    let directory = target.parent().unwrap_or_else(|| Path::new("."));
    let name = target.file_name().unwrap_or_default().to_string_lossy();
    let prefix = format!("{name}.bak-");
    let mut backups = Vec::new();
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(Error::Save(error.kind())),
    };
    for entry in entries {
        let entry = entry.map_err(|error| Error::Save(error.kind()))?;
        let file_name = entry.file_name();
        let file_name = file_name.to_string_lossy();
        if is_picker_backup(&prefix, &file_name) {
            backups.push(entry.path());
        }
    }
    backups.sort();
    let extra = backups.len().saturating_sub(MAX_BACKUPS);
    for path in backups.into_iter().take(extra) {
        fs::remove_file(path).map_err(|error| Error::Save(error.kind()))?;
    }
    Ok(())
}

fn is_picker_backup(prefix: &str, name: &str) -> bool {
    let Some(suffix) = name.strip_prefix(prefix) else {
        return false;
    };
    let (stamp, rest) = suffix
        .split_once('-')
        .map_or((suffix, None), |(stamp, rest)| (stamp, Some(rest)));
    stamp.len() == 16
        && stamp.as_bytes()[8] == b'T'
        && stamp.ends_with('Z')
        && stamp
            .bytes()
            .enumerate()
            .all(|(index, byte)| matches!(index, 8 | 15) || byte.is_ascii_digit())
        && rest.is_none_or(|value| {
            !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit())
        })
}

fn atomic_replace(target: &Path, contents: &[u8]) -> Result<(), Error> {
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(|error| Error::Save(error.kind()))?;
    }
    let temporary = temp_path(target);
    if temporary.exists() {
        fs::remove_file(&temporary).map_err(|error| Error::Save(error.kind()))?;
    }
    let mode = existing_mode(target).unwrap_or(OWNER_READ_WRITE);
    write_with_mode(&temporary, contents, mode).map_err(Error::Save)?;
    let file = File::open(&temporary).map_err(|error| Error::Save(error.kind()))?;
    file.sync_all().map_err(|error| Error::Save(error.kind()))?;
    drop(file);
    fs::rename(&temporary, target).map_err(|error| {
        let _ = fs::remove_file(&temporary);
        Error::Save(error.kind())
    })?;
    if let Some(parent) = target.parent()
        && let Ok(directory) = File::open(parent)
    {
        let _ = directory.sync_all();
    }
    Ok(())
}

fn temp_path(target: &Path) -> PathBuf {
    let mut name = std::ffi::OsString::from(".");
    name.push(target.file_name().unwrap_or_default());
    name.push(".tmp");
    target.parent().unwrap_or_else(|| Path::new(".")).join(name)
}

fn existing_mode(path: &Path) -> Option<u32> {
    fs::metadata(path)
        .ok()
        .map(|metadata| metadata.permissions().mode() & 0o777)
}

fn write_owner_only(path: &Path, contents: &[u8]) -> Result<(), ErrorKind> {
    write_with_mode(path, contents, OWNER_READ_WRITE)
}

fn write_with_mode(path: &Path, contents: &[u8], mode: u32) -> Result<(), ErrorKind> {
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(mode)
        .open(path)
        .map_err(|error| error.kind())?;
    file.write_all(contents).map_err(|error| error.kind())?;
    file.sync_all().map_err(|error| error.kind())?;
    drop(file);
    fs::set_permissions(path, fs::Permissions::from_mode(mode)).map_err(|error| error.kind())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::configuration::{LaunchMode, RoutingAction, UrlCondition};
    use std::os::unix::fs as unix_fs;
    use tempfile::TempDir;

    fn sample(label: &str) -> String {
        format!(
            "# keep this header\nversion = 1 # schema\n\n[[destinations]]\nid = \"work\"\nlabel = \"{label}\"\n\n[destinations.application]\ntype = \"manual\"\nexecutable = \"/bin/true\"\nargs = [\"{{target}}\"]\n\n[fallback]\naction = \"show-picker\"\n"
        )
    }

    fn write_sample(dir: &TempDir, label: &str) -> PathBuf {
        let path = dir.path().join("config.toml");
        fs::write(&path, sample(label)).expect("sample should write");
        path
    }

    fn load_config(path: &Path) -> Configuration {
        ConfigurationStore::open_path(path)
            .expect("sample should load")
            .configuration
            .expect("sample should contain configuration")
    }

    fn relabel(mut configuration: Configuration, label: &str) -> Configuration {
        configuration.destinations[0].label = label.to_owned();
        configuration
    }

    #[test]
    fn complete_schema_round_trips_destinations_rules_actions_and_fallback() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("config.toml");
        fs::write(
            &path,
            r#"version = 1

[[destinations]]
id = "work"
label = "Work"
profile_label = "Office"
icon = "browser"

[destinations.application]
type = "manual"
label = "True"
executable = "/bin/true"
args = ["{target}"]
private_args = ["--private", "{target}"]

[[rules]]
id = "docs"
name = "Docs"
enabled = true

[[rules.groups]]
[[rules.groups.conditions]]
type = "query-value"
key = "q"
value = "secret"

[rules.action]
type = "open"
destination = "work"
mode = "private"

[fallback]
action = "open"
destination = "work"
mode = "private"
"#,
        )
        .unwrap();

        let configuration = load_config(&path);
        assert_eq!(configuration.destinations[0].id, "work");
        assert_eq!(
            configuration.destinations[0].profile_label.as_deref(),
            Some("Office")
        );
        assert!(matches!(
            configuration.rules[0].action,
            RoutingAction::Open {
                mode: LaunchMode::Private,
                ..
            }
        ));
        assert!(matches!(
            configuration.fallback,
            super::super::FallbackAction::Open {
                mode: LaunchMode::Private,
                ..
            }
        ));
        assert!(matches!(
            configuration.rules[0].groups[0].conditions[0],
            UrlCondition::QueryValue { .. }
        ));
    }

    #[test]
    fn save_preserves_comments_and_unaffected_formatting() {
        let dir = TempDir::new().unwrap();
        let path = write_sample(&dir, "Work");
        let mut store = ConfigurationStore::open_path(&path).unwrap();
        let updated = relabel(store.configuration.clone().unwrap(), "Office");
        store.save(&updated, SaveConflictPolicy::Abort).unwrap();
        let saved = fs::read_to_string(&path).unwrap();
        assert!(saved.contains("# keep this header"));
        assert!(saved.contains("version = 1 # schema"));
        assert!(saved.contains("label = \"Office\""));
        assert!(!saved.contains("label = \"Work\""));
    }

    #[test]
    fn unchanged_save_keeps_original_bytes() {
        let dir = TempDir::new().unwrap();
        let path = write_sample(&dir, "Work");
        let original = fs::read(&path).unwrap();
        let mut store = ConfigurationStore::open_path(&path).unwrap();
        let configuration = store.configuration.clone().unwrap();
        store
            .save(&configuration, SaveConflictPolicy::Abort)
            .unwrap();
        assert_eq!(fs::read(&path).unwrap(), original);
    }

    #[test]
    fn changed_destination_uses_deterministic_key_order() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("config.toml");
        fs::write(
            &path,
            "# header\nversion = 1\n\n[[destinations]]\nlabel = \"Work\"\nid = \"work\"\n\n[destinations.application]\nargs = [\"{target}\"]\ntype = \"manual\"\nexecutable = \"/bin/true\"\n\n[fallback]\naction = \"show-picker\"\n",
        )
        .unwrap();
        let mut store = ConfigurationStore::open_path(&path).unwrap();
        let updated = relabel(store.configuration.clone().unwrap(), "Office");
        store.save(&updated, SaveConflictPolicy::Abort).unwrap();
        let saved = fs::read_to_string(&path).unwrap();
        let destination = saved
            .split("[[destinations]]")
            .nth(1)
            .unwrap()
            .split("[fallback]")
            .next()
            .unwrap();
        assert!(
            destination.find("id =").unwrap() < destination.find("label =").unwrap(),
            "{destination}"
        );
        assert!(saved.contains("# header"));
    }

    #[test]
    fn rename_save_replacement_is_a_conflict_until_overwrite() {
        let dir = TempDir::new().unwrap();
        let path = write_sample(&dir, "Work");
        let mut store = ConfigurationStore::open_path(&path).unwrap();
        let updated = relabel(store.configuration.clone().unwrap(), "Office");
        let replacement = dir.path().join("external.toml");
        fs::write(&replacement, sample("External")).unwrap();
        fs::rename(&replacement, &path).unwrap();
        assert!(matches!(
            store.save(&updated, SaveConflictPolicy::Abort),
            Err(Error::Conflict)
        ));
        assert_eq!(fs::read_to_string(&path).unwrap(), sample("External"));

        let result = store.save(&updated, SaveConflictPolicy::Overwrite).unwrap();
        let backup = result.backup_path.expect("overwrite should report backup");
        assert_eq!(fs::read_to_string(&backup).unwrap(), sample("External"));
        assert_eq!(
            fs::metadata(&backup).unwrap().permissions().mode() & 0o777,
            0o600
        );
        let saved = fs::read_to_string(&path).unwrap();
        assert!(saved.contains("label = \"Office\""));
        assert!(!saved.contains("label = \"External\""));
    }

    #[test]
    fn atomicity_failure_leaves_the_original_file_unchanged() {
        let dir = TempDir::new().unwrap();
        let path = write_sample(&dir, "Work");
        let original = fs::read_to_string(&path).unwrap();
        let mut store = ConfigurationStore::open_path(&path).unwrap();
        let updated = relabel(store.configuration.clone().unwrap(), "Office");
        fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o555)).unwrap();
        let result = store.save(&updated, SaveConflictPolicy::Abort);
        fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o755)).unwrap();
        assert!(matches!(
            result,
            Err(Error::Save(ErrorKind::PermissionDenied))
        ));
        assert_eq!(fs::read_to_string(&path).unwrap(), original);
    }

    #[test]
    fn symlink_save_updates_the_regular_file_target() {
        let dir = TempDir::new().unwrap();
        let target = dir.path().join("real.toml");
        let path = dir.path().join("config.toml");
        fs::write(&target, sample("Work")).unwrap();
        unix_fs::symlink(&target, &path).unwrap();
        let mut store = ConfigurationStore::open_path(&path).unwrap();
        let updated = relabel(store.configuration.clone().unwrap(), "Office");
        store.save(&updated, SaveConflictPolicy::Abort).unwrap();
        assert!(path.symlink_metadata().unwrap().file_type().is_symlink());
        assert!(fs::read_to_string(&target).unwrap().contains("Office"));
        assert!(!fs::read_to_string(&path).unwrap().contains("Work"));
    }

    #[test]
    fn new_configuration_is_owner_only_and_broad_modes_warn() {
        let dir = TempDir::new().unwrap();
        let path = write_sample(&dir, "Work");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
        let mut store = ConfigurationStore::open_path(&path).unwrap();
        assert!(
            store
                .warnings
                .iter()
                .any(|warning| warning.contains("group or others"))
        );
        let updated = relabel(store.configuration.clone().unwrap(), "Office");
        store.save(&updated, SaveConflictPolicy::Abort).unwrap();
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o644
        );

        let created = dir.path().join("fresh.toml");
        let configuration = store.configuration.clone().unwrap();
        ConfigurationStore::create_path(&created)
            .unwrap()
            .save(&configuration, SaveConflictPolicy::Abort)
            .unwrap();
        assert_eq!(
            fs::metadata(&created).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }

    #[test]
    fn successful_save_prunes_older_picker_backups_to_five() {
        let dir = TempDir::new().unwrap();
        let path = write_sample(&dir, "Work");
        for index in 1..=6 {
            fs::write(
                dir.path()
                    .join(format!("config.toml.bak-2020010{index}T000000Z")),
                format!("old-{index}"),
            )
            .unwrap();
        }
        fs::write(dir.path().join("config.toml.bak-keep-me"), "user").unwrap();
        let mut store = ConfigurationStore::open_path(&path).unwrap();
        let updated = relabel(store.configuration.clone().unwrap(), "Office");
        store.save(&updated, SaveConflictPolicy::Abort).unwrap();
        let mut backups: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|name| name.contains(".bak-"))
            .collect();
        backups.sort();
        assert_eq!(
            backups,
            vec![
                "config.toml.bak-20200102T000000Z",
                "config.toml.bak-20200103T000000Z",
                "config.toml.bak-20200104T000000Z",
                "config.toml.bak-20200105T000000Z",
                "config.toml.bak-20200106T000000Z",
                "config.toml.bak-keep-me",
            ]
        );
        assert_eq!(
            fs::read_to_string(dir.path().join("config.toml.bak-keep-me")).unwrap(),
            "user"
        );
    }

    #[test]
    fn save_preserves_comments_on_unidentified_condition_groups() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("config.toml");
        fs::write(
            &path,
            "# header\nversion = 1\n\n[[destinations]]\nid = \"work\"\nlabel = \"Work\"\n\n[destinations.application]\ntype = \"manual\"\nexecutable = \"/bin/true\"\nargs = [\"{target}\"]\n\n[[rules]]\nid = \"docs\"\nname = \"Docs\"\nenabled = true\n\n# group note\n[[rules.groups]]\n# condition note\n[[rules.groups.conditions]]\ntype = \"host\"\nvalue = \"example.com\"\n\n[rules.action]\ntype = \"open\"\ndestination = \"work\"\n\n[fallback]\naction = \"show-picker\"\n",
        )
        .unwrap();
        let mut store = ConfigurationStore::open_path(&path).unwrap();
        let updated = relabel(store.configuration.clone().unwrap(), "Office");
        store.save(&updated, SaveConflictPolicy::Abort).unwrap();
        let saved = fs::read_to_string(&path).unwrap();
        assert!(saved.contains("# group note"), "{saved}");
        assert!(saved.contains("# condition note"), "{saved}");
        assert!(saved.contains("label = \"Office\""), "{saved}");
    }

    #[test]
    fn directory_target_is_rejected() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("config.toml");
        fs::create_dir(&path).unwrap();
        assert!(matches!(
            ConfigurationStore::open_path(&path),
            Err(Error::NotRegularFile)
        ));
    }

    fn fixture(name: &str) -> String {
        fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/schema")
                .join(name),
        )
        .expect("schema fixture should be readable")
    }

    #[test]
    fn released_schema_fixture_loads_as_current() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("config.toml");
        let source = fixture("v1.toml");
        fs::write(&path, &source).unwrap();
        let store = ConfigurationStore::inspect_path(&path).unwrap();
        assert!(matches!(store.status, StoreStatus::Current));
        let configuration = store.configuration.expect("v1 fixture should load");
        assert_eq!(configuration.destinations[0].id, "work");
        assert_eq!(fs::read_to_string(&path).unwrap(), source);
    }

    #[test]
    fn newer_unknown_version_is_refused_without_modification() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("config.toml");
        let source = "version = 2\nunknown_field = true\nsecret = \"https://user:pass@example.com/?q=token\"\ndestinations = []\n[fallback]\naction = \"show-picker\"\n";
        fs::write(&path, source).unwrap();
        let store = ConfigurationStore::inspect_path(&path).unwrap();
        let StoreStatus::Invalid(error) = store.status else {
            panic!("newer schema should be invalid");
        };
        assert!(matches!(error, Error::UnsupportedVersion(2)), "{error:?}");
        let message = error.message();
        assert!(message.contains("2"), "{message}");
        assert!(!message.contains("https://"));
        assert!(!message.contains("token"));
        assert!(!message.contains("unknown_field"));
        assert_eq!(fs::read_to_string(&path).unwrap(), source);
        assert!(store.configuration.is_none());
    }

    #[test]
    fn unknown_key_is_reported_without_rewriting_or_leaking_values() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("config.toml");
        let source = "# keep\nversion = 1\n\n[[destinations]]\nid = \"work\"\nlabel = \"Work\"\n\n[destinations.application]\ntype = \"manual\"\nexecutable = \"/bin/true\"\nargs = [\"{target}\"]\ncommand = \"https://user:secret@example.com/path?q=1\"\n\n[fallback]\naction = \"show-picker\"\n";
        fs::write(&path, source).unwrap();
        let store = ConfigurationStore::inspect_path(&path).unwrap();
        let StoreStatus::Invalid(error) = store.status else {
            panic!("unknown key should be invalid");
        };
        let message = error.message();
        assert!(message.contains("command"), "{message}");
        assert!(message.contains("line"), "{message}");
        assert!(!message.contains("https://"));
        assert!(!message.contains("secret"));
        assert_eq!(fs::read_to_string(&path).unwrap(), source);
    }

    #[test]
    fn invalid_toml_and_semantic_errors_preserve_the_file() {
        let dir = TempDir::new().unwrap();
        let broken = dir.path().join("broken.toml");
        let broken_source = "version = [\n";
        fs::write(&broken, broken_source).unwrap();
        let store = ConfigurationStore::inspect_path(&broken).unwrap();
        assert!(matches!(
            store.status,
            StoreStatus::Invalid(Error::InvalidToml)
        ));
        assert_eq!(fs::read_to_string(&broken).unwrap(), broken_source);

        let semantic = dir.path().join("semantic.toml");
        let semantic_source = "version = 1\n\n[[destinations]]\nid = \"Not Stable\"\nlabel = \"Work\"\n\n[destinations.application]\ntype = \"manual\"\nexecutable = \"/bin/true\"\nargs = [\"{target}\"]\n\n[fallback]\naction = \"open\"\ndestination = \"Not Stable\"\n";
        fs::write(&semantic, semantic_source).unwrap();
        let store = ConfigurationStore::inspect_path(&semantic).unwrap();
        let StoreStatus::Invalid(error) = store.status else {
            panic!("semantic error should be invalid");
        };
        assert!(matches!(error, Error::InvalidId(_)));
        assert_eq!(fs::read_to_string(&semantic).unwrap(), semantic_source);
        assert!(store.configuration.is_none());
    }

    #[test]
    fn old_schema_preview_does_not_write_until_confirmed_migration() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("config.toml");
        let source = fixture("v0.toml");
        fs::write(&path, &source).unwrap();
        let mut store = ConfigurationStore::inspect_path(&path).unwrap();
        let StoreStatus::Migratable(preview) = store.status.clone() else {
            panic!("version 0 should be migratable");
        };
        assert_eq!(preview.from, 0);
        assert_eq!(preview.to, 1);
        assert!(preview.message().contains("0"));
        assert!(preview.message().contains("1"));
        assert!(!preview.message().contains("secret"));
        assert!(!preview.message().contains("https://"));
        assert_eq!(fs::read_to_string(&path).unwrap(), source);

        let result = store.migrate().unwrap();
        let backup = result.backup_path.expect("migration should backup");
        assert_eq!(fs::read_to_string(&backup).unwrap(), source);
        assert_eq!(
            fs::metadata(&backup).unwrap().permissions().mode() & 0o777,
            0o600
        );
        let saved = fs::read_to_string(&path).unwrap();
        assert!(saved.contains("version = 1"), "{saved}");
        assert!(!saved.contains("version = 0"), "{saved}");
        assert!(saved.contains("# Browser Picker configuration"), "{saved}");
        assert!(saved.contains("value = \"secret\""), "{saved}");
        assert!(matches!(store.status, StoreStatus::Current));
    }

    #[test]
    fn migration_backup_failure_leaves_the_original_file_unchanged() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("config.toml");
        let source = fixture("v0.toml");
        fs::write(&path, &source).unwrap();
        let mut store = ConfigurationStore::inspect_path(&path).unwrap();
        fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o555)).unwrap();
        let result = store.migrate();
        fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o755)).unwrap();
        assert!(
            matches!(result, Err(Error::Backup(ErrorKind::PermissionDenied))),
            "{result:?}"
        );
        assert_eq!(fs::read_to_string(&path).unwrap(), source);
    }
}
