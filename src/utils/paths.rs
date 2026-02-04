use anyhow::{Context, Result};
use std::path::PathBuf;

/// Get the GeoEngine configuration directory (~/.geoengine)
pub fn get_config_dir() -> Result<PathBuf> {
    let home = dirs::home_dir().context("Could not find home directory")?;
    let config_dir = home.join(".geoengine");
    std::fs::create_dir_all(&config_dir)?;
    Ok(config_dir)
}

/// Get the settings file path
pub fn get_settings_file() -> Result<PathBuf> {
    Ok(get_config_dir()?.join("settings.yaml"))
}

/// Get the PID file path for the proxy service
pub fn get_pid_file() -> Result<PathBuf> {
    Ok(get_config_dir()?.join("service.pid"))
}

/// Get the log file path for the proxy service
pub fn get_log_file() -> Result<PathBuf> {
    let logs_dir = get_config_dir()?.join("logs");
    std::fs::create_dir_all(&logs_dir)?;
    Ok(logs_dir.join("service.log"))
}

/// Get the jobs directory for temporary job data
pub fn get_jobs_dir() -> Result<PathBuf> {
    let jobs_dir = get_config_dir()?.join("jobs");
    std::fs::create_dir_all(&jobs_dir)?;
    Ok(jobs_dir)
}

/// Get temporary directory for file transfers
pub fn get_temp_dir() -> Result<PathBuf> {
    let temp_dir = get_config_dir()?.join("tmp");
    std::fs::create_dir_all(&temp_dir)?;
    Ok(temp_dir)
}
