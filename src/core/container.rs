use super::docker::{docker_restart_container, DockerError};
use super::schedule::RestartSchedule;
use chrono::{DateTime, Utc};

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

// TODO: Unit tests would be good here to make sure timezone conversions are correct
