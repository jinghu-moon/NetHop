use nethop_android::{
    CapabilityError, CommandUpdateNotifier, ProbeBackend, ProbeCommand, ProbeOutput,
    UpdateNotificationOutcome, UpdateNotificationSink, core_update_notification_arguments,
};

struct RecordingBackend {
    observed: Vec<ProbeCommand>,
    succeeds: bool,
}

impl ProbeBackend for RecordingBackend {
    fn run(&mut self, command: ProbeCommand) -> Result<ProbeOutput, CapabilityError> {
        self.observed.push(command);
        Ok(ProbeOutput::new(self.succeeds, "", ""))
    }
}

#[test]
fn core_update_notification_uses_fixed_non_sensitive_argv() {
    assert_eq!(
        core_update_notification_arguments(),
        [
            "notification",
            "post",
            "-S",
            "bigtext",
            "-t",
            "NetHop update available",
            "nethop-core-update",
            "A newer stable sing-box core is available. Update NetHop to use it.",
        ]
    );
}

#[test]
fn notification_is_best_effort_and_never_becomes_a_service_error() {
    let mut notifier = CommandUpdateNotifier::new(RecordingBackend {
        observed: Vec::new(),
        succeeds: false,
    });
    assert_eq!(
        notifier.notify_core_update(),
        UpdateNotificationOutcome::Unavailable
    );
}
