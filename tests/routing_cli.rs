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
        .env("XDG_CONFIG_HOME", config_home.path());
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

fn write_config(config_home: &TempDir, executable: &std::path::Path) {
    let config_dir = config_home.path().join("browser-picker");
    fs::create_dir(&config_dir).expect("configuration directory should be writable");
    fs::write(
        config_dir.join("config.toml"),
        format!(
            "version = 1\n\n[[destinations]]\nid = \"controlled\"\nlabel = \"Controlled Browser\"\n\n[destinations.application]\ntype = \"manual\"\nexecutable = \"{}\"\nargs = [\"--new-window\", \"{{target}}\"]\n\n[fallback]\naction = \"open\"\ndestination = \"controlled\"\n",
            executable.display()
        ),
    )
    .expect("configuration should be writable");
}

fn write_raw_config(config_home: &TempDir, source: &str) {
    let config_dir = config_home.path().join("browser-picker");
    fs::create_dir(&config_dir).expect("configuration directory should be writable");
    fs::write(config_dir.join("config.toml"), source).expect("configuration should be writable");
}

fn wait_for_file(path: &std::path::Path) -> String {
    let deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < deadline {
        if let Ok(contents) = fs::read_to_string(path) {
            return contents;
        }
        thread::sleep(Duration::from_millis(10));
    }
    panic!("fake browser did not record its arguments");
}

#[test]
fn valid_https_target_is_dispatched_unchanged_to_the_fallback_destination() {
    let config_home = TempDir::new().expect("temporary configuration home should be created");
    let executable = install_fake_browser(&config_home);
    write_config(&config_home, &executable);
    let received = config_home.path().join("received-argv");
    let target = "https://user:secret@example.com/Path?token=a%2Fb&order=first#Fragment";

    let output = browser_picker(&config_home)
        .env("BROWSER_PICKER_TEST_OUTPUT", &received)
        .arg(target)
        .output()
        .expect("Browser Picker should start");

    assert!(output.status.success());
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
    assert_eq!(
        wait_for_file(&received),
        format!("--new-window\n{target}\n")
    );
}

#[test]
fn valid_http_target_is_dispatched_unchanged_to_the_fallback_destination() {
    let config_home = TempDir::new().expect("temporary configuration home should be created");
    let executable = install_fake_browser(&config_home);
    write_config(&config_home, &executable);
    let received = config_home.path().join("received-argv");
    let target = "http://example.com/path?order=first&order=second";

    let output = browser_picker(&config_home)
        .env("BROWSER_PICKER_TEST_OUTPUT", &received)
        .arg(target)
        .output()
        .expect("Browser Picker should start");

    assert!(output.status.success());
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
    assert_eq!(
        wait_for_file(&received),
        format!("--new-window\n{target}\n")
    );
}

#[test]
fn malformed_url_fails_with_a_target_status() {
    let config_home = TempDir::new().expect("temporary configuration home should be created");

    let output = browser_picker(&config_home)
        .arg("https://")
        .output()
        .expect("Browser Picker should start");

    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert_eq!(
        String::from_utf8(output.stderr).expect("error output should be UTF-8"),
        "Invalid Open Target: expected an absolute HTTP or HTTPS URL\n"
    );
}

#[test]
fn unsupported_url_scheme_fails_without_revealing_url_secrets() {
    let config_home = TempDir::new().expect("temporary configuration home should be created");
    let secret = "query-value-that-must-stay-private";

    let output = browser_picker(&config_home)
        .arg(format!(
            "ftp://user:password@example.com/path?token={secret}"
        ))
        .output()
        .expect("Browser Picker should start");
    let stderr = String::from_utf8(output.stderr).expect("error output should be UTF-8");

    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert_eq!(
        stderr,
        "Unsupported Open Target scheme; expected HTTP or HTTPS\n"
    );
    assert!(!stderr.contains(secret));
    assert!(!stderr.contains("password"));
}

#[test]
fn unsupported_configuration_version_fails_closed() {
    let config_home = TempDir::new().expect("temporary configuration home should be created");
    write_raw_config(
        &config_home,
        "version = 2\n\ndestinations = []\n\n[fallback]\naction = \"open\"\ndestination = \"missing\"\n",
    );

    let output = browser_picker(&config_home)
        .arg("https://example.com/?secret=do-not-print")
        .output()
        .expect("Browser Picker should start");

    assert_eq!(output.status.code(), Some(3));
    assert!(output.stdout.is_empty());
    assert_eq!(
        String::from_utf8(output.stderr).expect("error output should be UTF-8"),
        "Unsupported configuration version 2; expected version 1\n"
    );
}

