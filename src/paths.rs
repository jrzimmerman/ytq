use anyhow::{Context, Result};
use etcetera::app_strategy::{AppStrategy, AppStrategyArgs};
use std::fs;
use std::path::PathBuf;

// Choose the Strategy based on OS
// Windows -> AppData\Roaming\ytq
#[cfg(target_os = "windows")]
use etcetera::app_strategy::Windows as Strategy;

// Mac & Linux -> ~/.config/ytq
#[cfg(not(target_os = "windows"))]
use etcetera::app_strategy::Xdg as Strategy;

pub struct AppPaths {
    pub config_file: PathBuf,
    pub queue_file: PathBuf,
    pub history_dir: PathBuf,
}

impl AppPaths {
    pub fn init() -> Result<Self> {
        let args = AppStrategyArgs {
            top_level_domain: "com".to_string(),
            author: "ytq".to_string(),
            app_name: "ytq".to_string(),
        };

        let strategy =
            Strategy::new(args).map_err(|_| anyhow::anyhow!("Could not determine system paths"))?;

        // Resolve base directories
        let config_dir = strategy.config_dir();
        let data_dir = strategy.data_dir();

        fs::create_dir_all(&config_dir)
            .with_context(|| format!("failed to create config dir: {}", config_dir.display()))?;

        fs::create_dir_all(&data_dir)
            .with_context(|| format!("failed to create data dir: {}", data_dir.display()))?;

        let history_dir = data_dir.join("history");
        fs::create_dir_all(&history_dir)
            .with_context(|| format!("failed to create history dir: {}", history_dir.display()))?;

        // Return the specific file paths we need
        Ok(Self {
            config_file: config_dir.join("config.json"),
            queue_file: data_dir.join("queue.json"),
            history_dir,
        })
    }
}
