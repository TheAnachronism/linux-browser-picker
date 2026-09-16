use std::process::{Command, Stdio};

use gtk::gio;
use gtk::gio::prelude::AppInfoExt;

use crate::configuration::{BrowserDestination, DestinationLaunch};
use crate::discovery;
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
    match &destination.launch {
        DestinationLaunch::Manual {
            executable,
            arguments,
            private_arguments,
        } => {
            let arguments = if private {
                private_arguments.as_deref().unwrap_or(arguments)
            } else {
                arguments
            };
            let mut command = Command::new(executable);
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
        DestinationLaunch::Discovered { desktop_id } => {
            let application = discovery::app_info(desktop_id).ok_or_else(|| Error {
                destination_id: destination.id.clone(),
                reason: FailureReason::NotFound,
            })?;
            application
                .launch_uris(&[target.as_str()], gio::AppLaunchContext::NONE)
                .map_err(|_| Error {
                    destination_id: destination.id.clone(),
                    reason: FailureReason::Other,
                })
        }
    }
}
