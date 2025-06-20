use anyhow::Context;
use restart_that_thang::routes::health;
use restart_that_thang::{run, Config, Controller};
use tokio::net::TcpListener;
use tokio::time::interval;

// Currently, the program runs on two total threads. One thread is utilized for the healthcheck
// endpoint while the main thread is utilized for core application logic.
#[tokio::main(worker_threads = 2)]
async fn main() -> anyhow::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    log::info!("`Restart That Thang` is running!");

    let config = Config::from_env()?;

    let listener = TcpListener::bind(format!("{}:{}", config.host, config.port))
        .await
        .with_context(|| {
            format!(
                "Failed to create tcp listener on {}:{}",
                config.host, config.port
            )
        })?;

    // Spawn thread for health check endpoint
    tokio::spawn(health(listener));

    // Initial startup delay before loop cycle starts
    tokio::time::sleep(tokio::time::Duration::from_secs(config.startup_delay)).await;

    let mut interval_timer = interval(tokio::time::Duration::from_secs(config.polling_interval));

    let mut controller = Controller::new();

    loop {
        interval_timer.tick().await;
        if let Err(e) = run(&mut controller).await {
            log::error!("{e}")
        }
    }
}
