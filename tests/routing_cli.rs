use std::fs;
use std::ops::Not;
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
        "Invalid Open Target: expected an existing regular local file or an absolute HTTP or HTTPS URL\n"
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
        "Unsupported Open Target scheme; expected HTTP, HTTPS, or a local file\n"
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
    let stderr = String::from_utf8(output.stderr).expect("error output should be UTF-8");
    assert_eq!(
        stderr,
        "Unsupported configuration version 2; expected version 1\n"
    );
    assert!(!stderr.contains("secret"));
    assert!(!stderr.contains("do-not-print"));
    assert_eq!(
        fs::read_to_string(config_home.path().join("browser-picker/config.toml")).unwrap(),
        "version = 2\n\ndestinations = []\n\n[fallback]\naction = \"open\"\ndestination = \"missing\"\n",
    );
}

#[test]
fn old_schema_without_a_session_is_not_migrated() {
    let config_home = TempDir::new().expect("temporary configuration home should be created");
    let source = include_str!("fixtures/schema/v0.toml");
    write_raw_config(&config_home, source);

    let output = browser_picker(&config_home)
        .arg("https://user:secret@example.com/?q=token")
        .output()
        .expect("Browser Picker should start");

    assert_eq!(output.status.code(), Some(3));
    let stderr = String::from_utf8(output.stderr).expect("error output should be UTF-8");
    assert_eq!(
        stderr,
        "Configuration schema version 0 requires a confirmed migration to version 1\n"
    );
    assert!(!stderr.contains("secret"));
    assert!(!stderr.contains("token"));
    assert!(!stderr.contains("example.com"));
    assert_eq!(
        fs::read_to_string(config_home.path().join("browser-picker/config.toml")).unwrap(),
        source
    );
}

