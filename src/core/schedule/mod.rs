use chrono::{DateTime, Utc};

mod daily;
mod interval;

pub(crate) use daily::ScheduleDaily;
pub(crate) use interval::ScheduleInterval;

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub(crate) struct RestartSchedule {
    /// The interval of time between restarts if container is set to restart on an interval via a
    /// label.
    interval: Option<ScheduleInterval>,

    /// The daily time a container is scheduled to restart if a container is scheduled to restart
    /// at a specific time every day via a label.
    daily_at: Option<ScheduleDaily>,
}

impl RestartSchedule {
    pub fn new(
        interval: Option<ScheduleInterval>,
        daily_at: Option<ScheduleDaily>,
    ) -> Option<Self> {
        if interval.is_none() && daily_at.is_none() {
            return None;
        }
        Some(Self { interval, daily_at })
    }

    /// Calculates the next restart time for a container.
    pub(crate) fn calc_next_restart_time(&self) -> anyhow::Result<Option<DateTime<Utc>>> {
        let interval_time = self
            .interval
            .as_ref()
            .map(|t| t.next_restart_time())
            .transpose()?;
        let daily_time = self
            .daily_at
            .as_ref()
            .map(|t| t.next_restart_time())
            .transpose()?;

        Ok(match (interval_time, daily_time) {
            (Some(i), None) => Some(i),
            (None, Some(d)) => Some(d),
            (Some(i), Some(d)) => Some(i.min(d)),
            (None, None) => None,
        })
    }
}
