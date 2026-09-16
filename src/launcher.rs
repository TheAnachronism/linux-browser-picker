use std::path::Path;
use std::process::{Command, Stdio};

use gtk::gio;
use gtk::gio::prelude::AppInfoExt;

use crate::configuration::{BrowserDestination, DestinationLaunch};
use crate::discovery;
use crate::open_target::OpenTarget;
use crate::profiles::{self, BrowserFamily, ProfileIdentity};

pub enum FailureReason {
    NotFound,
    PermissionDenied,
    UnsupportedPrivate,
    Other,
}

pub struct Error {
    pub destination_id: String,
    pub reason: FailureReason,
}

pub fn dispatch(
    destination: &BrowserDestination,
    target: &OpenTarget,
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
            spawn(
                &destination.id,
                Path::new(executable),
                &arguments
                    .iter()
                    .map(|argument| {
                        if argument == "{target}" {
                            target.as_str().to_owned()
                        } else {
                            argument.clone()
                        }
                    })
                    .collect::<Vec<_>>(),
            )
        }
        DestinationLaunch::Discovered { desktop_id } => {
            if private {
                return Err(Error {
                    destination_id: destination.id.clone(),
                    reason: FailureReason::UnsupportedPrivate,
                });
            }
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
        DestinationLaunch::FirefoxProfile {
            desktop_id,
            name,
            path,
        } => dispatch_profile(
            destination,
            desktop_id,
            BrowserFamily::Firefox,
            ProfileIdentity::Firefox {
                name: name.clone(),
                path: path.into(),
            },
            target,
            private,
        ),
        DestinationLaunch::ChromiumProfile {
            desktop_id,
            user_data_dir,
            profile_directory,
        } => dispatch_profile(
            destination,
            desktop_id,
            BrowserFamily::Chromium,
            ProfileIdentity::Chromium {
                user_data_dir: user_data_dir.into(),
                profile_directory: profile_directory.clone(),
            },
            target,
            private,
        ),
    }
}

fn dispatch_profile(
    destination: &BrowserDestination,
    desktop_id: &str,
    family: BrowserFamily,
    identity: ProfileIdentity,
    target: &OpenTarget,
    private: bool,
) -> Result<(), Error> {
    let application = discovery::app_info(desktop_id).ok_or_else(|| Error {
        destination_id: destination.id.clone(),
        reason: FailureReason::NotFound,
    })?;
    let executable = application.executable();
    let paths = profiles::DiscoveryPaths::from_env();
    match profiles::capability(desktop_id, Some(&executable), &identity, &paths) {
        profiles::ProfileCapability::Verified => {}
        profiles::ProfileCapability::MissingApplication
        | profiles::ProfileCapability::MissingProfile => {
            return Err(Error {
                destination_id: destination.id.clone(),
                reason: FailureReason::NotFound,
            });
        }
        profiles::ProfileCapability::UnsupportedPackaging => {
            return Err(Error {
                destination_id: destination.id.clone(),
                reason: if private {
                    FailureReason::UnsupportedPrivate
                } else {
                    FailureReason::Other
                },
            });
        }
    }
    let assumptions = profiles::classify(desktop_id, &executable, &paths)
        .expect("verified profile capability should classify");
    if assumptions.family != family {
        return Err(Error {
            destination_id: destination.id.clone(),
            reason: FailureReason::Other,
        });
    }
    let arguments = profiles::launch_arguments(
        &identity,
        private,
        assumptions.private_flag,
        target.as_str(),
    );
    spawn(&destination.id, &executable, &arguments)
}

fn spawn(destination_id: &str, executable: &Path, arguments: &[String]) -> Result<(), Error> {
    let mut command = Command::new(executable);
    command.args(arguments);
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    command.spawn().map(|_| ()).map_err(|error| Error {
        destination_id: destination_id.to_owned(),
        reason: match error.kind() {
            std::io::ErrorKind::NotFound => FailureReason::NotFound,
            std::io::ErrorKind::PermissionDenied => FailureReason::PermissionDenied,
            _ => FailureReason::Other,
        },
    })
}