#[test]
fn invalid_toml_fails_closed_without_using_a_subset() {
    let config_home = TempDir::new().expect("temporary configuration home should be created");
    let source = "version = 1\nthis is not [[toml\n";
    write_raw_config(&config_home, source);

    let output = browser_picker(&config_home)
        .arg("https://example.com/?secret=do-not-print")
        .output()
        .expect("Browser Picker should start");

    assert_eq!(output.status.code(), Some(3));
    let stderr = String::from_utf8(output.stderr).expect("error output should be UTF-8");
    assert_eq!(stderr, "Configuration is not valid versioned TOML\n");
    assert!(!stderr.contains("secret"));
    assert!(!stderr.contains("do-not-print"));
    assert_eq!(
        fs::read_to_string(config_home.path().join("browser-picker/config.toml")).unwrap(),
        source
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
        let stderr = String::from_utf8(output.stderr).expect("error output should be UTF-8");
        assert!(
            stderr.starts_with("Unknown configuration key '"),
            "{stderr}"
        );
        assert!(stderr.contains(" at line "), "{stderr}");
        assert!(!stderr.contains("controlled-browser"));
        assert!(!stderr.contains("/tmp"));
        let saved =
            fs::read_to_string(config_home.path().join("browser-picker/config.toml")).unwrap();
        assert!(!saved.contains("https://example.com/"));
        assert_eq!(output.stdout.len(), 0);
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
    let stderr = String::from_utf8(output.stderr).expect("error output should be UTF-8");
    assert_eq!(stderr, "Unknown configuration key 'exec' at line 7\n");
    assert!(!stderr.contains("/bin/true"));
}

#[test]
fn routing_rule_matches_idna_host_using_the_matching_url() {
    let config_home = TempDir::new().expect("temporary configuration home should be created");
    let executable = install_fake_browser(&config_home);
    write_raw_config(
        &config_home,
        &format!(
            "version = 1\n\n[[destinations]]\nid = \"controlled\"\nlabel = \"Controlled Browser\"\n\n[destinations.application]\ntype = \"manual\"\nexecutable = \"{}\"\nargs = [\"--rule\", \"{{target}}\"]\n\n[[rules]]\nid = \"international-host\"\nname = \"International host\"\nenabled = true\n\n[rules.action]\ntype = \"open\"\ndestination = \"controlled\"\nmode = \"normal\"\n\n[[rules.groups]]\n\n[[rules.groups.conditions]]\ntype = \"host\"\nvalue = \"xn--bcher-kva.example\"\ninclude_subdomains = false\n\n[fallback]\naction = \"show-picker\"\n",
            executable.display()
        ),
    );
    let received = config_home.path().join("received-argv");
    let target = "HTTPS://BÜCHER.example:443/Path?value=a%2Fb#Fragment";

    let output = browser_picker(&config_home)
        .env("BROWSER_PICKER_TEST_OUTPUT", &received)
        .arg(target)
        .output()
        .expect("Browser Picker should start");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(wait_for_file(&received), format!("--rule\n{target}\n"));
}

#[test]
fn structured_conditions_preserve_encodings_and_match_repeated_query_values() {
    let config_home = TempDir::new().expect("temporary configuration home should be created");
    let executable = install_fake_browser(&config_home);
    write_raw_config(
        &config_home,
        &format!(
            "version = 1\n\n[[destinations]]\nid = \"matched\"\nlabel = \"Matched Browser\"\n\n[destinations.application]\ntype = \"manual\"\nexecutable = \"{}\"\nargs = [\"--matched\", \"{{target}}\"]\n\n[[destinations]]\nid = \"fallback\"\nlabel = \"Fallback Browser\"\n\n[destinations.application]\ntype = \"manual\"\nexecutable = \"{}\"\nargs = [\"--fallback\", \"{{target}}\"]\n\n[[rules]]\nid = \"structured\"\nname = \"Structured conditions\"\nenabled = true\n\n[rules.action]\ntype = \"open\"\ndestination = \"matched\"\n\n[[rules.groups]]\n\n[[rules.groups.conditions]]\ntype = \"scheme\"\nvalue = \"HTTPS\"\n\n[[rules.groups.conditions]]\ntype = \"host\"\nvalue = \"example.com\"\ninclude_subdomains = true\n\n[[rules.groups.conditions]]\ntype = \"path\"\nvalue = \"/Encoded%2F\"\ncomparison = \"prefix\"\n\n[[rules.groups.conditions]]\ntype = \"query-key\"\nkey = \"flag\"\n\n[[rules.groups.conditions]]\ntype = \"query-value\"\nkey = \"value\"\nvalue = \"A+B\"\n\n[[rules.groups.conditions]]\ntype = \"query-value\"\nkey = \"blocked\"\nvalue = \"yes\"\nnegate = true\n\n[fallback]\naction = \"open\"\ndestination = \"fallback\"\n",
            executable.display(),
            executable.display()
        ),
    );
    let received = config_home.path().join("received-argv");
    let target = "https://Sub.Example.com/Encoded%2FSegment?value=no&flag&value=A+B&order=last";

    let output = browser_picker(&config_home)
        .env("BROWSER_PICKER_TEST_OUTPUT", &received)
        .arg(target)
        .output()
        .expect("Browser Picker should start");

    assert!(output.status.success());
    assert_eq!(wait_for_file(&received), format!("--matched\n{target}\n"));

    fs::remove_file(&received).expect("first dispatch record should be removable");
    let boundary_target = "https://notexample.com/Encoded%2FSegment?flag&value=A+B";
    let output = browser_picker(&config_home)
        .env("BROWSER_PICKER_TEST_OUTPUT", &received)
        .arg(boundary_target)
        .output()
        .expect("Browser Picker should start");
    assert!(output.status.success());
    assert_eq!(
        wait_for_file(&received),
        format!("--fallback\n{boundary_target}\n")
    );
}

#[test]
fn default_ports_are_absent_and_first_enabled_matching_rule_wins() {
    let config_home = TempDir::new().expect("temporary configuration home should be created");
    let executable = install_fake_browser(&config_home);
    write_raw_config(
        &config_home,
        &format!(
            "version = 1\n\n[[destinations]]\nid = \"first\"\nlabel = \"First Browser\"\n\n[destinations.application]\ntype = \"manual\"\nexecutable = \"{}\"\nargs = [\"--first\", \"{{target}}\"]\n\n[[destinations]]\nid = \"second\"\nlabel = \"Second Browser\"\n\n[destinations.application]\ntype = \"manual\"\nexecutable = \"{}\"\nargs = [\"--second\", \"{{target}}\"]\n\n[[rules]]\nid = \"disabled\"\nname = \"Disabled first\"\nenabled = false\n\n[rules.action]\ntype = \"open\"\ndestination = \"second\"\n\n[[rules.groups]]\n\n[[rules.groups.conditions]]\ntype = \"host\"\nvalue = \"example.com\"\n\n[[rules]]\nid = \"first-winner\"\nname = \"First winner\"\nenabled = true\n\n[rules.action]\ntype = \"open\"\ndestination = \"first\"\n\n[[rules.groups]]\n\n[[rules.groups.conditions]]\ntype = \"host\"\nvalue = \"example.com\"\n\n[[rules]]\nid = \"later-match\"\nname = \"Later match\"\nenabled = true\n\n[rules.action]\ntype = \"open\"\ndestination = \"second\"\n\n[[rules.groups]]\n\n[[rules.groups.conditions]]\ntype = \"host\"\nvalue = \"example.com\"\n\n[fallback]\naction = \"open\"\ndestination = \"second\"\n",
            executable.display(),
            executable.display()
        ),
    );
    let received = config_home.path().join("received-argv");
    let target = "https://example.com:443/";

    let output = browser_picker(&config_home)
        .env("BROWSER_PICKER_TEST_OUTPUT", &received)
        .arg(target)
        .output()
        .expect("Browser Picker should start");

    assert!(output.status.success());
    assert_eq!(wait_for_file(&received), format!("--first\n{target}\n"));
}

#[test]
fn or_groups_use_normalized_ports_and_explicit_case_options() {
    let config_home = TempDir::new().expect("temporary configuration home should be created");
    let executable = install_fake_browser(&config_home);
    write_raw_config(
        &config_home,
        &format!(
            "version = 1\n\n[[destinations]]\nid = \"wrong\"\nlabel = \"Wrong Browser\"\n\n[destinations.application]\ntype = \"manual\"\nexecutable = \"{}\"\nargs = [\"--wrong\", \"{{target}}\"]\n\n[[destinations]]\nid = \"controlled\"\nlabel = \"Controlled Browser\"\n\n[destinations.application]\ntype = \"manual\"\nexecutable = \"{}\"\nargs = [\"--matched\", \"{{target}}\"]\n\n[[rules]]\nid = \"explicit-default-port\"\nname = \"Explicit default port\"\nenabled = true\n\n[rules.action]\ntype = \"open\"\ndestination = \"wrong\"\n\n[[rules.groups]]\n\n[[rules.groups.conditions]]\ntype = \"port\"\nvalue = 443\n\n[[rules]]\nid = \"or-groups\"\nname = \"OR groups\"\nenabled = true\n\n[rules.action]\ntype = \"open\"\ndestination = \"controlled\"\n\n[[rules.groups]]\n\n[[rules.groups.conditions]]\ntype = \"path\"\nvalue = \"/never\"\ncomparison = \"exact\"\n\n[[rules.groups]]\n\n[[rules.groups.conditions]]\ntype = \"path\"\nvalue = \"/case\"\ncomparison = \"exact\"\ncase_insensitive = true\n\n[[rules.groups.conditions]]\ntype = \"query-value\"\nkey = \"Token\"\nvalue = \"VALUE\"\ncase_insensitive = true\n\n[fallback]\naction = \"show-picker\"\n",
            executable.display(),
            executable.display()
        ),
    );
    let received = config_home.path().join("received-argv");
    let target = "https://example.com:443/CASE?token=value";

    let output = browser_picker(&config_home)
        .env("BROWSER_PICKER_TEST_OUTPUT", &received)
        .arg(target)
        .output()
        .expect("Browser Picker should start");

    assert!(output.status.success());
    assert_eq!(wait_for_file(&received), format!("--matched\n{target}\n"));
}

#[test]
fn preselection_does_not_dispatch_automatically() {
    let config_home = TempDir::new().expect("temporary configuration home should be created");
    let executable = install_fake_browser(&config_home);
    write_raw_config(
        &config_home,
        &format!(
            "version = 1\n\n[[destinations]]\nid = \"controlled\"\nlabel = \"Controlled Browser\"\n\n[destinations.application]\ntype = \"manual\"\nexecutable = \"{}\"\nargs = [\"--preselected\", \"{{target}}\"]\nprivate_args = [\"--private\", \"{{target}}\"]\n\n[[rules]]\nid = \"suggest\"\nname = \"Suggest controlled\"\nenabled = true\n\n[rules.action]\ntype = \"preselect\"\ndestination = \"controlled\"\nmode = \"private\"\n\n[[rules.groups]]\n\n[[rules.groups.conditions]]\ntype = \"host\"\nvalue = \"suggested.example\"\n\n[fallback]\naction = \"open\"\ndestination = \"controlled\"\n",
            executable.display()
        ),
    );
    let received = config_home.path().join("received-argv");
    let output = browser_picker(&config_home)
        .env("BROWSER_PICKER_TEST_OUTPUT", &received)
        .arg("https://suggested.example/path")
        .output()
        .expect("Browser Picker should start");
    assert_eq!(output.status.code(), Some(5));
    assert_eq!(
        String::from_utf8(output.stderr).expect("error output should be UTF-8"),
        "Picker, configuration, and shared queue operations require a user session D-Bus\n"
    );
    assert!(
        received.exists() == false,
        "Preselection must queue instead of dispatching"
    );
}

#[test]
fn ipv6_literal_host_matches_without_double_brackets() {
    let config_home = TempDir::new().expect("temporary configuration home should be created");
    let executable = install_fake_browser(&config_home);
    write_raw_config(
        &config_home,
        &format!(
            "version = 1\n\n[[destinations]]\nid = \"controlled\"\nlabel = \"Controlled Browser\"\n\n[destinations.application]\ntype = \"manual\"\nexecutable = \"{}\"\nargs = [\"--ipv6\", \"{{target}}\"]\n\n[[rules]]\nid = \"loopback\"\nname = \"IPv6 loopback\"\nenabled = true\n\n[rules.action]\ntype = \"open\"\ndestination = \"controlled\"\n\n[[rules.groups]]\n\n[[rules.groups.conditions]]\ntype = \"host\"\nvalue = \"::1\"\n\n[fallback]\naction = \"show-picker\"\n",
            executable.display()
        ),
    );
    let received = config_home.path().join("received-argv");
    let target = "http://[::1]/Path";

    let output = browser_picker(&config_home)
        .env("BROWSER_PICKER_TEST_OUTPUT", &received)
        .arg(target)
        .output()
        .expect("Browser Picker should start");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(wait_for_file(&received), format!("--ipv6\n{target}\n"));
}

fn pattern_rule_config(executable: &std::path::Path, condition: &str, action: &str) -> String {
    let mode = if action == "preselect" {
        "private"
    } else {
        "normal"
    };
    format!(
        "version = 1\n\n[[destinations]]\nid = \"controlled\"\nlabel = \"Controlled Browser\"\n\n[destinations.application]\ntype = \"manual\"\nexecutable = \"{}\"\nargs = [\"--matched\", \"{{target}}\"]\nprivate_args = [\"--private\", \"{{target}}\"]\n\n[[destinations]]\nid = \"fallback\"\nlabel = \"Fallback Browser\"\n\n[destinations.application]\ntype = \"manual\"\nexecutable = \"{}\"\nargs = [\"--fallback\", \"{{target}}\"]\n\n[[rules]]\nid = \"pattern\"\nname = \"Pattern\"\nenabled = true\n\n[rules.action]\ntype = \"{}\"\ndestination = \"controlled\"\nmode = \"{}\"\n\n[[rules.groups]]\n\n{}\n\n[fallback]\naction = \"open\"\ndestination = \"fallback\"\n",
        executable.display(),
        executable.display(),
        action,
        mode,
        condition
    )
}

#[test]
fn glob_matches_the_entire_matching_url_with_wildcards_and_escaping() {
    let config_home = TempDir::new().expect("temporary configuration home should be created");
    let executable = install_fake_browser(&config_home);
    write_raw_config(
        &config_home,
        &pattern_rule_config(
            &executable,
            "[[rules.groups.conditions]]\ntype = \"glob\"\nvalue = \"https://*.example.com/\\\\?/a*\"\n",
            "open",
        ),
    );
    let received = config_home.path().join("received-argv");
    let target = "https://news.example.com/?/a/b?q=1";

    let output = browser_picker(&config_home)
        .env("BROWSER_PICKER_TEST_OUTPUT", &received)
        .arg(target)
        .output()
        .expect("Browser Picker should start");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(wait_for_file(&received), format!("--matched\n{target}\n"));
}

#[test]
fn glob_requires_explicit_wildcards_for_substring_matches() {
    let config_home = TempDir::new().expect("temporary configuration home should be created");
    let executable = install_fake_browser(&config_home);
    write_raw_config(
        &config_home,
        &pattern_rule_config(
            &executable,
            "[[rules.groups.conditions]]\ntype = \"glob\"\nvalue = \"example.com\"\n",
            "open",
        ),
    );
    let received = config_home.path().join("received-argv");

    let output = browser_picker(&config_home)
        .env("BROWSER_PICKER_TEST_OUTPUT", &received)
        .arg("https://example.com/path")
        .output()
        .expect("Browser Picker should start");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        wait_for_file(&received),
        "--fallback\nhttps://example.com/path\n"
    );
}

