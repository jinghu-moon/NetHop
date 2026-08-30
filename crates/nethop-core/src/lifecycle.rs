use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CoreState {
    Stopped,
    Starting,
    Ready,
    Stopping,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaptureState {
    Disabled,
    Preparing,
    Enabled,
    Disabling,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceState {
    Active,
    Warm,
    Idle,
    Cold,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LifecycleEvent {
    CoreStartRequested,
    CoreReady,
    CoreStopRequested,
    CoreStopped,
    CoreFailed,
    CaptureEnableRequested,
    CaptureEnabled,
    CaptureDisableRequested,
    CaptureDisabled,
    CaptureFailed,
    IdleDeadlineReached,
    ResourcePressure,
    UserDemand,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LifecycleAxes {
    pub core_state: CoreState,
    pub capture_state: CaptureState,
    pub resource_state: ResourceState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum LifecycleError {
    #[error("invalid lifecycle axes: core={core:?}, capture={capture:?}, resource={resource:?}")]
    InvalidAxes {
        core: CoreState,
        capture: CaptureState,
        resource: ResourceState,
    },
    #[error("invalid lifecycle event {event:?} for axes {axes:?}")]
    InvalidEvent {
        axes: LifecycleAxes,
        event: LifecycleEvent,
    },
}

impl LifecycleAxes {
    pub const fn cold() -> Self {
        Self {
            core_state: CoreState::Stopped,
            capture_state: CaptureState::Disabled,
            resource_state: ResourceState::Cold,
        }
    }

    pub const fn warm() -> Self {
        Self {
            core_state: CoreState::Ready,
            capture_state: CaptureState::Disabled,
            resource_state: ResourceState::Warm,
        }
    }

    pub const fn active() -> Self {
        Self {
            core_state: CoreState::Ready,
            capture_state: CaptureState::Enabled,
            resource_state: ResourceState::Active,
        }
    }

    pub const fn is_valid(self) -> bool {
        let core_capture_valid = match self.core_state {
            CoreState::Stopped => matches!(
                self.capture_state,
                CaptureState::Disabled | CaptureState::Failed
            ),
            CoreState::Starting | CoreState::Stopping => {
                matches!(
                    self.capture_state,
                    CaptureState::Disabled | CaptureState::Disabling
                )
            }
            CoreState::Ready => true,
            CoreState::Failed => matches!(
                self.capture_state,
                CaptureState::Disabled | CaptureState::Failed
            ),
        };
        let resource_valid = match self.resource_state {
            ResourceState::Active => {
                matches!(self.core_state, CoreState::Ready)
                    && matches!(
                        self.capture_state,
                        CaptureState::Enabled | CaptureState::Disabling
                    )
            }
            ResourceState::Warm => {
                matches!(self.core_state, CoreState::Ready)
                    && matches!(
                        self.capture_state,
                        CaptureState::Disabled | CaptureState::Preparing | CaptureState::Failed
                    )
            }
            ResourceState::Idle => {
                matches!(self.core_state, CoreState::Ready | CoreState::Stopping)
                    && matches!(self.capture_state, CaptureState::Disabled)
            }
            ResourceState::Cold => {
                matches!(
                    self.core_state,
                    CoreState::Stopped | CoreState::Starting | CoreState::Stopping
                ) && matches!(self.capture_state, CaptureState::Disabled)
            }
        };
        core_capture_valid && resource_valid
    }

    pub fn validate(self) -> Result<Self, LifecycleError> {
        self.is_valid()
            .then_some(self)
            .ok_or(LifecycleError::InvalidAxes {
                core: self.core_state,
                capture: self.capture_state,
                resource: self.resource_state,
            })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LifecycleReducer {
    axes: LifecycleAxes,
}

impl Default for LifecycleReducer {
    fn default() -> Self {
        Self::new()
    }
}

impl LifecycleReducer {
    pub const fn new() -> Self {
        Self {
            axes: LifecycleAxes::cold(),
        }
    }

    pub const fn from_axes(axes: LifecycleAxes) -> Result<Self, LifecycleError> {
        if axes.is_valid() {
            Ok(Self { axes })
        } else {
            Err(LifecycleError::InvalidAxes {
                core: axes.core_state,
                capture: axes.capture_state,
                resource: axes.resource_state,
            })
        }
    }

    pub const fn axes(self) -> LifecycleAxes {
        self.axes
    }

    pub fn apply(&mut self, event: LifecycleEvent) -> Result<LifecycleAxes, LifecycleError> {
        let next = match (self.axes, event) {
            (axes, LifecycleEvent::CoreStartRequested) if axes == LifecycleAxes::cold() => {
                LifecycleAxes {
                    core_state: CoreState::Starting,
                    capture_state: CaptureState::Disabled,
                    resource_state: ResourceState::Cold,
                }
            }
            (axes, LifecycleEvent::CoreReady)
                if matches!(axes.core_state, CoreState::Starting)
                    && matches!(axes.capture_state, CaptureState::Disabled) =>
            {
                LifecycleAxes::warm()
            }
            (axes, LifecycleEvent::CaptureEnableRequested)
                if axes.core_state == CoreState::Ready
                    && axes.capture_state == CaptureState::Disabled =>
            {
                LifecycleAxes {
                    core_state: CoreState::Ready,
                    capture_state: CaptureState::Preparing,
                    resource_state: ResourceState::Warm,
                }
            }
            (axes, LifecycleEvent::CaptureEnabled)
                if axes.core_state == CoreState::Ready
                    && axes.capture_state == CaptureState::Preparing =>
            {
                LifecycleAxes::active()
            }
            (axes, LifecycleEvent::CaptureDisableRequested)
                if axes.core_state == CoreState::Ready
                    && axes.capture_state == CaptureState::Enabled =>
            {
                LifecycleAxes {
                    core_state: CoreState::Ready,
                    capture_state: CaptureState::Disabling,
                    resource_state: ResourceState::Active,
                }
            }
            (axes, LifecycleEvent::CaptureDisabled)
                if axes.core_state == CoreState::Ready
                    && matches!(
                        axes.capture_state,
                        CaptureState::Disabling | CaptureState::Enabled
                    ) =>
            {
                LifecycleAxes::warm()
            }
            (axes, LifecycleEvent::IdleDeadlineReached) if axes == LifecycleAxes::warm() => {
                LifecycleAxes {
                    core_state: CoreState::Ready,
                    capture_state: CaptureState::Disabled,
                    resource_state: ResourceState::Idle,
                }
            }
            (axes, LifecycleEvent::CoreStopRequested)
                if matches!(
                    axes.resource_state,
                    ResourceState::Warm | ResourceState::Idle
                ) && axes.core_state == CoreState::Ready
                    && axes.capture_state == CaptureState::Disabled =>
            {
                LifecycleAxes {
                    core_state: CoreState::Stopping,
                    capture_state: CaptureState::Disabled,
                    resource_state: axes.resource_state,
                }
            }
            (axes, LifecycleEvent::CoreStopped)
                if axes.core_state == CoreState::Stopping
                    && axes.capture_state == CaptureState::Disabled =>
            {
                LifecycleAxes::cold()
            }
            (axes, LifecycleEvent::UserDemand) if axes.resource_state == ResourceState::Idle => {
                LifecycleAxes::warm()
            }
            (axes, LifecycleEvent::CoreFailed) if axes.capture_state == CaptureState::Disabled => {
                LifecycleAxes {
                    core_state: CoreState::Failed,
                    capture_state: CaptureState::Disabled,
                    resource_state: ResourceState::Cold,
                }
            }
            (axes, LifecycleEvent::CaptureFailed) if axes.core_state == CoreState::Ready => {
                LifecycleAxes {
                    core_state: CoreState::Ready,
                    capture_state: CaptureState::Failed,
                    resource_state: ResourceState::Warm,
                }
            }
            _ => {
                return Err(LifecycleError::InvalidEvent {
                    axes: self.axes,
                    event,
                });
            }
        };
        self.axes = next.validate()?;
        Ok(next)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_core_capture_invalid_combinations() {
        let invalid = LifecycleAxes {
            core_state: CoreState::Stopped,
            capture_state: CaptureState::Enabled,
            resource_state: ResourceState::Active,
        };
        assert!(matches!(
            invalid.validate(),
            Err(LifecycleError::InvalidAxes { .. })
        ));
    }

    #[test]
    fn follows_cold_warm_active_idle_cold_path() {
        let mut reducer = LifecycleReducer::new();
        for event in [
            LifecycleEvent::CoreStartRequested,
            LifecycleEvent::CoreReady,
            LifecycleEvent::CaptureEnableRequested,
            LifecycleEvent::CaptureEnabled,
            LifecycleEvent::CaptureDisableRequested,
            LifecycleEvent::CaptureDisabled,
            LifecycleEvent::IdleDeadlineReached,
            LifecycleEvent::CoreStopRequested,
            LifecycleEvent::CoreStopped,
        ] {
            reducer.apply(event).unwrap();
        }
        assert_eq!(reducer.axes(), LifecycleAxes::cold());
    }

    #[test]
    fn core_stop_requires_capture_disabled() {
        let reducer = LifecycleReducer::from_axes(LifecycleAxes::active()).unwrap();
        let mut reducer = reducer;
        assert!(matches!(
            reducer.apply(LifecycleEvent::CoreStopRequested),
            Err(LifecycleError::InvalidEvent { .. })
        ));
    }
}
