mod application;
mod i18n;

use std::env;

const HELP: &str = "Browser Picker\nChoose where links and local files open.\n\nUsage:\n  browser-picker\n  browser-picker --help\n  browser-picker --version\n\nOptions:\n  -h, --help       Show this help\n  -V, --version    Show version\n";

fn main() -> gtk::glib::ExitCode {
    i18n::initialize();

    match env::args().nth(1).as_deref() {
        Some("-h" | "--help") => {
            print!("{}", i18n::text(HELP));
            gtk::glib::ExitCode::SUCCESS
        }
        Some("-V" | "--version") => {
            println!(
                "{} {}",
                i18n::text("Browser Picker"),
                env!("CARGO_PKG_VERSION")
            );
            gtk::glib::ExitCode::SUCCESS
        }
        Some(argument) => {
            eprintln!("{}: {argument}", i18n::text("Unknown argument"));
            gtk::glib::ExitCode::FAILURE
        }
        None => application::run(),
    }
}
