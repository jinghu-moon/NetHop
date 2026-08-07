use crate::{ProbeBackend, ProbeCommand};

pub const CORE_UPDATE_NOTIFICATION_TAG: &str = "nethop-core-update";

pub fn core_update_notification_arguments() -> Vec<String> {
    [
        "notification",
        "post",
        "-S",
        "bigtext",
        "-t",
        "NetHop update available",
        CORE_UPDATE_NOTIFICATION_TAG,
        "A newer stable sing-box core is available. Update NetHop to use it.",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateNotificationOutcome {
    Posted,
    Unavailable,
}

pub trait UpdateNotificationSink {
    fn notify_core_update(&mut self) -> UpdateNotificationOutcome;
}

pub struct CommandUpdateNotifier<B> {
    backend: B,
}

impl<B> CommandUpdateNotifier<B> {
    pub const fn new(backend: B) -> Self {
        Self { backend }
    }
}

impl<B: ProbeBackend> UpdateNotificationSink for CommandUpdateNotifier<B> {
    fn notify_core_update(&mut self) -> UpdateNotificationOutcome {
        match self.backend.run(ProbeCommand::CoreUpdateNotification) {
            Ok(output) if output.success() => UpdateNotificationOutcome::Posted,
            Ok(_) | Err(_) => UpdateNotificationOutcome::Unavailable,
        }
    }
}