#[test]
fn destination_id_must_be_a_lowercase_slug() {
    let config_home = TempDir::new().expect("temporary configuration home should be created");
    write_raw_config(
        &config_home,
        "version = 1\n\n[[destinations]]\nid = \"Not Stable\"\nlabel = \"Controlled Browser\"\n\n[destinations.application]\ntype = \"manual\"\nexecutable = \"/bin/true\"\nargs = [\"{target}\"]\n\n[fallback]\naction = \"open\"\ndestination = \"Not Stable\"\n",
    );

    let output = browser_picker(&config_home)
        .arg("https://example.com/")
        .output()
        .expect("Browser Picker should start");

    assert_eq!(output.status.code(), Some(3));
    assert_eq!(
        String::from_utf8(output.stderr).expect("error output should be UTF-8"),
        "Browser Destination ID 'Not Stable' must be a lowercase slug\n"
    );
}

#[test]
fn relative_manual_executable_is_rejected_before_dispatch() {
    let config_home = TempDir::new().expect("temporary configuration home should be created");
    write_raw_config(
        &config_home,
        "version = 1\n\n[[destinations]]\nid = \"controlled\"\nlabel = \"Controlled Browser\"\n\n[destinations.application]\ntype = \"manual\"\nexecutable = \"./browser\"\nargs = [\"{target}\"]\n\n[fallback]\naction = \"open\"\ndestination = \"controlled\"\n",
    );

    let output = browser_picker(&config_home)
        .arg("https://example.com/")
        .output()
        .expect("Browser Picker should start");

    assert_eq!(output.status.code(), Some(3));
    assert_eq!(
        String::from_utf8(output.stderr).expect("error output should be UTF-8"),
        "Manual destination 'controlled' executable must be an absolute path or a PATH-resolved name\n"
    );
}

#[test]
fn path_resolved_manual_executable_receives_the_target() {
    let config_home = TempDir::new().expect("temporary configuration home should be created");
    install_fake_browser(&config_home);
    write_raw_config(
        &config_home,
        "version = 1\n\n[[destinations]]\nid = \"controlled\"\nlabel = \"Controlled Browser\"\n\n[destinations.application]\ntype = \"manual\"\nexecutable = \"controlled-browser\"\nargs = [\"{target}\"]\n\n[fallback]\naction = \"open\"\ndestination = \"controlled\"\n",
    );
    let received = config_home.path().join("received-argv");
    let target = "https://example.com/path";

    let output = browser_picker(&config_home)
        .env("PATH", config_home.path())
        .env("BROWSER_PICKER_TEST_OUTPUT", &received)
        .arg(target)
        .output()
        .expect("Browser Picker should start");

    assert!(output.status.success());
    assert_eq!(wait_for_file(&received), format!("{target}\n"));
}

#[test]
fn manual_destination_requires_exactly_one_target_argument_element() {
    for arguments in [
        "[\"--new-window\"]",
        "[\"{target}\", \"{target}\"]",
        "[\"--url={target}\"]",
    ] {
        let config_home = TempDir::new().expect("temporary configuration home should be created");
        write_raw_config(
            &config_home,
            &format!(
                "version = 1\n\n[[destinations]]\nid = \"controlled\"\nlabel = \"Controlled Browser\"\n\n[destinations.application]\ntype = \"manual\"\nexecutable = \"/missing/browser\"\nargs = {arguments}\n\n[fallback]\naction = \"open\"\ndestination = \"controlled\"\n"
            ),
        );

        let output = browser_picker(&config_home)
            .arg("https://example.com/")
            .output()
            .expect("Browser Picker should start");

        assert_eq!(output.status.code(), Some(3));
        assert_eq!(
            String::from_utf8(output.stderr).expect("error output should be UTF-8"),
            "Manual destination 'controlled' must contain exactly one '{target}' argument\n"
        );
    }
}

