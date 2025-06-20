use anyhow::Context;

const DEFAULT_HOST: &str = "127.0.0.1";
const DEFAULT_PORT: &str = "8080";
const DEFAULT_POLLING_INTERVAL: u64 = 30;
const DEFAULT_STARTUP_DELAY: u64 = 30;

/// Configurable environmental variables
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    pub host: String,
    pub port: String,
    pub polling_interval: u64,
    pub startup_delay: u64,
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        Ok(Self {
            host: std::env::var("HOST").unwrap_or(DEFAULT_HOST.to_string()),
            port: std::env::var("PORT").unwrap_or(DEFAULT_PORT.to_string()),
            polling_interval: std::env::var("POLLING_INTERVAL")
                .unwrap_or(DEFAULT_POLLING_INTERVAL.to_string())
                .parse()
                .context("Failed to parse 'POLLING_INTERVAL'")?,
            startup_delay: std::env::var("STARTUP_DELAY")
                .unwrap_or(DEFAULT_STARTUP_DELAY.to_string())
                .parse()
                .context("Failed to parse 'STARTUP_DELAY'")?,
        })
    }
}
