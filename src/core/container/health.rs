/// Represents the possible health states of a container.
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub enum ContainerHealth {
    Healthy,
    UnHealthy,
    Starting,
    None,
}

impl ContainerHealth {
    pub(crate) fn new(field: &str) -> ContainerHealth {
        match field.to_lowercase().as_str() {
            "healthy" => ContainerHealth::Healthy,
            "unhealthy" => ContainerHealth::UnHealthy,
            "starting" => ContainerHealth::Starting,
            _ => ContainerHealth::None,
        }
    }
}