#[test]
fn manual_destination_rejects_prohibited_launch_features() {
    for prohibited in [
        "command = \"controlled-browser {target}\"",
        "shell = true",
        "environment = { BROWSER_PROFILE = \"private\" }",
        "condition = \"private\"",
        "working_directory = \"/tmp\"",
    ] {
        let config_home = TempDir::new().expect("temporary configuration home should be created");
        write_raw_config(
            &config_home,
            &format!(
                "version = 1\n\n[[destinations]]\nid = \"controlled\"\nlabel = \"Controlled Browser\"\n\n[destinations.application]\ntype = \"manual\"\nexecutable = \"/missing/browser\"\nargs = [\"{{target}}\"]\n{prohibited}\n\n[fallback]\naction = \"open\"\ndestination = \"controlled\"\n"
            ),
        );

        let output = browser_picker(&config_home)
            .arg("https://example.com/")
            .output()
            .expect("Browser Picker should start");

        assert_eq!(output.status.code(), Some(3));
        assert_eq!(
            String::from_utf8(output.stderr).expect("error output should be UTF-8"),
            "Configuration is not valid versioned TOML\n"
        );
    }
}

#[test]
fn synchronous_spawn_failure_reports_dispatch_was_not_accepted() {
    let config_home = TempDir::new().expect("temporary configuration home should be created");
    write_raw_config(
        &config_home,
        "version = 1\n\n[[destinations]]\nid = \"unavailable\"\nlabel = \"Unavailable Browser\"\n\n[destinations.application]\ntype = \"manual\"\nexecutable = \"/definitely/missing/browser\"\nargs = [\"{target}\"]\n\n[fallback]\naction = \"open\"\ndestination = \"unavailable\"\n",
    );

    let output = browser_picker(&config_home)
        .arg("https://user:password@example.com/?token=do-not-print")
        .output()
        .expect("Browser Picker should start");

    assert_eq!(output.status.code(), Some(4));
    assert!(output.stdout.is_empty());
    assert_eq!(
        String::from_utf8(output.stderr).expect("error output should be UTF-8"),
        "Browser Destination 'unavailable' could not accept dispatch: executable was not found\n"
    );
}

#[test]
fn serialized_open_target_size_is_bounded_at_sixty_four_kibibytes() {
    let config_home = TempDir::new().expect("temporary configuration home should be created");
    let executable = install_fake_browser(&config_home);
    write_config(&config_home, &executable);
    let received = config_home.path().join("received-argv");
    let prefix = "https://example.com/";
    let largest_allowed = format!("{prefix}{}", "a".repeat((64 * 1024) - prefix.len()));

    let accepted = browser_picker(&config_home)
        .env("BROWSER_PICKER_TEST_OUTPUT", &received)
        .arg(&largest_allowed)
        .output()
        .expect("Browser Picker should start");
    assert!(accepted.status.success());
    assert_eq!(
        wait_for_file(&received),
        format!("--new-window\n{largest_allowed}\n")
    );

    let oversized = format!("{largest_allowed}a");
    let rejected = browser_picker(&config_home)
        .arg(oversized)
        .output()
        .expect("Browser Picker should start");

    assert_eq!(rejected.status.code(), Some(2));
    assert!(rejected.stdout.is_empty());
    assert_eq!(
        String::from_utf8(rejected.stderr).expect("error output should be UTF-8"),
        "Open Target exceeds the 65536-byte limit\n"
    );
}

#[test]
fn manual_launch_never_interprets_shell_syntax_or_environment_references() {
    let config_home = TempDir::new().expect("temporary configuration home should be created");
    let executable = install_fake_browser(&config_home);
    write_raw_config(
        &config_home,
        &format!(
            "version = 1\n\n[[destinations]]\nid = \"controlled\"\nlabel = \"Controlled Browser\"\n\n[destinations.application]\ntype = \"manual\"\nexecutable = \"{}\"\nargs = [\"$HOME\", \"&&\", \"{{target}}\"]\n\n[fallback]\naction = \"open\"\ndestination = \"controlled\"\n",
            executable.display()
        ),
    );
    let received = config_home.path().join("received-argv");
    let shell_marker = config_home.path().join("shell-was-run");
    let target = format!(
        "https://example.com/?value=$(touch${{IFS}}{})",
        shell_marker.display()
    );

    let output = browser_picker(&config_home)
        .env("BROWSER_PICKER_TEST_OUTPUT", &received)
        .arg(&target)
        .output()
        .expect("Browser Picker should start");

    assert!(output.status.success());
    assert_eq!(wait_for_file(&received), format!("$HOME\n&&\n{target}\n"));
    assert!(!shell_marker.exists());
}

