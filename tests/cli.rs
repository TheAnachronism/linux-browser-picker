use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::process::Command;

use tempfile::TempDir;

const HELP: &str = "\
Browser Picker
Choose where links and local files open.

Usage:
  browser-picker
  browser-picker <http(s)-url|file>...
  browser-picker config
  browser-picker validate
  browser-picker diagnose <http(s)-url|file>
  browser-picker help
  browser-picker version

Operations:
  config      Open configuration
  validate    Validate configuration
  diagnose    Print redacted routing diagnostics
  help        Show this help
  version     Show version

Options:
  -h, --help       Show this help
  -V, --version    Show version

Exit statuses:
  0  success
  1  unknown argument
  2  invalid Open Target
  3  invalid configuration
  4  dispatch failure
  5  environment
  6  queue overflow
  7  forwarding failure
";

fn browser_picker() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_browser-picker"));
    command
        .env_remove("DISPLAY")
        .env_remove("WAYLAND_DISPLAY")
        .env_remove("DBUS_SESSION_BUS_ADDRESS");
    command
}

fn browser_picker_with_config(config_home: &TempDir) -> Command {
    let mut command = browser_picker();
    command.env("XDG_CONFIG_HOME", config_home.path());
    command.env("XDG_STATE_HOME", config_home.path().join("state"));
    command.env("HOME", config_home.path());
    command
}

fn write_valid_config(config_home: &TempDir) {
    let config_dir = config_home.path().join("browser-picker");
    fs::create_dir(&config_dir).expect("configuration directory should be writable");
    let executable = config_home.path().join("controlled-browser");
    fs::write(&executable, "#!/bin/sh\n").expect("fake browser should be writable");
    fs::set_permissions(&executable, fs::Permissions::from_mode(0o700))
        .expect("fake browser should be executable");
    fs::write(
        config_dir.join("config.toml"),
        format!(
            "version = 1\n\n[[destinations]]\nid = \"controlled\"\nlabel = \"Controlled Browser\"\n\n[destinations.application]\ntype = \"manual\"\nexecutable = \"{}\"\nargs = [\"{{target}}\"]\n\n[fallback]\naction = \"open\"\ndestination = \"controlled\"\n",
            executable.display()
        ),
    )
    .expect("configuration should be writable");
    fs::set_permissions(
        config_dir.join("config.toml"),
        fs::Permissions::from_mode(0o600),
    )
    .expect("valid configuration should be owner-only");
}

#[test]
fn help_succeeds_without_a_graphical_session() {
    let output = browser_picker()
        .arg("--help")
        .output()
        .expect("Browser Picker should start");

    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).expect("help output should be UTF-8"),
        HELP
    );
    assert!(output.stderr.is_empty());
}

#[test]
fn help_operation_succeeds_without_a_graphical_session() {
    let output = browser_picker()
        .arg("help")
        .output()
        .expect("Browser Picker should start");

    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).expect("help output should be UTF-8"),
        HELP
    );
    assert!(output.stderr.is_empty());
}

#[test]
fn version_succeeds_without_a_graphical_session() {
    let output = browser_picker()
        .arg("--version")
        .output()
        .expect("Browser Picker should start");

    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).expect("version output should be UTF-8"),
        concat!("Browser Picker ", env!("CARGO_PKG_VERSION"), "\n")
    );
    assert!(output.stderr.is_empty());
}

#[test]
fn version_operation_succeeds_without_a_graphical_session() {
    let output = browser_picker()
        .arg("version")
        .output()
        .expect("Browser Picker should start");

    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).expect("version output should be UTF-8"),
        concat!("Browser Picker ", env!("CARGO_PKG_VERSION"), "\n")
    );
    assert!(output.stderr.is_empty());
}

#[test]
fn unknown_argument_fails_without_a_graphical_session() {
    let output = browser_picker()
        .arg("--bogus")
        .output()
        .expect("Browser Picker should start");

    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert_eq!(
        String::from_utf8(output.stderr).expect("error output should be UTF-8"),
        "Unknown argument: --bogus\n"
    );
}

#[test]
fn diagnose_without_a_target_fails_with_a_usage_status() {
    let output = browser_picker()
        .arg("diagnose")
        .output()
        .expect("Browser Picker should start");

    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert_eq!(
        String::from_utf8(output.stderr).expect("error output should be UTF-8"),
        "diagnose requires an Open Target\n"
    );
}

#[test]
fn validate_accepts_valid_configuration_without_a_graphical_session() {
    let config_home = TempDir::new().expect("temporary configuration home should be created");
    write_valid_config(&config_home);

    let output = browser_picker_with_config(&config_home)
        .arg("validate")
        .output()
        .expect("Browser Picker should start");

    assert!(output.status.success());
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
}

#[test]
fn validate_reports_missing_configuration_without_a_graphical_session() {
    let config_home = TempDir::new().expect("temporary configuration home should be created");

    let output = browser_picker_with_config(&config_home)
        .arg("validate")
        .output()
        .expect("Browser Picker should start");

    assert_eq!(output.status.code(), Some(3));
    assert!(output.stdout.is_empty());
    assert_eq!(
        String::from_utf8(output.stderr).expect("error output should be UTF-8"),
        "Browser Picker configuration was not found\n"
    );
}