#[test]
fn glob_can_ignore_path_case_when_requested() {
    let config_home = TempDir::new().expect("temporary configuration home should be created");
    let executable = install_fake_browser(&config_home);
    write_raw_config(
        &config_home,
        &pattern_rule_config(
            &executable,
            "[[rules.groups.conditions]]\ntype = \"glob\"\nvalue = \"https://example.com/docs*\"\ncase_insensitive = true\n",
            "open",
        ),
    );
    let received = config_home.path().join("received-argv");
    let target = "https://example.com/DOCS/Guide";

    let output = browser_picker(&config_home)
        .env("BROWSER_PICKER_TEST_OUTPUT", &received)
        .arg(target)
        .output()
        .expect("Browser Picker should start");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(wait_for_file(&received), format!("--matched\n{target}\n"));
}

#[test]
fn glob_matches_ascii_matching_url_instead_of_unicode_host_spelling() {
    let config_home = TempDir::new().expect("temporary configuration home should be created");
    let executable = install_fake_browser(&config_home);
    write_raw_config(
        &config_home,
        &pattern_rule_config(
            &executable,
            "[[rules.groups.conditions]]\ntype = \"glob\"\nvalue = \"https://*.xn--bcher-kva.example/*\"\n",
            "open",
        ),
    );
    let received = config_home.path().join("received-argv");
    let target = "https://review.bücher.example/Path";

    let output = browser_picker(&config_home)
        .env("BROWSER_PICKER_TEST_OUTPUT", &received)
        .arg(target)
        .output()
        .expect("Browser Picker should start");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(wait_for_file(&received), format!("--matched\n{target}\n"));
}