#[test]
fn every_manual_destination_is_validated_before_routing() {
    let config_home = TempDir::new().expect("temporary configuration home should be created");
    let executable = install_fake_browser(&config_home);
    write_raw_config(
        &config_home,
        &format!(
            "version = 1\n\n[[destinations]]\nid = \"controlled\"\nlabel = \"Controlled Browser\"\n\n[destinations.application]\ntype = \"manual\"\nexecutable = \"{}\"\nargs = [\"{{target}}\"]\n\n[[destinations]]\nid = \"invalid\"\nlabel = \"Invalid Browser\"\n\n[destinations.application]\ntype = \"manual\"\nexecutable = \"./relative-browser\"\nargs = []\n\n[fallback]\naction = \"open\"\ndestination = \"controlled\"\n",
            executable.display()
        ),
    );

    let output = browser_picker(&config_home)
        .arg("https://example.com/")
        .output()
        .expect("Browser Picker should start");

    assert_eq!(output.status.code(), Some(3));
    assert_eq!(
        String::from_utf8(output.stderr).expect("error output should be UTF-8"),
        "Manual destination 'invalid' executable must be an absolute path or a PATH-resolved name\n"
    );
}

#[test]
fn destination_process_output_cannot_reveal_the_open_target() {
    let config_home = TempDir::new().expect("temporary configuration home should be created");
    let executable = config_home.path().join("noisy-browser");
    fs::write(
        &executable,
        "#!/bin/sh\nprintf '%s\\n' \"$1\"\nprintf '%s\\n' \"$1\" >&2\n",
    )
    .expect("fake browser should be writable");
    fs::set_permissions(&executable, fs::Permissions::from_mode(0o700))
        .expect("fake browser should be executable");
    write_config(&config_home, &executable);

    let output = browser_picker(&config_home)
        .arg("https://user:password@example.com/?token=do-not-print")
        .output()
        .expect("Browser Picker should start");

    assert!(output.status.success());
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
}

#[test]
fn routing_requires_a_graphical_session() {
    let config_home = TempDir::new().expect("temporary configuration home should be created");

    let output = browser_picker(&config_home)
        .env_remove("DISPLAY")
        .arg("https://example.com/")
        .output()
        .expect("Browser Picker should start");

    assert_eq!(output.status.code(), Some(5));
    assert!(output.stdout.is_empty());
    assert_eq!(
        String::from_utf8(output.stderr).expect("error output should be UTF-8"),
        "Routing an Open Target requires a graphical session\n"
    );
}

#[test]
fn picker_destination_display_labels_must_be_unique() {
    let config_home = TempDir::new().expect("temporary configuration home should be created");
    write_raw_config(
        &config_home,
        "version = 1\n\n[[destinations]]\nid = \"first\"\nlabel = \"Same Browser\"\n\n[destinations.application]\ntype = \"manual\"\nexecutable = \"/bin/true\"\nargs = [\"{target}\"]\n\n[[destinations]]\nid = \"second\"\nlabel = \"Same Browser\"\n\n[destinations.application]\ntype = \"manual\"\nexecutable = \"/bin/true\"\nargs = [\"{target}\"]\n\n[fallback]\naction = \"show-picker\"\n",
    );

    let output = browser_picker(&config_home)
        .arg("https://example.com/")
        .output()
        .expect("Browser Picker should start");

    assert_eq!(output.status.code(), Some(3));
    assert_eq!(
        String::from_utf8(output.stderr).expect("error output should be UTF-8"),
        "Browser Destination display label 'Same Browser' is not unique\n"
    );
}

#[test]
fn manual_private_arguments_require_one_exact_target_element() {
    let config_home = TempDir::new().expect("temporary configuration home should be created");
    write_raw_config(
        &config_home,
        "version = 1\n\n[[destinations]]\nid = \"controlled\"\nlabel = \"Controlled Browser\"\n\n[destinations.application]\ntype = \"manual\"\nexecutable = \"/bin/true\"\nargs = [\"{target}\"]\nprivate_args = [\"--private={target}\"]\n\n[fallback]\naction = \"open\"\ndestination = \"controlled\"\n",
    );

    let output = browser_picker(&config_home)
        .arg("https://example.com/")
        .output()
        .expect("Browser Picker should start");

    assert_eq!(output.status.code(), Some(3));
    assert_eq!(
        String::from_utf8(output.stderr).expect("error output should be UTF-8"),
        "Manual destination 'controlled' private arguments must contain exactly one '{target}' argument\n"
    );
}

