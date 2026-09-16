use std::process::{Command, Stdio};

use crate::configuration::BrowserDestination;
use crate::open_target::WebTarget;

pub enum FailureReason {
    NotFound,
    PermissionDenied,
    Other,
}

pub struct Error {
    pub destination_id: String,
    pub reason: FailureReason,
}

pub fn dispatch(
    destination: &BrowserDestination,
    target: &WebTarget,
    private: bool,
) -> Result<(), Error> {
    let arguments = if private {
        destination
            .private_arguments
            .as_deref()
            .unwrap_or(&destination.arguments)
    } else {
        &destination.arguments
    };
    let mut command = Command::new(&destination.executable);
    command.args(arguments.iter().map(|argument| {
        if argument == "{target}" {
            target.as_str()
        } else {
            argument.as_str()
        }
    }));
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    command.spawn().map(|_| ()).map_err(|error| Error {
        destination_id: destination.id.clone(),
        reason: match error.kind() {
            std::io::ErrorKind::NotFound => FailureReason::NotFound,
            std::io::ErrorKind::PermissionDenied => FailureReason::PermissionDenied,
            _ => FailureReason::Other,
        },
    })
}
