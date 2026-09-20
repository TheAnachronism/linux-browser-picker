mod application;
mod associations;
mod configuration;
mod discovery;
mod i18n;
mod launcher;
mod list_order;
mod open_target;
mod profiles;
mod reorder_ui;
mod routing;
mod routing_editor;
mod setup;
mod url_pattern;
mod window_state;

use std::env;
use std::ffi::OsStr;

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
const STATUS_INVALID_TARGET: u8 = 2;
const STATUS_CONFIGURATION: u8 = 3;
const STATUS_LAUNCH: u8 = 4;
const STATUS_ENVIRONMENT: u8 = 5;
pub(crate) const STATUS_OVERFLOW: u8 = 6;
const STATUS_FORWARDING: u8 = 7;
pub(crate) const MAX_TARGETS_PER_ACTIVATION: usize = 100;

fn main() -> gtk::glib::ExitCode {
    i18n::initialize();

    match env::args_os().nth(1).as_deref() {
        Some(argument) if matches!(argument.to_str(), Some("-h" | "--help" | "help")) => {
            print!("{}", i18n::text(HELP));
            gtk::glib::ExitCode::SUCCESS
        }
        Some(argument) if matches!(argument.to_str(), Some("-V" | "--version" | "version")) => {
            println!(
                "{} {}",
                i18n::text("Browser Picker"),
                env!("CARGO_PKG_VERSION")
            );
            gtk::glib::ExitCode::SUCCESS
        }
        Some(argument) if matches!(argument.to_str(), Some("validate" | "--validate")) => {
            validate_configuration()
        }
        Some(argument) if matches!(argument.to_str(), Some("diagnose" | "--diagnose")) => {
            diagnose_target()
        }
        Some(argument) if matches!(argument.to_str(), Some("associations" | "--associations")) => {
            print_associations()
        }
        Some(argument) if is_config_operation(argument.to_str()) => open_configuration(),
        None => open_configuration(),
        Some(argument) if argument.to_string_lossy().starts_with('-') => {
            eprintln!(
                "{}\n{}",
                i18n::text("Unknown argument"),
                i18n::text("Run 'browser-picker help' to inspect valid operations.")
            );
            gtk::glib::ExitCode::FAILURE
        }
        Some(_) => route_arguments(),
    }
}

fn has_graphical_session() -> bool {
    env::var_os("DISPLAY").is_some() || env::var_os("WAYLAND_DISPLAY").is_some()
}

fn has_session_bus() -> bool {
    env::var_os("DBUS_SESSION_BUS_ADDRESS").is_some()
}

pub(crate) fn is_config_operation(argument: Option<&str>) -> bool {
    matches!(argument, Some("config" | "--config"))
}

fn environment_failure(message: String) -> gtk::glib::ExitCode {
    eprintln!("{message}");
    gtk::glib::ExitCode::from(STATUS_ENVIRONMENT)
}

fn open_configuration() -> gtk::glib::ExitCode {
    if !has_graphical_session() {
        return environment_failure(i18n::text(
            "Opening configuration requires a graphical session",
        ));
    }
    if !has_session_bus() {
        return environment_failure(i18n::text(
            "Picker, configuration, and shared queue operations require a user session D-Bus",
        ));
    }
    application::run()
}

fn print_associations() -> gtk::glib::ExitCode {
    for line in associations::report().lines() {
        println!("{line}");
    }
    gtk::glib::ExitCode::SUCCESS
}

fn diagnose_target() -> gtk::glib::ExitCode {
    let Some(argument) = env::args_os().nth(2) else {
        eprintln!("{}", i18n::text("diagnose requires an Open Target"));
        return gtk::glib::ExitCode::FAILURE;
    };
    match routing::diagnose(&argument) {
        Ok(report) => {
            print!("{report}");
            gtk::glib::ExitCode::SUCCESS
        }
        Err(error) => {
            let (message, status) = routing_error_message(error);
            eprintln!("{message}");
            gtk::glib::ExitCode::from(status)
        }
    }
}

fn validate_configuration() -> gtk::glib::ExitCode {
    let path = match configuration::default_path() {
        Ok(path) => path,
        Err(error) => {
            eprintln!("{}", configuration_error_message(error));
            return gtk::glib::ExitCode::from(STATUS_CONFIGURATION);
        }
    };
    let store = match configuration::ConfigurationStore::inspect_path(&path) {
        Ok(store) => store,
        Err(error) => {
            eprintln!("{}", configuration_error_message(error));
            return gtk::glib::ExitCode::from(STATUS_CONFIGURATION);
        }
    };
    for warning in &store.warnings {
        eprintln!("{warning}");
    }
    let error = match store.status {
        configuration::StoreStatus::Current => return gtk::glib::ExitCode::SUCCESS,
        configuration::StoreStatus::Missing => {
            configuration::Error::Read(std::io::ErrorKind::NotFound)
        }
        configuration::StoreStatus::Invalid(error) => error,
        configuration::StoreStatus::Migratable(preview) => {
            configuration::Error::MigrationRequired {
                from: preview.from,
                to: preview.to,
            }
        }
    };
    eprintln!("{}", configuration_error_message(error));
    gtk::glib::ExitCode::from(STATUS_CONFIGURATION)
}

fn route_arguments() -> gtk::glib::ExitCode {
    if !has_graphical_session() {
        return environment_failure(i18n::text(
            "Routing an Open Target requires a graphical session",
        ));
    }
    if has_session_bus() {
        return application::run();
    }
    let arguments: Vec<_> = env::args_os().skip(1).collect();
    if arguments.is_empty() {
        return open_configuration();
    }
    route_without_session_bus(&arguments)
}

fn route_without_session_bus(arguments: &[std::ffi::OsString]) -> gtk::glib::ExitCode {
    let mut status = gtk::glib::ExitCode::SUCCESS;
    for (index, argument) in arguments.iter().enumerate() {
        if index == MAX_TARGETS_PER_ACTIVATION {
            eprintln!(
                "{}",
                i18n::text("One activation accepts at most 100 Open Targets")
            );
            status = gtk::glib::ExitCode::from(STATUS_OVERFLOW);
            break;
        }
        match route_one_shot(argument) {
            gtk::glib::ExitCode::SUCCESS => {}
            error_status => {
                if status == gtk::glib::ExitCode::SUCCESS {
                    status = error_status;
                }
            }
        }
    }
    status
}

fn route_one_shot(argument: &OsStr) -> gtk::glib::ExitCode {
    match routing::route(argument) {
        Ok(routing::Outcome::Dispatched) => gtk::glib::ExitCode::SUCCESS,
        Ok(
            routing::Outcome::Pick { .. }
            | routing::Outcome::RecoverLaunch { .. }
            | routing::Outcome::Setup { .. }
            | routing::Outcome::Recover { .. }
            | routing::Outcome::Migrate { .. },
        ) => environment_failure(i18n::text(
            "Picker, configuration, and shared queue operations require a user session D-Bus",
        )),
        Err(error) => {
            let (message, code) = routing_error_message(error);
            eprintln!("{message}");
            gtk::glib::ExitCode::from(code)
        }
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

pub(crate) fn launch_error_message(error: launcher::Error) -> String {
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