fn install_discovered_browser(config_home: &TempDir) -> (std::path::PathBuf, String) {
    let executable = install_fake_browser(config_home);
    let data_home = config_home.path().join("xdg-data");
    let applications = data_home.join("applications");
    fs::create_dir_all(&applications).expect("application registry should be writable");
    fs::write(
        applications.join("synthetic-firefox.desktop"),
        format!(
            "[Desktop Entry]\nType=Application\nName=Synthetic Firefox\nExec={} %u\nMimeType=x-scheme-handler/http;x-scheme-handler/https;\nTerminal=false\n",
            executable.display()
        ),
    )
    .expect("desktop file should be writable");
    fs::write(
        applications.join("http-only.desktop"),
        format!(
            "[Desktop Entry]\nType=Application\nName=HTTP Only Browser\nExec={} %u\nMimeType=x-scheme-handler/http;\nTerminal=false\n",
            executable.display()
        ),
    )
    .expect("partial handler should be writable");
    fs::write(
        applications.join("mimeinfo.cache"),
        "[MIME Cache]\nx-scheme-handler/http=synthetic-firefox.desktop;http-only.desktop;\nx-scheme-handler/https=synthetic-firefox.desktop;\n",
    )
    .expect("MIME cache should be writable");
    (data_home, "synthetic-firefox.desktop".to_owned())
}

#[test]
fn discovered_destination_launches_through_gappinfo() {
    let config_home = TempDir::new().expect("temporary configuration home should be created");
    let (data_home, desktop_id) = install_discovered_browser(&config_home);
    write_raw_config(
        &config_home,
        &format!(
            "version = 1\n\n[[destinations]]\nid = \"firefox\"\nlabel = \"Synthetic Firefox\"\n\n[destinations.application]\ntype = \"discovered\"\ndesktop_id = \"{desktop_id}\"\n\n[fallback]\naction = \"open\"\ndestination = \"firefox\"\n"
        ),
    );
    let received = config_home.path().join("received-argv");
    let target = "https://user:secret@example.com/discovered";

    let output = browser_picker(&config_home)
        .env("XDG_DATA_HOME", &data_home)
        .env("XDG_DATA_DIRS", &data_home)
        .env("BROWSER_PICKER_TEST_OUTPUT", &received)
        .arg(target)
        .output()
        .expect("Browser Picker should start");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
    assert_eq!(wait_for_file(&received), format!("{target}\n"));
}

#[test]
fn discovered_destination_requires_a_desktop_id() {
    let config_home = TempDir::new().expect("temporary configuration home should be created");
    write_raw_config(
        &config_home,
        "version = 1\n\n[[destinations]]\nid = \"firefox\"\nlabel = \"Synthetic Firefox\"\n\n[destinations.application]\ntype = \"discovered\"\ndesktop_id = \"\"\n\n[fallback]\naction = \"open\"\ndestination = \"firefox\"\n",
    );

    let output = browser_picker(&config_home)
        .arg("https://example.com/")
        .output()
        .expect("Browser Picker should start");

    assert_eq!(output.status.code(), Some(3));
    assert_eq!(
        String::from_utf8(output.stderr).expect("error output should be UTF-8"),
        "Discovered destination 'firefox' must have a desktop application ID\n"
    );
}

#[test]
fn discovered_destination_rejects_parsed_command_fields() {
    let config_home = TempDir::new().expect("temporary configuration home should be created");
    write_raw_config(
        &config_home,
        "version = 1\n\n[[destinations]]\nid = \"firefox\"\nlabel = \"Synthetic Firefox\"\n\n[destinations.application]\ntype = \"discovered\"\ndesktop_id = \"synthetic-firefox.desktop\"\nexec = \"/bin/true {target}\"\n\n[fallback]\naction = \"show-picker\"\n",
    );

    let output = browser_picker(&config_home)
        .arg("https://example.com/")
        .output()
        .expect("Browser Picker should start");

    assert_eq!(output.status.code(), Some(3));
    assert_eq!(
        String::from_utf8(output.stderr).expect("error output should be UTF-8"),
        "Configuration is not valid versioned TOML\n"
    );
}
