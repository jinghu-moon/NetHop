use nethop_core::{IdleDecision, IdleInputs, IdlePolicy, ResourcePressure, ResourceState};
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IdlePolicyController {
    policy: IdlePolicy,
    resource_state: ResourceState,
    last_capture_disabled: Duration,
    last_demand: Duration,
}

impl IdlePolicyController {
    pub const fn new(policy: IdlePolicy, now: Duration) -> Self {
        Self {
            policy,
            resource_state: ResourceState::Cold,
            last_capture_disabled: now,
            last_demand: now,
        }
    }

    pub const fn resource_state(&self) -> ResourceState {
        self.resource_state
    }

    pub fn record_capture_enabled(&mut self, now: Duration) {
        self.resource_state = ResourceState::Active;
        self.last_demand = now;
    }

    pub fn record_capture_disabled(&mut self, now: Duration) {
        self.resource_state = ResourceState::Warm;
        self.last_capture_disabled = now;
    }

    pub fn record_demand(&mut self, now: Duration) {
        self.last_demand = now;
        if self.resource_state == ResourceState::Idle {
            self.resource_state = ResourceState::Warm;
        }
    }

    pub fn decide(
        &mut self,
        now: Duration,
        pending_operations: u32,
        active_connections: u32,
        network_recovery: bool,
        pressure: ResourcePressure,
    ) -> IdleDecision {
        let input = IdleInputs {
            now_ms: now.as_millis() as u64,
            last_capture_disabled_ms: self.last_capture_disabled.as_millis() as u64,
            last_demand_ms: self.last_demand.as_millis() as u64,
            pending_operations,
            active_connections,
            network_recovery,
            resource_pressure: pressure,
        };
        let decision = self.policy.decide(input);
        self.resource_state = match decision {
            IdleDecision::EnterIdle => ResourceState::Idle,
            IdleDecision::EnterCold => ResourceState::Cold,
            IdleDecision::KeepActive => ResourceState::Active,
            IdleDecision::KeepWarm => ResourceState::Warm,
        };
        decision
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn daemon_controller_moves_warm_to_idle_to_cold_and_wakes_on_demand() {
        let policy = IdlePolicy::new(Duration::from_secs(5), Duration::from_secs(30)).unwrap();
        let mut controller = IdlePolicyController::new(policy, Duration::ZERO);
        controller.record_capture_disabled(Duration::ZERO);
        assert_eq!(
            controller.decide(Duration::from_secs(5), 0, 0, false, ResourcePressure::None),
            IdleDecision::EnterIdle
        );
        controller.record_demand(Duration::from_secs(6));
        assert_eq!(controller.resource_state(), ResourceState::Warm);
        assert_eq!(
            controller.decide(Duration::from_secs(37), 0, 0, false, ResourcePressure::None),
            IdleDecision::EnterCold
        );
    }
}