#[test]
fn regex_matches_the_entire_matching_url() {
    let config_home = TempDir::new().expect("temporary configuration home should be created");
    let executable = install_fake_browser(&config_home);
    write_raw_config(
        &config_home,
        &pattern_rule_config(
            &executable,
            "[[rules.groups.conditions]]\ntype = \"regex\"\nvalue = \"https://docs\\\\.example\\\\.com/.*\"\n",
            "open",
        ),
    );
    let received = config_home.path().join("received-argv");
    let target = "https://docs.example.com/guide#top";

    let output = browser_picker(&config_home)
        .env("BROWSER_PICKER_TEST_OUTPUT", &received)
        .arg(target)
        .output()
        .expect("Browser Picker should start");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(wait_for_file(&received), format!("--matched\n{target}\n"));
}

#[test]
fn regex_does_not_substring_match_without_wildcards() {
    let config_home = TempDir::new().expect("temporary configuration home should be created");
    let executable = install_fake_browser(&config_home);
    write_raw_config(
        &config_home,
        &pattern_rule_config(
            &executable,
            "[[rules.groups.conditions]]\ntype = \"regex\"\nvalue = \"docs\\\\.example\\\\.com\"\n",
            "open",
        ),
    );
    let received = config_home.path().join("received-argv");

    let output = browser_picker(&config_home)
        .env("BROWSER_PICKER_TEST_OUTPUT", &received)
        .arg("https://docs.example.com/guide")
        .output()
        .expect("Browser Picker should start");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        wait_for_file(&received),
        "--fallback\nhttps://docs.example.com/guide\n"
    );
}

