use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::process::Command;

use tempfile::TempDir;

fn browser_picker(config_home: &TempDir) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_browser-picker"));
    command
        .env("DISPLAY", ":99")
        .env_remove("WAYLAND_DISPLAY")
        .env_remove("DBUS_SESSION_BUS_ADDRESS")
        .env("XDG_CONFIG_HOME", config_home.path())
        .env("XDG_STATE_HOME", config_home.path().join("state"))
        .env("HOME", config_home.path());
    command
}

fn install_fake_browser(config_home: &TempDir) -> std::path::PathBuf {
    let executable = config_home.path().join("controlled-browser");
    fs::write(
        &executable,
        "#!/bin/sh\nprintf '%s\\n' \"$@\" > \"$BROWSER_PICKER_TEST_OUTPUT\"\n",
    )
    .expect("fake browser should be writable");
    fs::set_permissions(&executable, fs::Permissions::from_mode(0o700))
        .expect("fake browser should be executable");
    executable
}

fn write_raw_config(config_home: &TempDir, source: &str) {
    let config_dir = config_home.path().join("browser-picker");
    fs::create_dir(&config_dir).expect("configuration directory should be writable");
    fs::write(config_dir.join("config.toml"), source).expect("configuration should be writable");
}

fn write_config(config_home: &TempDir, executable: &std::path::Path, fallback: &str) {
    let config_dir = config_home.path().join("browser-picker");
    fs::create_dir(&config_dir).expect("configuration directory should be writable");
    fs::write(
        config_dir.join("config.toml"),
        format!(
            "version = 1\n\n[[destinations]]\nid = \"controlled\"\nlabel = \"Controlled Browser\"\n\n[destinations.application]\ntype = \"manual\"\nexecutable = \"{}\"\nargs = [\"{{target}}\"]\n\n[fallback]\naction = \"{fallback}\"\ndestination = \"controlled\"\n",
            executable.display()
        ),
    )
    .expect("configuration should be writable");
}

#[test]
fn configuration_opening_requires_session_bus_even_with_a_display() {
    let config_home = TempDir::new().expect("temporary configuration home should be created");

    let output = browser_picker(&config_home)
        .arg("config")
        .output()
        .expect("Browser Picker should start");

    assert_eq!(output.status.code(), Some(5));
    assert!(output.stdout.is_empty());
    assert_eq!(
        String::from_utf8(output.stderr).expect("error output should be UTF-8"),
        "Picker, configuration, and shared queue operations require a user session D-Bus\n"
    );
}

#[test]
fn picker_fallback_fails_without_session_bus_and_does_not_dispatch() {
    let config_home = TempDir::new().expect("temporary configuration home should be created");
    let executable = install_fake_browser(&config_home);
    write_config(&config_home, &executable, "show-picker");
    let received = config_home.path().join("received-argv");

    let output = browser_picker(&config_home)
        .env("BROWSER_PICKER_TEST_OUTPUT", &received)
        .arg("https://user:secret@example.com/path?token=do-not-print")
        .output()
        .expect("Browser Picker should start");
    let stderr = String::from_utf8(output.stderr).expect("error output should be UTF-8");

    assert_eq!(output.status.code(), Some(5));
    assert!(output.stdout.is_empty());
    assert_eq!(
        stderr,
        "Picker, configuration, and shared queue operations require a user session D-Bus\n"
    );
    assert!(!stderr.contains("secret"));
    assert!(!stderr.contains("token"));
    assert!(!stderr.contains("example.com"));
    assert!(!received.exists());
}

#[test]
fn multiple_targets_fail_without_session_bus() {
    let config_home = TempDir::new().expect("temporary configuration home should be created");
    let executable = install_fake_browser(&config_home);
    write_config(&config_home, &executable, "open");

    let output = browser_picker(&config_home)
        .arg("https://example.com/one")
        .arg("https://example.com/two")
        .output()
        .expect("Browser Picker should start");

    assert_eq!(output.status.code(), Some(5));
    assert_eq!(
        String::from_utf8(output.stderr).expect("error output should be UTF-8"),
        "Picker, configuration, and shared queue operations require a user session D-Bus\n"
    );
}

