use std::path::PathBuf;

use anyhow::{Context, Result};
use tauri::{path::BaseDirectory, Manager, Runtime};

pub const APP_DIR_NAME: &str = ".mihomo_switch";
#[cfg(test)]
pub const LEGACY_APP_DIR_NAME: &str = "MihomoSwitch";

pub fn runtime_dir() -> PathBuf {
    if let Ok(custom) = std::env::var("MIHOMO_MANAGER_HOME") {
        return PathBuf::from(custom);
    }

    if let Some(home) = dirs::home_dir() {
        return home.join(APP_DIR_NAME);
    }

    std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("runtime")
}

pub fn default_mihomo_path() -> PathBuf {
    runtime_dir().join("mihomo.exe")
}

pub fn ensure_runtime_dir() -> Result<PathBuf> {
    let dir = runtime_dir();
    std::fs::create_dir_all(&dir)
        .with_context(|| format!("create runtime dir: {}", dir.display()))?;

    Ok(dir)
}

pub fn settings_path() -> Result<PathBuf> {
    Ok(ensure_runtime_dir()?.join("settings.json"))
}

pub fn pool_config_path() -> Result<PathBuf> {
    Ok(ensure_runtime_dir()?.join("pool_config.yaml"))
}

pub fn latency_config_path() -> Result<PathBuf> {
    Ok(ensure_runtime_dir()?.join("latency_test.yaml"))
}

pub fn resolve_mihomo_path(configured_path: &str) -> PathBuf {
    let trimmed = configured_path.trim();
    if trimmed.is_empty() {
        default_mihomo_path()
    } else {
        PathBuf::from(trimmed)
    }
}

pub fn install_bundled_mihomo<R: Runtime>(app: &tauri::AppHandle<R>) -> Result<Option<PathBuf>> {
    let target = default_mihomo_path();
    if target.exists() {
        return Ok(None);
    }

    let source = app
        .path()
        .resolve("mihomo.exe", BaseDirectory::Resource)
        .context("resolve bundled mihomo resource")?;
    if !source.exists() {
        return Ok(None);
    }

    ensure_runtime_dir()?;
    std::fs::copy(&source, &target).with_context(|| {
        format!(
            "install bundled mihomo: {} -> {}",
            source.display(),
            target.display()
        )
    })?;
    Ok(Some(target))
}

#[cfg(test)]
pub fn legacy_runtime_dir() -> Option<PathBuf> {
    dirs::data_local_dir().map(|local_data| local_data.join(LEGACY_APP_DIR_NAME))
}

#[cfg(test)]
pub fn legacy_runtime_settings_path() -> Option<PathBuf> {
    legacy_runtime_dir().map(|dir| dir.join("settings.json"))
}

#[cfg(test)]
pub fn test_env_lock() -> &'static std::sync::Mutex<()> {
    static LOCK: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();
    LOCK.get_or_init(|| std::sync::Mutex::new(()))
}
