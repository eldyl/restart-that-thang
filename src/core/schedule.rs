use anyhow::Context;
use chrono::{DateTime, Local, NaiveDateTime, NaiveTime, Utc};
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IntervalTime(Duration);

impl IntervalTime {
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

impl AsRef<Duration> for IntervalTime {
    fn as_ref(&self) -> &Duration {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DailyTime(NaiveTime);

impl DailyTime {
    pub fn new(time: NaiveTime) -> Self {
        Self(time)
    }

    pub fn next_restart_time(&self) -> anyhow::Result<DateTime<Utc>> {
        let now = Local::now();
        let today = now.date_naive();
        let next_local = NaiveDateTime::new(today, *self.as_ref())
            .and_local_timezone(Local)
            .single()
            .unwrap_or_else(|| {
                NaiveDateTime::new(today, *self.as_ref())
                    .and_local_timezone(Local)
                    .earliest()
                    .unwrap()
            });

        let mut next = next_local.with_timezone(&Utc);

        if next <= Utc::now() {
            next += chrono::Duration::days(1);
        }

        Ok(next)
    }
}

impl AsRef<NaiveTime> for DailyTime {
    fn as_ref(&self) -> &NaiveTime {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RestartSchedule {
    /// The interval of time between restarts if container is set to restart on an interval via a
    /// label.
    interval: Option<IntervalTime>,

    /// The daily time a container is scheduled to restart if a container is scheduled to restart
    /// at a specific time every day via a label.
    daily_at: Option<DailyTime>,
}

impl RestartSchedule {
    pub fn new(interval: Option<IntervalTime>, daily_at: Option<DailyTime>) -> Option<Self> {
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
