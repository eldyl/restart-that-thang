use super::container::RestartSchedule;
use anyhow::Context;
use chrono::NaiveTime;

// RTT label strings to be parsed from container labels
// Short form
const RTT_ENABLED: &str = "rtt.enable=true";
const RTT_WATCH_RESTARTS: &str = "rtt.watch.restarts=";
const RTT_WATCH_UNHEALTHY: &str = "rtt.watch.unhealthy=";
const RTT_SCHEDULE_INTERVAL: &str = "rtt.schedule.interval=";
const RTT_SCHEDULE_TIME: &str = "rtt.schedule.time=";
// Long form
const RTT_ENABLED_LONG: &str = "restart-that-thang.enable=true";
const RTT_WATCH_RESTARTS_LONG: &str = "restart-that-thang.watch.restarts=";
const RTT_WATCH_UNHEALTHY_LONG: &str = "restart-that-thang.watch.unhealthy=";
const RTT_SCHEDULE_INTERVAL_LONG: &str = "restart-that-thang.schedule.interval=";
const RTT_SCHEDULE_TIME_LONG: &str = "restart-that-thang.schedule.time=";

type ContainerName = String;

/// The various label values that will be parsed for containers monitored by RTT.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RttLabels {
    /// If a container is in this list was restarted, this container will need to be restarted.
    pub watch_restarted: Vec<ContainerName>,

    /// Restart this container if a listed container becomes unhealthy.
    pub watch_unhealthy: Vec<ContainerName>,

    /// Container has a time based restart schedule.
    pub schedule: RestartSchedule,
}

impl RttLabels {
    /// Parses all RTT labels.
    pub fn parse(labels: &str) -> anyhow::Result<Option<Self>> {
        let is_enabled = labels.contains(RTT_ENABLED) || labels.contains(RTT_ENABLED_LONG);
        if !is_enabled {
            return Ok(None);
        }

        let watch_restarted = Self::parse_watch_restarted(labels);

        let watch_unhealthy = Self::parse_watch_unhealthy(labels);

        let schedule = Self::parse_schedule_interval(labels)
            .or_else(|_| Self::parse_schedule_time(labels))
            .unwrap_or(RestartSchedule::None);

        Ok(Some(Self {
            watch_restarted,
            watch_unhealthy,
            schedule,
        }))
    }

    /// Parses label that indicates which containers should be monitored for restarts
    pub(crate) fn parse_watch_restarted(labels: &str) -> Vec<String> {
        labels
            .split(RTT_WATCH_RESTARTS)
            .nth(1)
            .or_else(|| labels.split(RTT_WATCH_RESTARTS_LONG).nth(1))
            .map(|part| {
                part.split(",")
                    .take_while(|s| !s.trim().contains("="))
                    .map(String::from)
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Parses label that indicates which containers should be monitored for unhealthy state
    pub(crate) fn parse_watch_unhealthy(labels: &str) -> Vec<String> {
        labels
            .split(RTT_WATCH_UNHEALTHY)
            .nth(1)
            .or_else(|| labels.split(RTT_WATCH_UNHEALTHY_LONG).nth(1))
            .map(|part| {
                part.split(",")
                    .take_while(|s| !s.trim().contains("="))
                    .map(String::from)
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Parses the label used to set interval based restarts.
    pub(crate) fn parse_schedule_interval(labels: &str) -> anyhow::Result<RestartSchedule> {
        labels
            .split(RTT_SCHEDULE_INTERVAL)
            .nth(1)
            .or_else(|| labels.split(RTT_SCHEDULE_INTERVAL_LONG).nth(1))
            .map(|part| {
                part.split(",")
                    .take_while(|s| !s.trim().contains("="))
                    .collect::<Vec<_>>()
                    .join("")
            })
            .filter(|s| !s.is_empty())
            .map(|schedule_str| -> anyhow::Result<RestartSchedule>{
                const HOURS_IN_DAY: u64 = 24;
                const MINUTES_IN_HOUR: u64 = 60;
                const SECONDS_IN_MINUTE: u64 = 60;

                let duration_str = schedule_str.to_lowercase();
                let total_seconds = if let Some(day_str) = duration_str.strip_suffix("d") {
                    let days: u64 = day_str.parse().context("Invalid format for days")?;
                    days * HOURS_IN_DAY * MINUTES_IN_HOUR * SECONDS_IN_MINUTE
                } else if let Some(hour_str) = duration_str.strip_suffix("h") {
                    let hours: u64 = hour_str.parse().context("Invalid format for hours")?;
                    hours * MINUTES_IN_HOUR * SECONDS_IN_MINUTE
                } else if let Some(min_str) = duration_str.strip_suffix("m") {
                    let mins: u64 = min_str.parse().context("Invalid format for minutes")?;
                    mins * SECONDS_IN_MINUTE
                } else if let Some(sec_str) = duration_str.strip_suffix("s") {
                    let secs: u64 = sec_str.parse().context("Invalid format for seconds")?;
                    secs
                } else {
                    log::error!("Invalid format for interval schedule. Use '1d', '12h', '90m', or  '30s'. Read: '{duration_str}'");
                    return Ok(RestartSchedule::None);
                };

                Ok(RestartSchedule::Interval(tokio::time::Duration::from_secs(
                            total_seconds,
                )))
            }).unwrap_or(Ok(RestartSchedule::None))
    }

    /// Parses the label for containers to be restarted at a specific time of day.
    pub(crate) fn parse_schedule_time(labels: &str) -> anyhow::Result<RestartSchedule> {
        labels
            .split(RTT_SCHEDULE_TIME)
            .nth(1)
            .or_else(|| labels.split(RTT_SCHEDULE_TIME_LONG).nth(1))
            .map(|part| {
                part.split(",")
                    .take_while(|s| !s.trim().contains("="))
                    .collect::<Vec<_>>()
                    .join("")
            })
            .filter(|s| !s.is_empty())
            .map(|schedule_str| -> anyhow::Result<RestartSchedule> {
                let time =
                    NaiveTime::parse_from_str(&schedule_str, "%H:%M").with_context(|| {
                        format!("Invalid time format: {schedule_str}. Use HH:MM format -> '23:00'")
                    })?;

                Ok(RestartSchedule::DailyAt(time))
            })
            .unwrap_or(Ok(RestartSchedule::None))
    }
}
