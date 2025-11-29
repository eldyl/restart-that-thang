use anyhow::Context;
use chrono::{DateTime, Utc};
use std::time::Duration;

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct ScheduleInterval(Duration);

impl ScheduleInterval {
    pub fn new(duration: Duration) -> Self {
        Self(duration)
    }

    pub fn from_secs(secs: u64) -> Self {
        let duration = Duration::from_secs(secs);
        Self::new(duration)
    }

    pub fn next_restart_time(&self) -> anyhow::Result<DateTime<Utc>> {
        let chrono_duration =
            chrono::Duration::from_std(*self.as_ref()).context("Duration conversion failed")?;
        let next = Utc::now() + chrono_duration;
        Ok(next)
    }
}

impl AsRef<Duration> for ScheduleInterval {
    fn as_ref(&self) -> &Duration {
        &self.0
    }
}
