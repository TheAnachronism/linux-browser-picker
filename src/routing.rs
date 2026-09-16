use std::ffi::OsStr;

use crate::configuration::{self, BrowserDestination, FallbackAction};
use crate::launcher;
use crate::open_target::{self, WebTarget};

pub enum Error {
    InvalidTarget(open_target::Error),
    Configuration(configuration::Error),
    Launch(launcher::Error),
    NoGraphicalSession,
}

pub enum Outcome {
    Dispatched,
    Pick {
        target: WebTarget,
        destinations: Vec<BrowserDestination>,
    },
}

pub fn route(argument: &OsStr) -> Result<Outcome, Error> {
    if std::env::var_os("DISPLAY").is_none() && std::env::var_os("WAYLAND_DISPLAY").is_none() {
        return Err(Error::NoGraphicalSession);
    }

    let target = WebTarget::parse(argument).map_err(Error::InvalidTarget)?;
    let configuration = configuration::load().map_err(Error::Configuration)?;
    match configuration.fallback {
        FallbackAction::Open(destination_id) => {
            let destination = configuration
                .destinations
                .iter()
                .find(|destination| destination.id == destination_id)
                .expect("validated fallback destination should exist");
            launcher::dispatch(destination, &target, false).map_err(Error::Launch)?;
            Ok(Outcome::Dispatched)
        }
        FallbackAction::ShowPicker => Ok(Outcome::Pick {
            target,
            destinations: configuration.destinations,
        }),
    }
}