#[test]
fn negated_regex_in_or_group_can_preselect_without_dispatch() {
    let config_home = TempDir::new().expect("temporary configuration home should be created");
    let executable = install_fake_browser(&config_home);
    write_raw_config(
        &config_home,
        &pattern_rule_config(
            &executable,
            "[[rules.groups.conditions]]\ntype = \"regex\"\nvalue = \"https://ads\\\\.example/.*\"\nnegate = true\n\n[[rules.groups.conditions]]\ntype = \"glob\"\nvalue = \"https://*.example/*\"\n",
            "preselect",
        ),
    );
    let received = config_home.path().join("received-argv");
    let output = browser_picker(&config_home)
        .env("BROWSER_PICKER_TEST_OUTPUT", &received)
        .arg("https://news.example/story")
        .output()
        .expect("Browser Picker should start");
    assert_eq!(output.status.code(), Some(5));
    assert!(
        received.exists() == false,
        "Preselection must queue instead of dispatching"
    );
}

#[test]
fn empty_patterns_are_rejected_before_routing() {
    for (kind, label) in [("glob", "glob"), ("regex", "regular expression")] {
        let config_home = TempDir::new().expect("temporary configuration home should be created");
        write_raw_config(
            &config_home,
            &format!(
                "version = 1\n\n[[destinations]]\nid = \"controlled\"\nlabel = \"Controlled Browser\"\n\n[destinations.application]\ntype = \"manual\"\nexecutable = \"/bin/true\"\nargs = [\"{{target}}\"]\n\n[[rules]]\nid = \"pattern\"\nname = \"Pattern\"\nenabled = true\n\n[rules.action]\ntype = \"open\"\ndestination = \"controlled\"\n\n[[rules.groups]]\n\n[[rules.groups.conditions]]\ntype = \"{kind}\"\nvalue = \"\"\n\n[fallback]\naction = \"show-picker\"\n"
            ),
        );

        let output = browser_picker(&config_home)
            .arg("https://example.com/")
            .output()
            .expect("Browser Picker should start");

        assert_eq!(output.status.code(), Some(3), "{kind}");
        let stderr = String::from_utf8(output.stderr).expect("error output should be UTF-8");
        assert!(
            stderr.contains(&format!("Routing Rule 'pattern' {label} is invalid")),
            "{kind}: {stderr}"
        );
        assert!(stderr.contains("must not be empty"), "{kind}: {stderr}");
        assert!(!stderr.contains("https://example.com"), "{kind}: {stderr}");
    }
}

#[test]
fn oversized_patterns_are_rejected_before_routing() {
    let pattern = "a".repeat(65_537);
    for (kind, label) in [("glob", "glob"), ("regex", "regular expression")] {
        let config_home = TempDir::new().expect("temporary configuration home should be created");
        write_raw_config(
            &config_home,
            &format!(
                "version = 1\n\n[[destinations]]\nid = \"controlled\"\nlabel = \"Controlled Browser\"\n\n[destinations.application]\ntype = \"manual\"\nexecutable = \"/bin/true\"\nargs = [\"{{target}}\"]\n\n[[rules]]\nid = \"pattern\"\nname = \"Pattern\"\nenabled = true\n\n[rules.action]\ntype = \"open\"\ndestination = \"controlled\"\n\n[[rules.groups]]\n\n[[rules.groups.conditions]]\ntype = \"{kind}\"\nvalue = \"{pattern}\"\n\n[fallback]\naction = \"show-picker\"\n"
            ),
        );

        let output = browser_picker(&config_home)
            .arg("https://example.com/")
            .output()
            .expect("Browser Picker should start");

        assert_eq!(output.status.code(), Some(3), "{kind}");
        let stderr = String::from_utf8(output.stderr).expect("error output should be UTF-8");
        assert!(
            stderr.contains(&format!("Routing Rule 'pattern' {label} is invalid")),
            "{kind}: {stderr}"
        );
        assert!(
            stderr.contains("exceeds the 65536-byte limit"),
            "{kind}: {stderr}"
        );
        assert!(!stderr.contains("https://example.com"), "{kind}: {stderr}");
    }
}

