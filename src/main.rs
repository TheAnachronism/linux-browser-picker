mod application;
mod configuration;
mod i18n;
mod launcher;
mod open_target;
mod routing;

use std::env;

const HELP: &str = "Browser Picker\nChoose where links and local files open.\n\nUsage:\n  browser-picker\n  browser-picker <http(s)-url>\n  browser-picker --help\n  browser-picker --version\n\nOptions:\n  -h, --help       Show this help\n  -V, --version    Show version\n";
const STATUS_INVALID_TARGET: u8 = 2;
const STATUS_CONFIGURATION: u8 = 3;
const STATUS_LAUNCH: u8 = 4;
const STATUS_ENVIRONMENT: u8 = 5;

fn main() -> gtk::glib::ExitCode {
    i18n::initialize();

    match env::args_os().nth(1).as_deref() {
        Some(argument) if matches!(argument.to_str(), Some("-h" | "--help")) => {
            print!("{}", i18n::text(HELP));
            gtk::glib::ExitCode::SUCCESS
        }
        Some(argument) if matches!(argument.to_str(), Some("-V" | "--version")) => {
            println!(
                "{} {}",
                i18n::text("Browser Picker"),
                env!("CARGO_PKG_VERSION")
            );
            gtk::glib::ExitCode::SUCCESS
        }
        Some(argument) if argument.to_string_lossy().starts_with('-') => {
            eprintln!(
                "{}: {}",
                i18n::text("Unknown argument"),
                argument.to_string_lossy()
            );
            gtk::glib::ExitCode::FAILURE
        }
        Some(argument) => match routing::route(argument) {
            Ok(()) => gtk::glib::ExitCode::SUCCESS,
            Err(error) => {
                let (message, status) = routing_error_message(error);
                eprintln!("{message}");
                gtk::glib::ExitCode::from(status)
            }
        },
        None => application::run(),
    }
}

fn routing_error_message(error: routing::Error) -> (String, u8) {
    match error {
        routing::Error::InvalidTarget(error) => {
            (target_error_message(error), STATUS_INVALID_TARGET)
        }
        routing::Error::Configuration(error) => {
            (configuration_error_message(error), STATUS_CONFIGURATION)
        }
        routing::Error::Launch(error) => (launch_error_message(error), STATUS_LAUNCH),
        routing::Error::NoGraphicalSession => (
            i18n::text("Routing an Open Target requires a graphical session"),
            STATUS_ENVIRONMENT,
        ),
    }
}

fn target_error_message(error: open_target::Error) -> String {
    match error {
        open_target::Error::InvalidUtf8 => i18n::text("Open Target must be valid UTF-8"),
        open_target::Error::TooLarge => i18n::text("Open Target exceeds the 65536-byte limit"),
        open_target::Error::Malformed => {
            i18n::text("Invalid Open Target: expected an absolute HTTP or HTTPS URL")
        }
        open_target::Error::UnsupportedScheme => {
            i18n::text("Unsupported Open Target scheme; expected HTTP or HTTPS")
        }
    }
}

fn configuration_error_message(error: configuration::Error) -> String {
    match error {
        configuration::Error::ConfigHomeNotAbsolute => {
            i18n::text("XDG_CONFIG_HOME must be an absolute path")
        }
        configuration::Error::HomeNotSet => i18n::text("HOME is not set"),
        configuration::Error::Read(std::io::ErrorKind::NotFound) => {
            i18n::text("Browser Picker configuration was not found")
        }
        configuration::Error::Read(std::io::ErrorKind::PermissionDenied) => {
            i18n::text("Browser Picker configuration permission was denied")
        }
        configuration::Error::Read(_) => {
            i18n::text("Browser Picker configuration could not be read")
        }
        configuration::Error::InvalidToml => {
            i18n::text("Configuration is not valid versioned TOML")
        }
        configuration::Error::UnsupportedVersion(version) => i18n::text_with(
            "Unsupported configuration version {version}; expected version 1",
            &[("{version}", &version.to_string())],
        ),
        configuration::Error::InvalidId(id) => i18n::text_with(
            "Browser Destination ID '{id}' must be a lowercase slug",
            &[("{id}", &id)],
        ),
        configuration::Error::DuplicateId(id) => i18n::text_with(
            "Browser Destination ID '{id}' is not unique",
            &[("{id}", &id)],
        ),
        configuration::Error::UnknownFallback(id) => i18n::text_with(
            "Fallback references unknown destination '{id}'",
            &[("{id}", &id)],
        ),
        configuration::Error::InvalidExecutable(id) => i18n::text_with(
            "Manual destination '{id}' executable must be an absolute path or a PATH-resolved name",
            &[("{id}", &id)],
        ),
        configuration::Error::InvalidTargetTemplate(id) => i18n::text_with(
            "Manual destination '{id}' must contain exactly one '{placeholder}' argument",
            &[("{id}", &id), ("{placeholder}", "{target}")],
        ),
    }
}

fn launch_error_message(error: launcher::Error) -> String {
    let reason = match error.reason {
        launcher::FailureReason::NotFound => i18n::text("executable was not found"),
        launcher::FailureReason::PermissionDenied => i18n::text("executable permission was denied"),
        launcher::FailureReason::Other => i18n::text("process could not be started"),
    };
    i18n::text_with(
        "Browser Destination '{id}' could not accept dispatch: {reason}",
        &[("{id}", &error.destination_id), ("{reason}", &reason)],
    )
}
