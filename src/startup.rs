use crate::routes::health;
use crate::Config;
use crate::Controller;
use anyhow::Context;
use tokio::net::TcpListener;
use tokio::time::interval;

pub struct Application {
    host: String,
    port: String,
    startup_delay: u64,
    polling_interval: u64,
    pub controller: Controller,
}

impl Application {
    pub fn build(config: Config) -> anyhow::Result<Self> {
        Ok(Self {
            host: config.host,
            port: config.port,
            startup_delay: config.startup_delay,
            polling_interval: config.polling_interval,
            controller: Controller::new(),
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
        tokio::time::sleep(tokio::time::Duration::from_secs(self.startup_delay)).await;

        let mut interval_timer = interval(tokio::time::Duration::from_secs(self.polling_interval));

        let mut controller = self.controller;
        loop {
            interval_timer.tick().await;
            if let Err(e) = poll(&mut controller).await {
                log::error!("{e}")
            }
        }
    }
}

/// Core logic that runs during each cycle.
pub async fn poll(controller: &mut Controller) -> anyhow::Result<()> {
    controller.fetch_rtt_containers().await?;
    controller.fetch_and_set_start_times().await?;
    controller.sort()?;
    controller.restart_if_needed().await?;
    controller.cleanup_after_cycle();
    Ok(())
}
