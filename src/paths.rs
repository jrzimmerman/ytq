use anyhow::{Context, Result};
use etcetera::app_strategy::{AppStrategy, AppStrategyArgs};
use std::env;
use std::fs;
use std::path::PathBuf;

/// Overrides the config directory. Useful for sandboxing and for tests, which
/// must never touch the invoking user's real queue.
const CONFIG_DIR_ENV: &str = "YTQ_CONFIG_DIR";

/// Overrides the data directory (database, categories, history).
const DATA_DIR_ENV: &str = "YTQ_DATA_DIR";

// Choose the Strategy based on OS
// Windows -> AppData\Roaming\ytq
#[cfg(target_os = "windows")]
use etcetera::app_strategy::Windows as Strategy;

// Mac & Linux -> ~/.config/ytq
#[cfg(not(target_os = "windows"))]
use etcetera::app_strategy::Xdg as Strategy;

pub struct AppPaths {
    pub config_file: PathBuf,
    pub history_dir: PathBuf,
    pub db_file: PathBuf,
    pub categories_file: PathBuf,
}

impl AppPaths {
    pub fn init() -> Result<Self> {
        // Resolve base directories, letting the environment take precedence
        // over the platform defaults.
        let config_override = env_dir(CONFIG_DIR_ENV);
        let data_override = env_dir(DATA_DIR_ENV);

        let (config_dir, data_dir) = match (config_override, data_override) {
            (Some(config_dir), Some(data_dir)) => (config_dir, data_dir),
            (config_override, data_override) => {
                let args = AppStrategyArgs {
                    top_level_domain: "com".to_string(),
                    author: "ytq".to_string(),
                    app_name: "ytq".to_string(),
                };
                let strategy = Strategy::new(args)
                    .map_err(|_| anyhow::anyhow!("could not determine system paths"))?;
                (
                    config_override.unwrap_or_else(|| strategy.config_dir()),
                    data_override.unwrap_or_else(|| strategy.data_dir()),
                )
            }
        };

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
            db_file: data_dir.join("ytq.db"),
            categories_file: data_dir.join("categories.json"),
            history_dir,
        })
    }
}

/// Reads a directory override from the environment. Unset and empty values both
/// fall back to the platform default rather than resolving to the current
/// working directory.
fn env_dir(key: &str) -> Option<PathBuf> {
    let value = env::var_os(key)?;
    if value.is_empty() {
        return None;
    }
    Some(PathBuf::from(value))
}

#[cfg(test)]
mod tests {
    use super::*;

    // Setting environment variables is `unsafe` in edition 2024 and this crate
    // forbids unsafe code, so the "override is honored" case is covered by the
    // subprocess tests in `tests/cli.rs` instead.
    #[test]
    fn env_dir_is_none_when_unset() {
        assert_eq!(env_dir("YTQ_DEFINITELY_UNSET_VARIABLE_NAME"), None);
    }
}
