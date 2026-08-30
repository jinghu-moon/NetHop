use serde::{Deserialize, Serialize};
use std::time::Duration;

pub const DEFAULT_TEST_WARM_TIMEOUT: Duration = Duration::from_secs(5 * 60);
pub const DEFAULT_TEST_COLD_TIMEOUT: Duration = Duration::from_secs(30 * 60);

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResourceSample {
    pub observed_at_ms: u64,
    pub rss_bytes: u64,
    pub cpu_user_ms: u64,
    pub cpu_system_ms: u64,
    pub threads: u32,
    pub open_fds: u32,
    pub active_connections: u32,
    pub dns_cache_entries: u32,
    pub rule_set_bytes: u64,
    pub wakeup_count: u64,
    pub network_bytes: u64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResourceAggregate {
    pub sample_count: u32,
    pub rss_baseline_bytes: u64,
    pub rss_median_bytes: u64,
    pub rss_p95_bytes: u64,
    pub rss_max_bytes: u64,
    pub rss_upper_bound_bytes: u64,
    pub cpu_user_ms: u64,
    pub cpu_system_ms: u64,
    pub threads_max: u32,
    pub open_fds_max: u32,
    pub active_connections_max: u32,
    pub wakeup_count: u64,
}

impl ResourceAggregate {
    pub fn from_samples(samples: &[ResourceSample]) -> Option<Self> {
        if samples.is_empty() {
            return None;
        }
        let mut rss: Vec<u64> = samples.iter().map(|sample| sample.rss_bytes).collect();
        rss.sort_unstable();
        let percentile = |numerator: usize, denominator: usize| -> u64 {
            let index = ((rss.len().saturating_sub(1) * numerator) + denominator - 1) / denominator;
            rss[index.min(rss.len() - 1)]
        };
        let first = samples[0];
        let last = samples[samples.len() - 1];
        Some(Self {
            sample_count: samples.len() as u32,
            rss_baseline_bytes: first.rss_bytes,
            rss_median_bytes: percentile(1, 2),
            rss_p95_bytes: percentile(95, 100),
            rss_max_bytes: *rss.last().unwrap_or(&0),
            rss_upper_bound_bytes: last.rss_bytes.max(*rss.last().unwrap_or(&0)),
            cpu_user_ms: last.cpu_user_ms.saturating_sub(first.cpu_user_ms),
            cpu_system_ms: last.cpu_system_ms.saturating_sub(first.cpu_system_ms),
            threads_max: samples
                .iter()
                .map(|sample| sample.threads)
                .max()
                .unwrap_or(0),
            open_fds_max: samples
                .iter()
                .map(|sample| sample.open_fds)
                .max()
                .unwrap_or(0),
            active_connections_max: samples
                .iter()
                .map(|sample| sample.active_connections)
                .max()
                .unwrap_or(0),
            wakeup_count: last.wakeup_count.saturating_sub(first.wakeup_count),
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResourceBudget {
    pub warm_rss_p95_bytes: u64,
    pub warm_cpu_user_ms: u64,
    pub idle_rss_p95_bytes: u64,
    pub idle_cpu_user_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourcePressure {
    None,
    WarmBudgetExceeded,
    IdleBudgetExceeded,
    ActiveBudgetExceeded,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IdleInputs {
    pub now_ms: u64,
    pub last_capture_disabled_ms: u64,
    pub last_demand_ms: u64,
    pub pending_operations: u32,
    pub active_connections: u32,
    pub network_recovery: bool,
    pub resource_pressure: ResourcePressure,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IdleDecision {
    KeepActive,
    KeepWarm,
    EnterIdle,
    EnterCold,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IdlePolicy {
    pub warm_timeout_ms: u64,
    pub cold_timeout_ms: u64,
}

impl IdlePolicy {
    pub const fn test_defaults() -> Self {
        Self {
            warm_timeout_ms: DEFAULT_TEST_WARM_TIMEOUT.as_millis() as u64,
            cold_timeout_ms: DEFAULT_TEST_COLD_TIMEOUT.as_millis() as u64,
        }
    }

    pub fn new(warm_timeout: Duration, cold_timeout: Duration) -> Option<Self> {
        if warm_timeout.is_zero() || cold_timeout < warm_timeout {
            return None;
        }
        Some(Self {
            warm_timeout_ms: warm_timeout.as_millis() as u64,
            cold_timeout_ms: cold_timeout.as_millis() as u64,
        })
    }

    pub fn decide(&self, input: IdleInputs) -> IdleDecision {
        if input.pending_operations > 0 || input.active_connections > 0 || input.network_recovery {
            return IdleDecision::KeepWarm;
        }
        match input.resource_pressure {
            ResourcePressure::ActiveBudgetExceeded => IdleDecision::KeepActive,
            ResourcePressure::IdleBudgetExceeded => IdleDecision::EnterCold,
            ResourcePressure::WarmBudgetExceeded => IdleDecision::EnterIdle,
            ResourcePressure::None => {
                let idle_for = input.now_ms.saturating_sub(input.last_capture_disabled_ms);
                let demand_for = input.now_ms.saturating_sub(input.last_demand_ms);
                if idle_for >= self.cold_timeout_ms && demand_for >= self.cold_timeout_ms {
                    IdleDecision::EnterCold
                } else if idle_for >= self.warm_timeout_ms && demand_for >= self.warm_timeout_ms {
                    IdleDecision::EnterIdle
                } else {
                    IdleDecision::KeepWarm
                }
            }
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResumeCost {
    pub resume_accepted_ms: u64,
    pub resume_core_ready_ms: u64,
    pub resume_capture_enabled_ms: u64,
    pub resume_first_successful_tcp_ms: Option<u64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(at: u64, rss: u64) -> ResourceSample {
        ResourceSample {
            observed_at_ms: at,
            rss_bytes: rss,
            cpu_user_ms: at,
            ..ResourceSample::default()
        }
    }

    #[test]
    fn aggregate_is_deterministic_and_bounded() {
        let aggregate =
            ResourceAggregate::from_samples(&[sample(0, 10), sample(1000, 30), sample(2000, 20)])
                .unwrap();
        assert_eq!(aggregate.sample_count, 3);
        assert_eq!(aggregate.rss_baseline_bytes, 10);
        assert_eq!(aggregate.rss_median_bytes, 20);
        assert_eq!(aggregate.rss_p95_bytes, 30);
        assert_eq!(aggregate.rss_upper_bound_bytes, 30);
    }

    #[test]
    fn idle_policy_honors_blockers_and_deadlines() {
        let policy = IdlePolicy::new(Duration::from_secs(5), Duration::from_secs(30)).unwrap();
        let base = IdleInputs {
            now_ms: 31_000,
            last_capture_disabled_ms: 0,
            last_demand_ms: 0,
            pending_operations: 0,
            active_connections: 0,
            network_recovery: false,
            resource_pressure: ResourcePressure::None,
        };
        assert_eq!(policy.decide(base), IdleDecision::EnterCold);
        assert_eq!(
            policy.decide(IdleInputs {
                active_connections: 1,
                ..base
            }),
            IdleDecision::KeepWarm
        );
        assert_eq!(
            policy.decide(IdleInputs {
                resource_pressure: ResourcePressure::WarmBudgetExceeded,
                ..base
            }),
            IdleDecision::EnterIdle
        );
    }
}
