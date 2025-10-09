use restart_that_thang::{Application, Config};

// Currently, the program runs on two total threads. One thread is utilized for the healthcheck
// endpoint while the main thread is utilized for core application logic.
#[tokio::main(worker_threads = 2)]
async fn main() -> anyhow::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    log::info!("`Restart That Thang` is running!");

    let config = Config::from_env()?;
    let application = Application::build(config)?;
    application.run().await?;

    Ok(())
}