#[test]
fn trailing_backslash_glob_is_rejected_before_routing() {
    let config_home = TempDir::new().expect("temporary configuration home should be created");
    write_raw_config(
        &config_home,
        "version = 1\n\n[[destinations]]\nid = \"controlled\"\nlabel = \"Controlled Browser\"\n\n[destinations.application]\ntype = \"manual\"\nexecutable = \"/bin/true\"\nargs = [\"{target}\"]\n\n[[rules]]\nid = \"pattern\"\nname = \"Pattern\"\nenabled = true\n\n[rules.action]\ntype = \"open\"\ndestination = \"controlled\"\n\n[[rules.groups]]\n\n[[rules.groups.conditions]]\ntype = \"glob\"\nvalue = \"https://example.com/\\\\\"\n\n[fallback]\naction = \"show-picker\"\n",
    );

    let output = browser_picker(&config_home)
        .arg("https://example.com/")
        .output()
        .expect("Browser Picker should start");

    assert_eq!(output.status.code(), Some(3));
    let stderr = String::from_utf8(output.stderr).expect("error output should be UTF-8");
    assert!(stderr.contains("Routing Rule 'pattern' glob is invalid"));
    assert!(stderr.contains("trailing backslash"));
}

#[test]
fn regex_rejects_backreferences_and_look_around() {
    for source in [r#"value = "(a)\\1""#, r#"value = "(?=https)""#] {
        let config_home = TempDir::new().expect("temporary configuration home should be created");
        write_raw_config(
            &config_home,
            &format!(
                "version = 1\n\n[[destinations]]\nid = \"controlled\"\nlabel = \"Controlled Browser\"\n\n[destinations.application]\ntype = \"manual\"\nexecutable = \"/bin/true\"\nargs = [\"{{target}}\"]\n\n[[rules]]\nid = \"pattern\"\nname = \"Pattern\"\nenabled = true\n\n[rules.action]\ntype = \"open\"\ndestination = \"controlled\"\n\n[[rules.groups]]\n\n[[rules.groups.conditions]]\ntype = \"regex\"\n{source}\n\n[fallback]\naction = \"show-picker\"\n"
            ),
        );

        let output = browser_picker(&config_home)
            .arg("https://example.com/")
            .output()
            .expect("Browser Picker should start");

        let stderr = String::from_utf8(output.stderr).expect("error output should be UTF-8");
        assert_eq!(
            output.status.code(),
            Some(3),
            "source={source} stderr={stderr}"
        );
        assert!(
            stderr.contains("Routing Rule 'pattern' regular expression is invalid"),
            "{stderr}"
        );
        assert!(!stderr.contains("https://example.com"), "{stderr}");
    }
}

#[test]
fn hostile_regex_still_routes_in_bounded_time() {
    let config_home = TempDir::new().expect("temporary configuration home should be created");
    let executable = install_fake_browser(&config_home);
    let nested = "a?".repeat(40) + &"a".repeat(40);
    write_raw_config(
        &config_home,
        &pattern_rule_config(
            &executable,
            &format!("[[rules.groups.conditions]]\ntype = \"regex\"\nvalue = \"{nested}\"\n"),
            "open",
        ),
    );
    let received = config_home.path().join("received-argv");
    let target = format!("https://example.com/{}", "a".repeat(40));

    let output = browser_picker(&config_home)
        .env("BROWSER_PICKER_TEST_OUTPUT", &received)
        .arg(&target)
        .output()
        .expect("Browser Picker should start");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(wait_for_file(&received), format!("--fallback\n{target}\n"));
}

fn write_html_file(directory: &std::path::Path, name: &str) -> std::path::PathBuf {
    fs::create_dir_all(directory).expect("file fixture directory should be writable");
    let path = directory.join(name);
    fs::write(&path, "<html></html>").expect("HTML fixture should be writable");
    path
}

fn assert_queued_without_dispatch(config_home: &TempDir, argument: &std::ffi::OsStr) {
    let received = config_home.path().join("received-argv");
    let output = browser_picker(config_home)
        .env("BROWSER_PICKER_TEST_OUTPUT", &received)
        .arg(argument)
        .output()
        .expect("Browser Picker should start");
    assert_eq!(output.status.code(), Some(5));
    assert_eq!(
        String::from_utf8(output.stderr).expect("error output should be UTF-8"),
        "Picker, configuration, and shared queue operations require a user session D-Bus\n"
    );
    assert!(
        received.exists() == false,
        "local files must not dispatch automatically"
    );
}

#[test]
fn local_file_path_requires_the_picker_even_with_automatic_fallback() {
    let config_home = TempDir::new().expect("temporary configuration home should be created");
    let executable = install_fake_browser(&config_home);
    write_config(&config_home, &executable);
    let path = write_html_file(config_home.path(), "page.html");
    assert_queued_without_dispatch(&config_home, path.as_os_str());
}

#[test]
fn local_file_uri_and_localhost_alias_require_the_picker() {
    let config_home = TempDir::new().expect("temporary configuration home should be created");
    let executable = install_fake_browser(&config_home);
    write_config(&config_home, &executable);
    let path = write_html_file(config_home.path(), "uri.html");
    let uri = format!("file://{}", path.display());
    let localhost = format!("file://localhost{}", path.display());
    assert_queued_without_dispatch(&config_home, std::ffi::OsStr::new(&uri));
    assert_queued_without_dispatch(&config_home, std::ffi::OsStr::new(&localhost));
}

