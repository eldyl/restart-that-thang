use crate::core::{
    fetch_and_set_start_times, fetch_rtt_containers, restart_if_needed, rtt_containers_topo_sort,
};
use crate::routes::health;
use crate::Config;
use crate::State;
use anyhow::Context;
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::time::interval;

pub struct Application {
    host: String,
    port: String,
    startup_delay: u64,
    polling_interval: u64,
    pub state: State,
}

impl Application {
    pub fn build(config: Config) -> anyhow::Result<Self> {
        Ok(Self {
            host: config.host,
            port: config.port,
            startup_delay: config.startup_delay,
            polling_interval: config.polling_interval,
            state: State::default(),
        })
    }

    async fn spawn_health_check_endpoint(&self) -> anyhow::Result<()> {
        let listener = TcpListener::bind(format!("{}:{}", self.host, self.port))
            .await
            .with_context(|| {
                format!(
                    "Failed to create tcp listener on {}:{}",
                    self.host, self.port
                )
            })?;

        // Spawn thread for health check endpoint
        tokio::spawn(health(listener));
        Ok(())
    }

    pub async fn run(self) -> anyhow::Result<()> {
        self.spawn_health_check_endpoint().await?;

        // Initial startup delay before loop cycle starts
        tokio::time::sleep(Duration::from_secs(self.startup_delay)).await;

        let mut interval_timer = interval(Duration::from_secs(self.polling_interval));

        let mut state = self.state;
        loop {
            interval_timer.tick().await;
            if let Err(e) = poll(&mut state).await {
                log::error!("{e}")
            }
        }
    }
}

/// Core logic that runs during each cycle.
pub async fn poll(app_state: &mut State) -> anyhow::Result<()> {
    let (container_names, containers) = match fetch_rtt_containers(app_state).await {
        Ok((container_names, containers)) => (container_names, containers),
        Err(e) => {
            log::warn!("{e}");
            return Ok(());
        }
    };

    let containers = match fetch_and_set_start_times(container_names, containers).await {
        Ok(containers) => containers,
        Err(e) => {
            log::warn!("{e}");
            return Ok(());
        }
    };

    let sorted_container_names = rtt_containers_topo_sort(&containers)?;
    let containers = restart_if_needed(containers, &sorted_container_names).await?;

    app_state.containers = containers;

    Ok(())
}
