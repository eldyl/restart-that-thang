use crate::core::docker::{docker_restart_container, DockerError};
use anyhow::Context;
use chrono::{DateTime, Local, NaiveDateTime, NaiveTime, Utc};

/// Holds required information for docker containers with RTT labels.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Container {
    /// The name of the container.
    pub(crate) name: String,

    /// The health state of the container.
    pub(crate) health: HealthState,

    /// Containers that this container depends on.
    pub(crate) depends_on: Vec<String>,

    /// Containers that this container needs to be healthy.
    pub(crate) depends_on_healthy: Vec<String>,

    /// The last restart time of this container.
    pub(crate) start_time: Option<DateTime<Utc>>,

    /// The restart schedule of this container.
    pub(crate) restart_schedule: Option<RestartSchedule>,

    /// The next restart time for this container.
    pub(crate) next_restart_time: Option<DateTime<Utc>>,
}

impl Container {
    pub(crate) fn new(
        name: impl Into<String>,
        depends_on: Vec<String>,
        depends_on_healthy: Vec<String>,
        restart_schedule: Option<RestartSchedule>,
        next_restart_time: Option<DateTime<Utc>>,
    ) -> Self {
        Self {
            name: name.into(),
            health: HealthState::None,
            depends_on,
            depends_on_healthy,
            start_time: None,
            restart_schedule,
            next_restart_time,
        }
    }

    /// Sets the health state of the container.
    pub(crate) fn set_health(&mut self, field: &str) {
        self.health = HealthState::parse(field);
    }

    /// Sets the start time of the container which is parsed from docker-cli output.
    pub(crate) fn set_start_time(&mut self, start_time: DateTime<Utc>) {
        self.start_time = Some(start_time);
    }

    /// Sets the next restart time for a container based on its schedule value. All times are
    /// converted to UTC.
    pub(crate) fn set_next_restart_time(&mut self) -> anyhow::Result<()> {
        // Determine if restart schedule is an interval or a time of day
        self.next_restart_time = match &self.restart_schedule {
            Some(schedule) => schedule.calc_next_restart_time()?,
            None => None,
        };
        Ok(())
    }

    /// Restart container with the docker-cli.
    pub(crate) async fn restart(&self) -> Result<(), DockerError> {
        docker_restart_container(&self.name).await
    }

    /// Determines if a container needs to be restarted based on the next restart time that was
    /// set.
    pub(crate) fn needs_scheduled_restart(&self) -> bool {
        log::debug!(
            "Checking if scheduled restart should occurr for {}",
            self.name
        );
        if let Some(next_restart_time) = self.next_restart_time {
            Utc::now() >= next_restart_time
        } else {
            false
        }
    }
}

/// Represents the possible health states of a container.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum HealthState {
    Healthy,
    UnHealthy,
    Starting,
    None,
}

impl HealthState {
    pub(crate) fn parse(field: &str) -> HealthState {
        match field.to_lowercase().as_str() {
            "healthy" => HealthState::Healthy,
            "unhealthy" => HealthState::UnHealthy,
            "starting" => HealthState::Starting,
            _ => HealthState::None,
        }
    }
}

/// Represents the various arguments that can be provided as a label for scheduling restarts by
/// time of day or on an interval.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RestartSchedule {
    Interval(tokio::time::Duration),
    DailyAt(NaiveTime),
    None,
}

impl RestartSchedule {
    /// Calculates the next restart time for a container.
    pub(crate) fn calc_next_restart_time(&self) -> anyhow::Result<Option<DateTime<Utc>>> {
        // Determine if restart schedule is an interval or a time of day
        match self {
            RestartSchedule::None => Ok(None),
            RestartSchedule::Interval(duration) => {
                let chrono_duration =
                    chrono::Duration::from_std(*duration).context("Duration conversion failed")?;
                let next = Utc::now() + chrono_duration;

                Ok(Some(next))
            }
            RestartSchedule::DailyAt(target_time) => {
                let now = Local::now();
                let today = now.date_naive();
                let next_local = NaiveDateTime::new(today, *target_time)
                    .and_local_timezone(Local)
                    .single()
                    .unwrap_or_else(|| {
                        NaiveDateTime::new(today, *target_time)
                            .and_local_timezone(Local)
                            .earliest()
                            .unwrap()
                    });

                let mut next = next_local.with_timezone(&Utc);

                if next <= Utc::now() {
                    next += chrono::Duration::days(1);
                }

                Ok(Some(next))
            }
        }
    }
}