#[test]
fn validate_reports_invalid_configuration_without_revealing_an_open_target() {
    let config_home = TempDir::new().expect("temporary configuration home should be created");
    let config_dir = config_home.path().join("browser-picker");
    fs::create_dir(&config_dir).expect("configuration directory should be writable");
    fs::write(
        config_dir.join("config.toml"),
        "version = 1\nthis is not [[toml\n",
    )
    .expect("configuration should be writable");
    fs::set_permissions(
        config_dir.join("config.toml"),
        fs::Permissions::from_mode(0o600),
    )
    .expect("invalid configuration should stay owner-only");

    let output = browser_picker_with_config(&config_home)
        .arg("validate")
        .output()
        .expect("Browser Picker should start");
    let stderr = String::from_utf8(output.stderr).expect("error output should be UTF-8");

    assert_eq!(output.status.code(), Some(3));
    assert!(output.stdout.is_empty());
    assert_eq!(stderr, "Configuration is not valid versioned TOML\n");
    assert!(!stderr.contains("secret"));
}

#[test]
fn validate_warns_about_broad_permissions_without_revealing_query_values() {
    let config_home = TempDir::new().expect("temporary configuration home should be created");
    write_valid_config(&config_home);
    let path = config_home.path().join("browser-picker/config.toml");
    let source = format!(
        "{}\n[[rules]]\nid = \"docs\"\nname = \"Docs\"\nenabled = true\n\n[[rules.groups]]\n\n[[rules.groups.conditions]]\ntype = \"query-value\"\nkey = \"q\"\nvalue = \"super-secret-token\"\n\n[rules.action]\ntype = \"preselect\"\ndestination = \"controlled\"\n",
        fs::read_to_string(&path)
            .expect("valid configuration should be readable")
            .trim_end(),
    );
    fs::write(&path, &source).expect("configuration should be writable");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o644))
        .expect("broad permissions should be set");

    let output = browser_picker_with_config(&config_home)
        .arg("validate")
        .output()
        .expect("Browser Picker should start");
    let stderr = String::from_utf8(output.stderr).expect("warning output should be UTF-8");

    assert!(output.status.success());
    assert!(output.stdout.is_empty());
    assert!(
        stderr.contains("readable by group or others"),
        "stderr={stderr}"
    );
    assert!(stderr.contains("plain text"), "stderr={stderr}");
    assert!(!stderr.contains("super-secret-token"));
    assert!(!stderr.contains("q="));
    assert_eq!(
        fs::read_to_string(&path).expect("file should remain"),
        source
    );
}

#[test]
fn validate_reports_unusable_configuration_paths_without_changing_them() {
    let config_home = TempDir::new().expect("temporary configuration home should be created");
    let config_dir = config_home.path().join("browser-picker");
    fs::create_dir(&config_dir).expect("configuration directory should be writable");
    let path = config_dir.join("config.toml");
    fs::create_dir(&path).expect("directory target should be created");

    let output = browser_picker_with_config(&config_home)
        .arg("validate")
        .output()
        .expect("Browser Picker should start");
    let stderr = String::from_utf8(output.stderr).expect("error output should be UTF-8");

    assert_eq!(output.status.code(), Some(3));
    assert!(output.stdout.is_empty());
    assert_eq!(stderr, "Configuration must be a regular file\n");
    assert!(path.is_dir());

    fs::remove_dir(&path).expect("directory target should be removable");
    std::os::unix::fs::symlink(config_dir.join("missing.toml"), &path)
        .expect("broken symlink should be created");
    let output = browser_picker_with_config(&config_home)
        .arg("validate")
        .output()
        .expect("Browser Picker should start");
    let stderr = String::from_utf8(output.stderr).expect("error output should be UTF-8");
    assert_eq!(output.status.code(), Some(3));
    assert_eq!(stderr, "Configuration symlink is broken\n");
    assert!(path.symlink_metadata().unwrap().file_type().is_symlink());
}

#[test]
fn configuration_opening_is_rejected_without_a_graphical_session() {
    let config_home = TempDir::new().expect("temporary configuration home should be created");

    let output = browser_picker_with_config(&config_home)
        .arg("config")
        .output()
        .expect("Browser Picker should start");

    assert_eq!(output.status.code(), Some(5));
    assert!(output.stdout.is_empty());
    assert_eq!(
        String::from_utf8(output.stderr).expect("error output should be UTF-8"),
        "Opening configuration requires a graphical session\n"
    );
}

#[test]
fn bare_invocation_is_rejected_without_a_graphical_session() {
    let output = browser_picker()
        .output()
        .expect("Browser Picker should start");

    assert_eq!(output.status.code(), Some(5));
    assert!(output.stdout.is_empty());
    assert_eq!(
        String::from_utf8(output.stderr).expect("error output should be UTF-8"),
        "Opening configuration requires a graphical session\n"
    );
}
