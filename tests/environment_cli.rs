use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};
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
        "#!/bin/sh\nprintf '%s\\n' \"$@\" >> \"$BROWSER_PICKER_TEST_OUTPUT\"\n",
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
fn multiple_automatic_targets_dispatch_in_order_without_session_bus() {
    let config_home = TempDir::new().expect("temporary configuration home should be created");
    let executable = install_fake_browser(&config_home);
    write_config(&config_home, &executable, "open");
    let received = config_home.path().join("received-argv");

    let output = browser_picker(&config_home)
        .env("BROWSER_PICKER_TEST_OUTPUT", &received)
        .arg("https://example.com/one")
        .arg("https://example.com/two")
        .output()
        .expect("Browser Picker should start");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
    // Dispatch is sequential; fake browsers still race when appending argv.
    let mut received_targets = wait_for_argv_records(&received, 2);
    received_targets.sort();
    assert_eq!(
        received_targets,
        ["https://example.com/one", "https://example.com/two"]
    );
}

#[test]
fn duplicate_automatic_targets_remain_distinct_without_session_bus() {
    let config_home = TempDir::new().expect("temporary configuration home should be created");
    let executable = install_fake_browser(&config_home);
    write_config(&config_home, &executable, "open");
    let received = config_home.path().join("received-argv");
    let target = "https://example.com/repeat";

    let output = browser_picker(&config_home)
        .env("BROWSER_PICKER_TEST_OUTPUT", &received)
        .arg(target)
        .arg(target)
        .output()
        .expect("Browser Picker should start");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(wait_for_argv_records(&received, 2), [target, target]);
}

#[test]
fn choice_requiring_targets_fail_without_dropping_accepted_automatic_dispatches() {
    let config_home = TempDir::new().expect("temporary configuration home should be created");
    let executable = install_fake_browser(&config_home);
    write_raw_config(
        &config_home,
        &format!(
            "version = 1\n\n[[destinations]]\nid = \"controlled\"\nlabel = \"Controlled Browser\"\n\n[destinations.application]\ntype = \"manual\"\nexecutable = \"{}\"\nargs = [\"{{target}}\"]\n\n[[rules]]\nid = \"suggest\"\nname = \"Suggest\"\nenabled = true\n\n[rules.action]\ntype = \"preselect\"\ndestination = \"controlled\"\n\n[[rules.groups]]\n\n[[rules.groups.conditions]]\ntype = \"host\"\nvalue = \"suggested.example\"\n\n[fallback]\naction = \"open\"\ndestination = \"controlled\"\n",
            executable.display()
        ),
    );
    let path = config_home.path().join("page.html");
    fs::write(&path, "<html></html>").expect("HTML fixture should be writable");
    let received = config_home.path().join("received-argv");
    let accepted = "https://example.com/ok";

    let output = browser_picker(&config_home)
        .env("BROWSER_PICKER_TEST_OUTPUT", &received)
        .arg(&path)
        .arg(accepted)
        .arg("https://suggested.example/path")
        .output()
        .expect("Browser Picker should start");
    let stderr = String::from_utf8(output.stderr).expect("error output should be UTF-8");

    assert_eq!(output.status.code(), Some(5));
    assert!(output.stdout.is_empty());
    assert_eq!(
        stderr,
        "Picker, configuration, and shared queue operations require a user session D-Bus\n\
Picker, configuration, and shared queue operations require a user session D-Bus\n"
    );
    assert!(!stderr.contains("page.html"));
    assert!(!stderr.contains(path.to_str().unwrap()));
    assert!(!stderr.contains("suggested.example"));
    assert_eq!(wait_for_argv_records(&received, 1), [accepted]);
}

#[test]
fn setup_targets_fail_without_session_bus_and_do_not_dispatch() {
    let config_home = TempDir::new().expect("temporary configuration home should be created");
    let received = config_home.path().join("received-argv");

    let output = browser_picker(&config_home)
        .env("BROWSER_PICKER_TEST_OUTPUT", &received)
        .arg("https://example.com/one")
        .arg("https://example.com/two")
        .output()
        .expect("Browser Picker should start");

    assert_eq!(output.status.code(), Some(5));
    assert!(output.stdout.is_empty());
    assert_eq!(
        String::from_utf8(output.stderr).expect("error output should be UTF-8"),
        "Picker, configuration, and shared queue operations require a user session D-Bus\n\
Picker, configuration, and shared queue operations require a user session D-Bus\n"
    );
    assert!(!received.exists());
}

#[test]
fn invalid_and_oversized_targets_do_not_suppress_accepted_automatic_dispatches() {
    let config_home = TempDir::new().expect("temporary configuration home should be created");
    let executable = install_fake_browser(&config_home);
    write_config(&config_home, &executable, "open");
    let received = config_home.path().join("received-argv");
    let accepted = "https://example.com/ok";
    let prefix = "https://example.com/";
    let oversized = format!("{prefix}{}", "a".repeat((64 * 1024) - prefix.len() + 1));

    let output = browser_picker(&config_home)
        .env("BROWSER_PICKER_TEST_OUTPUT", &received)
        .arg("https://")
        .arg(accepted)
        .arg(&oversized)
        .output()
        .expect("Browser Picker should start");
    let stderr = String::from_utf8(output.stderr).expect("error output should be UTF-8");

    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert_eq!(
        stderr,
        "Invalid Open Target: expected an existing regular local file or an absolute HTTP or HTTPS URL\n\
Open Target exceeds the 65536-byte limit\n"
    );
    assert_eq!(wait_for_argv_records(&received, 1), [accepted]);
}

#[test]
fn excess_targets_overflow_without_dropping_the_accepted_prefix() {
    let config_home = TempDir::new().expect("temporary configuration home should be created");
    let executable = install_fake_browser(&config_home);
    write_config(&config_home, &executable, "open");
    let received = config_home.path().join("received-argv");
    let accepted: Vec<String> = (0..100)
        .map(|index| format!("https://example.com/{index}"))
        .collect();
    let excess = "https://example.com/excess";

    let mut command = browser_picker(&config_home);
    command.env("BROWSER_PICKER_TEST_OUTPUT", &received);
    for target in &accepted {
        command.arg(target);
    }
    let output = command
        .arg(excess)
        .output()
        .expect("Browser Picker should start");
    let stderr = String::from_utf8(output.stderr).expect("error output should be UTF-8");

    assert_eq!(output.status.code(), Some(6));
    assert!(output.stdout.is_empty());
    assert_eq!(stderr, "One activation accepts at most 100 Open Targets\n");
    let mut received_targets = wait_for_argv_records(&received, 100);
    received_targets.sort();
    let mut expected = accepted;
    expected.sort();
    assert_eq!(received_targets, expected);
    assert!(!received_targets.iter().any(|target| target == excess));
}

fn wait_for_argv_records(path: &std::path::Path, count: usize) -> Vec<String> {
    let deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < deadline {
        if let Ok(contents) = fs::read_to_string(path) {
            let records: Vec<String> = contents.lines().map(str::to_owned).collect();
            if records.len() == count {
                return records;
            }
        }
        thread::sleep(Duration::from_millis(10));
    }
    panic!(
        "fake browser did not record {count} argv records: {:?}",
        fs::read_to_string(path)
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
