use std::process::{Command, Stdio};

use crate::configuration::LaunchDefinition;
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

pub fn dispatch(definition: LaunchDefinition, target: &WebTarget) -> Result<(), Error> {
    let mut command = Command::new(&definition.executable);
    command.args(definition.arguments.iter().map(|argument| {
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
        destination_id: definition.destination_id,
        reason: match error.kind() {
            std::io::ErrorKind::NotFound => FailureReason::NotFound,
            std::io::ErrorKind::PermissionDenied => FailureReason::PermissionDenied,
            _ => FailureReason::Other,
        },
    })
}
