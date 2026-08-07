use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::{
    ScheduleKey, SchedulePolicy, ScheduleStore, SchedulerEngine, SchedulerError, SourceConfig,
};

pub const CORE_VERSION_SCHEDULE_KEY: &str = "resource:sing-box-version";
pub const RULE_SET_SCHEDULE_KEY: &str = "resource:rulesets";

pub trait RuntimeUpdateSchedule {
    fn configure(
        &mut self,
        enabled: bool,
        interval_hours: u16,
        sources: &SourceConfig,
    ) -> Result<(), SchedulerError>;
    fn next_wakeup_in(&self) -> Option<Duration>;
    fn take_due(&mut self) -> Result<bool, SchedulerError>;
    fn record_result(&mut self, succeeded: bool) -> Result<(), SchedulerError>;
}

pub trait RuntimeCoreVersionSchedule {
    fn next_wakeup_in(&self) -> Option<Duration>;
    fn take_due(&mut self) -> Result<bool, SchedulerError>;
    fn record_result(&mut self, succeeded: bool) -> Result<(), SchedulerError>;
}

pub trait RuntimeRuleSetSchedule {
    fn next_wakeup_in(&self) -> Option<Duration>;
    fn take_due(&mut self) -> Result<bool, SchedulerError>;
    fn record_result(&mut self, succeeded: bool) -> Result<(), SchedulerError>;
}

#[derive(Debug, Default)]
pub struct UnavailableCoreVersionSchedule;

impl RuntimeCoreVersionSchedule for UnavailableCoreVersionSchedule {
    fn next_wakeup_in(&self) -> Option<Duration> {
        None
    }

    fn take_due(&mut self) -> Result<bool, SchedulerError> {
        Ok(false)
    }

    fn record_result(&mut self, _succeeded: bool) -> Result<(), SchedulerError> {
        Ok(())
    }
}

#[derive(Debug, Default)]
pub struct UnavailableRuleSetSchedule;

impl RuntimeRuleSetSchedule for UnavailableRuleSetSchedule {
    fn next_wakeup_in(&self) -> Option<Duration> {
        None
    }

    fn take_due(&mut self) -> Result<bool, SchedulerError> {
        Ok(false)
    }

    fn record_result(&mut self, _succeeded: bool) -> Result<(), SchedulerError> {
        Ok(())
    }
}

#[derive(Debug)]
struct PersistentResourceSchedule<S: ScheduleStore> {
    engine: SchedulerEngine<S>,
    key: ScheduleKey,
}

impl<S: ScheduleStore> PersistentResourceSchedule<S> {
    fn load(store: S, key: &'static str) -> Result<Self, SchedulerError> {
        let mut engine = SchedulerEngine::load(store, SchedulePolicy::default())?;
        let key = ScheduleKey::new(key)?;
        engine.ensure(key.clone(), wall_seconds()?)?;
        Ok(Self { engine, key })
    }

    fn next_wakeup_in(&self) -> Option<Duration> {
        let now = wall_seconds().ok()?;
        self.engine.record(&self.key).map(|record| {
            Duration::from_secs(
                record
                    .next_run_wall_seconds()
                    .saturating_sub(now)
                    .try_into()
                    .unwrap_or(0),
            )
        })
    }

    fn take_due(&mut self) -> Result<bool, SchedulerError> {
        Ok(self
            .engine
            .due(wall_seconds()?)?
            .into_iter()
            .any(|key| key == self.key))
    }

    fn record_result(&mut self, succeeded: bool) -> Result<(), SchedulerError> {
        let now = wall_seconds()?;
        if succeeded {
            self.engine.mark_success(&self.key, now)
        } else {
            self.engine.mark_failure(&self.key, now)
        }
    }
}

#[derive(Debug)]
pub struct PersistentCoreVersionSchedule<S: ScheduleStore> {
    resource: PersistentResourceSchedule<S>,
}

impl<S: ScheduleStore> PersistentCoreVersionSchedule<S> {
    pub fn load(store: S) -> Result<Self, SchedulerError> {
        Ok(Self {
            resource: PersistentResourceSchedule::load(store, CORE_VERSION_SCHEDULE_KEY)?,
        })
    }
}

impl<S: ScheduleStore> RuntimeCoreVersionSchedule for PersistentCoreVersionSchedule<S> {
    fn next_wakeup_in(&self) -> Option<Duration> {
        self.resource.next_wakeup_in()
    }

    fn take_due(&mut self) -> Result<bool, SchedulerError> {
        self.resource.take_due()
    }

    fn record_result(&mut self, succeeded: bool) -> Result<(), SchedulerError> {
        self.resource.record_result(succeeded)
    }
}

