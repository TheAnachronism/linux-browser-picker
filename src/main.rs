mod application;
mod configuration;
mod discovery;
mod i18n;
mod launcher;
mod open_target;
mod profiles;
mod routing;
mod routing_editor;
mod setup;
mod url_pattern;

use std::env;

const HELP: &str = "Browser Picker\nChoose where links and local files open.\n\nUsage:\n  browser-picker\n  browser-picker <http(s)-url|file>\n  browser-picker --help\n  browser-picker --version\n\nOptions:\n  -h, --help       Show this help\n  -V, --version    Show version\n";
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
        Some(argument)
            if env::var_os("DBUS_SESSION_BUS_ADDRESS").is_none() && env::args_os().len() == 2 =>
        {
            match routing::route(argument) {
                Ok(routing::Outcome::Dispatched) => gtk::glib::ExitCode::SUCCESS,
                Ok(
                    routing::Outcome::Pick { .. }
                    | routing::Outcome::Setup { .. }
                    | routing::Outcome::Recover { .. }
                    | routing::Outcome::Migrate { .. },
                ) => application::run(),
                Err(error) => {
                    let (message, status) = routing_error_message(error);
                    eprintln!("{message}");
                    gtk::glib::ExitCode::from(status)
                }
            }
        }
        Some(_) | None => application::run(),
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
    error.message()
}

fn configuration_error_message(error: configuration::Error) -> String {
    error.message()
}

fn launch_error_message(error: launcher::Error) -> String {
    let reason = match error.reason {
        launcher::FailureReason::NotFound => i18n::text("executable was not found"),
        launcher::FailureReason::PermissionDenied => i18n::text("executable permission was denied"),
        launcher::FailureReason::UnsupportedPrivate => {
            i18n::text("private Launch Mode is not available")
        }
        launcher::FailureReason::Other => i18n::text("process could not be started"),
    };
    i18n::text_with(
        "Browser Destination '{id}' could not accept dispatch: {reason}",
        &[("{id}", &error.destination_id), ("{reason}", &reason)],
    )
}
