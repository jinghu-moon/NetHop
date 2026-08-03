use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeState {
    Init,
    Probing,
    StartingCore,
    RunningTproxy,
    StartingTun,
    RunningTun,
    Degraded,
    Backoff,
    FailOpenDirect,
    Stopping,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum StateTransitionError {
    #[error("invalid runtime state transition from {from:?} to {to:?}")]
    Invalid {
        from: RuntimeState,
        to: RuntimeState,
    },
}

impl RuntimeState {
    pub fn transition(self, next: Self) -> Result<Self, StateTransitionError> {
        let valid = matches!(
            (self, next),
            (Self::Init, Self::Probing)
                | (Self::Probing, Self::StartingCore)
                | (Self::Probing, Self::StartingTun)
                | (Self::Probing, Self::FailOpenDirect)
                | (Self::StartingCore, Self::RunningTproxy)
                | (Self::StartingCore, Self::StartingTun)
                | (Self::StartingCore, Self::FailOpenDirect)
                | (Self::StartingTun, Self::RunningTun)
                | (Self::StartingTun, Self::Backoff)
                | (Self::StartingTun, Self::FailOpenDirect)
                | (Self::RunningTproxy, Self::Degraded)
                | (Self::RunningTproxy, Self::Stopping)
                | (Self::RunningTun, Self::Degraded)
                | (Self::RunningTun, Self::Stopping)
                | (Self::Degraded, Self::Backoff)
                | (Self::Degraded, Self::Stopping)
                | (Self::Backoff, Self::Probing)
                | (Self::Backoff, Self::FailOpenDirect)
                | (Self::FailOpenDirect, Self::Probing)
                | (Self::FailOpenDirect, Self::Stopping)
        );
        valid.then_some(next).ok_or(StateTransitionError::Invalid {
            from: self,
            to: next,
        })
    }
}