#[derive(Debug)]
pub struct PersistentRuleSetSchedule<S: ScheduleStore> {
    resource: PersistentResourceSchedule<S>,
}

impl<S: ScheduleStore> PersistentRuleSetSchedule<S> {
    pub fn load(store: S) -> Result<Self, SchedulerError> {
        Ok(Self {
            resource: PersistentResourceSchedule::load(store, RULE_SET_SCHEDULE_KEY)?,
        })
    }
}

impl<S: ScheduleStore> RuntimeRuleSetSchedule for PersistentRuleSetSchedule<S> {
    fn next_wakeup_in(&self) -> Option<Duration> {
        self.resource.next_wakeup_in()
    }

    fn take_due(&mut self) -> Result<bool, SchedulerError> {
        self.resource.take_due()
    }

    fn record_result(&mut self, succeeded: bool) -> Result<(), SchedulerError> {
        self.resource.record_result(succeeded)
    }
}

#[derive(Debug, Default)]
pub struct UnavailableUpdateSchedule;

impl RuntimeUpdateSchedule for UnavailableUpdateSchedule {
    fn configure(
        &mut self,
        _enabled: bool,
        _interval_hours: u16,
        _sources: &SourceConfig,
    ) -> Result<(), SchedulerError> {
        Ok(())
    }

    fn next_wakeup_in(&self) -> Option<Duration> {
        None
    }

    fn take_due(&mut self) -> Result<bool, SchedulerError> {
        Ok(false)
    }

    fn record_result(&mut self, _succeeded: bool) -> Result<(), SchedulerError> {
        Ok(())
    }
}

#[derive(Debug)]
pub struct PersistentUpdateSchedule<S: ScheduleStore> {
    engine: SchedulerEngine<S>,
    enabled: bool,
    keys: Vec<ScheduleKey>,
    pending: Vec<ScheduleKey>,
}

impl<S: ScheduleStore> PersistentUpdateSchedule<S> {
    pub fn load(store: S) -> Result<Self, SchedulerError> {
        Ok(Self {
            engine: SchedulerEngine::load(store, SchedulePolicy::default())?,
            enabled: false,
            keys: Vec::new(),
            pending: Vec::new(),
        })
    }
}

impl<S: ScheduleStore> RuntimeUpdateSchedule for PersistentUpdateSchedule<S> {
    fn configure(
        &mut self,
        enabled: bool,
        interval_hours: u16,
        sources: &SourceConfig,
    ) -> Result<(), SchedulerError> {
        let interval = i64::from(interval_hours) * 60 * 60;
        self.engine.set_policy(SchedulePolicy::new(
            interval,
            60 * 60,
            interval.min(24 * 60 * 60),
            (15 * 60).min(interval / 2),
        )?);
        self.enabled = enabled;
        self.pending.clear();
        self.keys = sources
            .active_sources()
            .map(|source| ScheduleKey::new(source.id().as_str()))
            .collect::<Result<Vec<_>, _>>()?;
        let now = wall_seconds()?;
        for key in &self.keys {
            self.engine.ensure(key.clone(), now)?;
        }
        Ok(())
    }

    fn next_wakeup_in(&self) -> Option<Duration> {
        if !self.enabled || self.keys.is_empty() || !self.pending.is_empty() {
            return None;
        }
        let now = wall_seconds().ok()?;
        self.keys
            .iter()
            .filter_map(|key| self.engine.record(key))
            .map(|record| {
                Duration::from_secs(
                    record
                        .next_run_wall_seconds()
                        .saturating_sub(now)
                        .try_into()
                        .unwrap_or(0),
                )
            })
            .min()
    }

    fn take_due(&mut self) -> Result<bool, SchedulerError> {
        if !self.enabled || self.keys.is_empty() || !self.pending.is_empty() {
            return Ok(false);
        }
        let due = self.engine.due(wall_seconds()?)?;
        self.pending = due
            .into_iter()
            .filter(|key| self.keys.contains(key))
            .collect();
        Ok(!self.pending.is_empty())
    }

    fn record_result(&mut self, succeeded: bool) -> Result<(), SchedulerError> {
        let now = wall_seconds()?;
        for key in self.pending.drain(..) {
            if succeeded {
                self.engine.mark_success(&key, now)?;
            } else {
                self.engine.mark_failure(&key, now)?;
            }
        }
        Ok(())
    }
}

fn wall_seconds() -> Result<i64, SchedulerError> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|duration| i64::try_from(duration.as_secs()).ok())
        .ok_or(SchedulerError::InvalidWallTime)
}
