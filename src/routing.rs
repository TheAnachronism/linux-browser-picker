use std::ffi::OsStr;

use crate::configuration;
use crate::launcher;
use crate::open_target::{self, WebTarget};

pub enum Error {
    InvalidTarget(open_target::Error),
    Configuration(configuration::Error),
    Launch(launcher::Error),
    NoGraphicalSession,
}

pub fn route(argument: &OsStr) -> Result<(), Error> {
    if std::env::var_os("DISPLAY").is_none() && std::env::var_os("WAYLAND_DISPLAY").is_none() {
        return Err(Error::NoGraphicalSession);
    }

    let target = WebTarget::parse(argument).map_err(Error::InvalidTarget)?;
    let launch = configuration::load_fallback().map_err(Error::Configuration)?;
    launcher::dispatch(launch, &target).map_err(Error::Launch)
}