#[test]
fn relative_and_dot_segments_are_normalized_without_following_symlink_parents() {
    let config_home = TempDir::new().expect("temporary configuration home should be created");
    let executable = install_fake_browser(&config_home);
    write_config(&config_home, &executable);
    let jail = config_home.path().join("jail");
    let visible = write_html_file(&jail.join("visible"), "doc.html");
    let outside = write_html_file(&config_home.path().join("outside"), "secret.html");
    std::os::unix::fs::symlink(&outside.parent().unwrap(), jail.join("link"))
        .expect("symlink fixture should be created");

    let nested = jail.join("visible").join("nested");
    fs::create_dir(&nested).expect("nested directory should be writable");
    let received = config_home.path().join("received-argv");
    let output = browser_picker(&config_home)
        .current_dir(&nested)
        .env("BROWSER_PICKER_TEST_OUTPUT", &received)
        .arg("./../doc.html")
        .output()
        .expect("Browser Picker should start");
    assert_ne!(
        output.status.code(),
        Some(2),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(received.exists() == false);
    let _ = visible;

    let output = browser_picker(&config_home)
        .arg(jail.join("link").join("..").join("secret.html"))
        .output()
        .expect("Browser Picker should start");
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).expect("error output should be UTF-8");
    assert_eq!(
        stderr,
        "Open Target must be an existing regular local file\n"
    );
    assert!(!stderr.contains("secret.html"));
}

#[test]
fn symlink_to_a_regular_file_is_accepted_and_directory_symlink_is_rejected() {
    let config_home = TempDir::new().expect("temporary configuration home should be created");
    let executable = install_fake_browser(&config_home);
    write_config(&config_home, &executable);
    let path = write_html_file(config_home.path(), "target.html");
    let file_link = config_home.path().join("alias.html");
    std::os::unix::fs::symlink(&path, &file_link).expect("file symlink should be created");
    assert_queued_without_dispatch(&config_home, file_link.as_os_str());

    let dir_link = config_home.path().join("dir-alias");
    std::os::unix::fs::symlink(config_home.path(), &dir_link)
        .expect("directory symlink should be created");
    let output = browser_picker(&config_home)
        .arg(&dir_link)
        .output()
        .expect("Browser Picker should start");
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).expect("error output should be UTF-8");
    assert_eq!(
        stderr,
        "Open Target must be an existing regular local file\n"
    );
}

#[test]
fn directories_missing_paths_fifos_and_remote_file_uris_are_rejected() {
    let config_home = TempDir::new().expect("temporary configuration home should be created");
    let executable = install_fake_browser(&config_home);
    write_config(&config_home, &executable);
    let fifo = config_home.path().join("pipe");
    assert!(
        Command::new("mkfifo")
            .arg(&fifo)
            .status()
            .expect("mkfifo should run")
            .success()
    );

    for argument in [
        config_home.path().as_os_str().to_os_string(),
        config_home.path().join("missing.html").into_os_string(),
        fifo.into_os_string(),
        std::ffi::OsString::from("file://example.com/tmp/page.html"),
    ] {
        let output = browser_picker(&config_home)
            .arg(&argument)
            .output()
            .expect("Browser Picker should start");
        assert_eq!(output.status.code(), Some(2), "{argument:?}");
        let stderr = String::from_utf8(output.stderr).expect("error output should be UTF-8");
        assert!(!stderr.contains("page.html"), "{stderr}");
        assert!(!stderr.contains("missing.html"), "{stderr}");
        if argument.to_string_lossy().starts_with("file://example.com") {
            assert_eq!(
                stderr,
                "Unsupported Open Target: remote file URIs are not accepted\n"
            );
        } else {
            assert_eq!(
                stderr,
                "Open Target must be an existing regular local file\n"
            );
        }
    }
}

fn install_family_desktop(
    config_home: &TempDir,
    desktop_id: &str,
    name: &str,
    executable: &std::path::Path,
) -> std::path::PathBuf {
    let data_home = config_home.path().join("xdg-data");
    let applications = data_home.join("applications");
    fs::create_dir_all(&applications).expect("application registry should be writable");
    fs::write(
        applications.join(desktop_id),
        format!(
            "[Desktop Entry]\nType=Application\nName={name}\nExec={} %u\nMimeType=x-scheme-handler/http;x-scheme-handler/https;\nTerminal=false\n",
            executable.display()
        ),
    )
    .expect("desktop file should be writable");
    fs::write(
        applications.join("mimeinfo.cache"),
        format!(
            "[MIME Cache]\nx-scheme-handler/http={desktop_id};\nx-scheme-handler/https={desktop_id};\n"
        ),
    )
    .expect("MIME cache should be writable");
    data_home
}

