/// Represents the possible health states of a container.
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub enum HealthState {
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