fn assert_redacted(text: &str) {
    for secret in [
        "secret",
        "token",
        "password",
        "example.com",
        "do-not-print",
        "page.html",
    ] {
        assert!(!text.contains(secret), "output leaked {secret:?}: {text:?}");
    }
}

#[test]
fn diagnose_reports_redacted_automatic_web_routing_without_dispatch() {
    let config_home = TempDir::new().expect("temporary configuration home should be created");
    let executable = install_fake_browser(&config_home);
    write_raw_config(
        &config_home,
        &format!(
            "version = 1\n\n[[destinations]]\nid = \"controlled\"\nlabel = \"Controlled Browser\"\n\n[destinations.application]\ntype = \"manual\"\nexecutable = \"{}\"\nargs = [\"{{target}}\"]\nprivate_args = [\"--private\", \"{{target}}\"]\n\n[[rules]]\nid = \"work\"\nname = \"Work\"\nenabled = true\n\n[rules.action]\ntype = \"open\"\ndestination = \"controlled\"\nmode = \"private\"\n\n[[rules.groups]]\n\n[[rules.groups.conditions]]\ntype = \"host\"\nvalue = \"example.com\"\n\n[fallback]\naction = \"show-picker\"\n",
            executable.display()
        ),
    );
    let received = config_home.path().join("received-argv");
    let target = "https://user:secret@example.com/Path?token=do-not-print#frag";

    let output = browser_picker(&config_home)
        .env("BROWSER_PICKER_TEST_OUTPUT", &received)
        .env_remove("DISPLAY")
        .arg("diagnose")
        .arg(target)
        .output()
        .expect("Browser Picker should start");
    let stdout = String::from_utf8(output.stdout).expect("diagnostic output should be UTF-8");
    let stderr = String::from_utf8(output.stderr).expect("error output should be UTF-8");

    assert!(output.status.success(), "stderr={stderr}");
    assert!(stderr.is_empty());
    assert_eq!(
        stdout,
        "kind=web\nresult=dispatched\nrule=work\ndestination=controlled\nmode=private\nprivate=available\navailability=available\n"
    );
    assert_redacted(&stdout);
    assert!(!received.exists());
}

#[test]
fn diagnose_reports_file_picker_without_revealing_the_path() {
    let config_home = TempDir::new().expect("temporary configuration home should be created");
    let executable = install_fake_browser(&config_home);
    write_config(&config_home, &executable, "open");
    let path = config_home.path().join("page.html");
    fs::write(&path, "<html></html>").expect("HTML fixture should be writable");

    let output = browser_picker(&config_home)
        .env_remove("DISPLAY")
        .arg("diagnose")
        .arg(&path)
        .output()
        .expect("Browser Picker should start");
    let stdout = String::from_utf8(output.stdout).expect("diagnostic output should be UTF-8");
    let stderr = String::from_utf8(output.stderr).expect("error output should be UTF-8");

    assert!(output.status.success(), "stderr={stderr}");
    assert_eq!(stdout, "kind=file\nresult=picker\n");
    assert!(!stdout.contains("page.html"));
    assert!(!stdout.contains(path.to_str().unwrap()));
}

#[test]
fn diagnose_keeps_malformed_input_redacted() {
    let config_home = TempDir::new().expect("temporary configuration home should be created");

    let output = browser_picker(&config_home)
        .env_remove("DISPLAY")
        .arg("diagnose")
        .arg("ftp://user:password@example.com/path?token=do-not-print")
        .output()
        .expect("Browser Picker should start");
    let stderr = String::from_utf8(output.stderr).expect("error output should be UTF-8");
    let stdout = String::from_utf8(output.stdout).expect("diagnostic output should be UTF-8");

    assert_eq!(output.status.code(), Some(2));
    assert!(stdout.is_empty());
    assert_eq!(
        stderr,
        "Unsupported Open Target scheme; expected HTTP, HTTPS, or a local file\n"
    );
    assert_redacted(&stderr);
}