#[test]
fn firefox_profile_destination_launches_with_profile_path_and_private_flag() {
    let config_home = TempDir::new().expect("temporary configuration home should be created");
    let executable = install_fake_browser(&config_home);
    let data_home = install_family_desktop(&config_home, "firefox.desktop", "Firefox", &executable);
    let profile = config_home.path().join("work-profile");
    fs::create_dir(&profile).expect("Firefox profile should exist");
    write_raw_config(
        &config_home,
        &format!(
            "version = 1\n\n[[destinations]]\nid = \"firefox-work\"\nlabel = \"Work Firefox\"\nprofile_label = \"Work\"\n\n[destinations.application]\ntype = \"firefox-profile\"\ndesktop_id = \"firefox.desktop\"\nname = \"Work\"\npath = \"{}\"\n\n[[rules]]\nid = \"private-work\"\nname = \"Private work\"\nenabled = true\n\n[rules.action]\ntype = \"open\"\ndestination = \"firefox-work\"\nmode = \"private\"\n\n[[rules.groups]]\n\n[[rules.groups.conditions]]\ntype = \"host\"\nvalue = \"work.example\"\n\n[fallback]\naction = \"show-picker\"\n",
            profile.display()
        ),
    );
    let received = config_home.path().join("received-argv");
    let target = "https://work.example/mail";

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
    assert_eq!(
        wait_for_file(&received),
        format!(
            "--profile\n{}\n--private-window\n{target}\n",
            profile.display()
        )
    );
}

#[test]
fn chromium_profile_destination_launches_with_user_data_root_and_profile_id() {
    let config_home = TempDir::new().expect("temporary configuration home should be created");
    let executable = install_fake_browser(&config_home);
    let data_home =
        install_family_desktop(&config_home, "chromium.desktop", "Chromium", &executable);
    let user_data = config_home.path().join("chromium-data");
    fs::create_dir_all(user_data.join("Profile 1")).expect("Chromium profile should exist");
    write_raw_config(
        &config_home,
        &format!(
            "version = 1\n\n[[destinations]]\nid = \"chromium-work\"\nlabel = \"Work Chromium\"\nprofile_label = \"Work\"\n\n[destinations.application]\ntype = \"chromium-profile\"\ndesktop_id = \"chromium.desktop\"\nuser_data_dir = \"{}\"\nprofile_directory = \"Profile 1\"\n\n[fallback]\naction = \"open\"\ndestination = \"chromium-work\"\n",
            user_data.display()
        ),
    );
    let received = config_home.path().join("received-argv");
    let target = "https://example.com/chromium";

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
    assert_eq!(
        wait_for_file(&received),
        format!(
            "--user-data-dir\n{}\n--profile-directory\nProfile 1\n{target}\n",
            user_data.display()
        )
    );
}

#[test]
fn lost_private_capability_keeps_configuration_and_does_not_downgrade() {
    let config_home = TempDir::new().expect("temporary configuration home should be created");
    let snap_bin = config_home.path().join("snap/bin");
    fs::create_dir_all(&snap_bin).expect("snap path should be writable");
    let executable = snap_bin.join("firefox");
    fs::write(
        &executable,
        "#!/bin/sh\nprintf '%s\\n' \"$@\" > \"$BROWSER_PICKER_TEST_OUTPUT\"\n",
    )
    .expect("sandboxed browser should be writable");
    fs::set_permissions(&executable, fs::Permissions::from_mode(0o700))
        .expect("sandboxed browser should be executable");
    let data_home = install_family_desktop(&config_home, "firefox.desktop", "Firefox", &executable);
    let profile = config_home.path().join("work-profile");
    fs::create_dir(&profile).expect("Firefox profile should exist");
    write_raw_config(
        &config_home,
        &format!(
            "version = 1\n\n[[destinations]]\nid = \"firefox-work\"\nlabel = \"Work Firefox\"\nprofile_label = \"Work\"\n\n[destinations.application]\ntype = \"firefox-profile\"\ndesktop_id = \"firefox.desktop\"\nname = \"Work\"\npath = \"{}\"\n\n[[rules]]\nid = \"private-work\"\nname = \"Private work\"\nenabled = true\n\n[rules.action]\ntype = \"open\"\ndestination = \"firefox-work\"\nmode = \"private\"\n\n[[rules.groups]]\n\n[[rules.groups.conditions]]\ntype = \"host\"\nvalue = \"work.example\"\n\n[fallback]\naction = \"show-picker\"\n",
            profile.display()
        ),
    );
    let received = config_home.path().join("received-argv");

    let output = browser_picker(&config_home)
        .env("XDG_DATA_HOME", &data_home)
        .env("XDG_DATA_DIRS", &data_home)
        .env("BROWSER_PICKER_TEST_OUTPUT", &received)
        .arg("https://work.example/mail")
        .output()
        .expect("Browser Picker should start");

    assert_eq!(output.status.code(), Some(4));
    assert!(
        received.exists().not(),
        "private Launch Mode must not downgrade to a normal launch"
    );
    let stderr = String::from_utf8(output.stderr).expect("error output should be UTF-8");
    assert_eq!(
        stderr,
        "Browser Destination 'firefox-work' could not accept dispatch: private Launch Mode is not available\n"
    );
}
