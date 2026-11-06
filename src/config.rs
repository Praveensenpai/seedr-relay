//! Configuration management for seedr-relay.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

/// Application configuration settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Local destination path where completed downloads are saved.
    pub save_path: PathBuf,
    /// Whether to send notifications via tayori.
    pub notify: bool,
    /// Whether to automatically delete items from Seedr cloud after download.
    pub auto_clean_cloud: bool,
    /// Polling interval in seconds when watching the queue.
    pub poll_interval_secs: u64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            save_path: default_save_path(),
            notify: true,
            auto_clean_cloud: true,
            poll_interval_secs: 5,
        }
    }
}

/// Returns the configuration directory path: `~/.config/seedr-relay`.
#[must_use]
pub fn config_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("seedr-relay")
}

/// Returns the path to `config.toml`.
#[must_use]
pub fn config_path() -> PathBuf {
    config_dir().join("config.toml")
}

/// Default save directory: `~/Downloads/Seedr`.
#[must_use]
pub fn default_save_path() -> PathBuf {
    dirs::download_dir()
        .unwrap_or_else(|| dirs::home_dir().unwrap_or_else(|| PathBuf::from(".")))
        .join("Seedr")
}

/// Loads configuration from disk or creates default if missing.
///
/// # Errors
/// Returns an error if the config file cannot be read or parsed.
pub fn load_config() -> Result<Config> {
    let path = config_path();
    if !path.exists() {
        let default = Config::default();
        save_config(&default)?;
        return Ok(default);
    }

    let contents = fs::read_to_string(&path)
        .with_context(|| format!("failed to read config file at {}", path.display()))?;
    let config: Config = toml::from_str(&contents)
        .with_context(|| format!("failed to parse config file at {}", path.display()))?;
    Ok(config)
}

/// Saves configuration to disk.
///
/// # Errors
/// Returns an error if directory creation or file writing fails.
pub fn save_config(config: &Config) -> Result<()> {
    let path = config_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create directory {}", parent.display()))?;
    }
    let data = toml::to_string_pretty(config).context("failed to serialize config to TOML")?;
    fs::write(&path, data)
        .with_context(|| format!("failed to write config to {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_has_sensible_values() {
        let cfg = Config::default();
        assert!(cfg.notify);
        assert!(cfg.auto_clean_cloud);
        assert_eq!(cfg.poll_interval_secs, 5);
        assert!(cfg.save_path.ends_with("Seedr"));
    }
}