#[test]
fn diagnose_reports_preselection_without_dispatch() {
    let config_home = TempDir::new().expect("temporary configuration home should be created");
    let executable = install_fake_browser(&config_home);
    write_raw_config(
        &config_home,
        &format!(
            "version = 1\n\n[[destinations]]\nid = \"controlled\"\nlabel = \"Controlled Browser\"\n\n[destinations.application]\ntype = \"manual\"\nexecutable = \"{}\"\nargs = [\"{{target}}\"]\n\n[[rules]]\nid = \"suggest\"\nname = \"Suggest\"\nenabled = true\n\n[rules.action]\ntype = \"preselect\"\ndestination = \"controlled\"\n\n[[rules.groups]]\n\n[[rules.groups.conditions]]\ntype = \"host\"\nvalue = \"suggested.example\"\n\n[fallback]\naction = \"open\"\ndestination = \"controlled\"\n",
            executable.display()
        ),
    );
    let received = config_home.path().join("received-argv");

    let output = browser_picker(&config_home)
        .env("BROWSER_PICKER_TEST_OUTPUT", &received)
        .env_remove("DISPLAY")
        .arg("diagnose")
        .arg("https://suggested.example/path")
        .output()
        .expect("Browser Picker should start");
    let stdout = String::from_utf8(output.stdout).expect("diagnostic output should be UTF-8");

    assert!(output.status.success());
    assert_eq!(
        stdout,
        "kind=web\nresult=preselection\nrule=suggest\ndestination=controlled\nmode=normal\nprivate=unavailable\navailability=available\n"
    );
    assert!(!received.exists());
}

fn state_contents(config_home: &TempDir) -> String {
    let state = config_home.path().join("state");
    let mut collected = String::new();
    if !state.exists() {
        return collected;
    }
    for entry in walkdir(&state) {
        if entry.is_file() {
            collected.push_str(entry.to_str().unwrap_or_default());
            collected.push('\n');
            if let Ok(contents) = fs::read_to_string(&entry) {
                collected.push_str(&contents);
            }
        }
    }
    collected
}

fn walkdir(path: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut files = Vec::new();
    if let Ok(entries) = fs::read_dir(path) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                files.extend(walkdir(&path));
            } else {
                files.push(path);
            }
        }
    }
    files
}

#[test]
fn diagnose_and_automatic_routing_do_not_persist_targets_or_telemetry() {
    let config_home = TempDir::new().expect("temporary configuration home should be created");
    let executable = install_fake_browser(&config_home);
    write_config(&config_home, &executable, "open");
    let received = config_home.path().join("received-argv");
    let target = "https://user:secret@example.com/Path?token=do-not-print";

    let diagnose = browser_picker(&config_home)
        .env_remove("DISPLAY")
        .arg("diagnose")
        .arg(target)
        .output()
        .expect("Browser Picker should start");
    assert!(diagnose.status.success());

    let routed = browser_picker(&config_home)
        .env("BROWSER_PICKER_TEST_OUTPUT", &received)
        .arg(target)
        .output()
        .expect("Browser Picker should start");
    assert!(routed.status.success());

    let persisted = state_contents(&config_home);
    assert_redacted(&persisted);
    assert!(!persisted.to_lowercase().contains("telemetry"));
    assert!(!persisted.to_lowercase().contains("crash"));
    assert!(!persisted.to_lowercase().contains("update"));
    for path in walkdir(&config_home.path().join("state")) {
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("");
        assert!(!name.contains("socket"));
        assert!(!name.contains("lock"));
        assert!(!name.contains("telemetry"));
        assert!(!name.contains("crash"));
    }
}

#[test]
fn unusable_session_bus_is_a_forwarding_failure() {
    let config_home = TempDir::new().expect("temporary configuration home should be created");

    let output = browser_picker(&config_home)
        .env(
            "DBUS_SESSION_BUS_ADDRESS",
            "unix:path=/tmp/browser-picker-missing-session-bus",
        )
        .arg("config")
        .output()
        .expect("Browser Picker should start");

    assert_eq!(output.status.code(), Some(7));
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8(output.stderr).expect("error output should be UTF-8");
    assert_eq!(
        stderr,
        "Could not forward to the Browser Picker session on D-Bus\n"
    );
}
