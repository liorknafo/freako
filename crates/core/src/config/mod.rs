pub mod types;

use std::path::Path;

use anyhow::{Context, Result};
use types::AppConfig;

/// Load config from the persisted config file, or return defaults.
pub fn load_config() -> Result<AppConfig> {
    let path = AppConfig::config_path();
    if path.exists() {
        load_from_file(&path)
    } else {
        Ok(AppConfig::default())
    }
}

fn load_from_file(path: &Path) -> Result<AppConfig> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read config file: {}", path.display()))?;
    let config: AppConfig =
        toml::from_str(&content).with_context(|| "Failed to parse config file")?;
    Ok(config)
}

/// Save config to the persisted config file.
pub fn save_config(config: &AppConfig) -> Result<()> {
    let path = AppConfig::config_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create config dir: {}", parent.display()))?;
    }
    let content =
        toml::to_string_pretty(config).with_context(|| "Failed to serialize config")?;
    std::fs::write(&path, content)
        .with_context(|| format!("Failed to write config: {}", path.display()))?;
    Ok(())
}
