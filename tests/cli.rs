use std::process::Command;

fn browser_picker() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_browser-picker"));
    command
        .env_remove("DISPLAY")
        .env_remove("WAYLAND_DISPLAY")
        .env_remove("DBUS_SESSION_BUS_ADDRESS");
    command
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
        "Browser Picker\nChoose where links and local files open.\n\nUsage:\n  browser-picker\n  browser-picker --help\n  browser-picker --version\n\nOptions:\n  -h, --help       Show this help\n  -V, --version    Show version\n"
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
