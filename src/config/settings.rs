use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

use crate::utils::paths;

/// Global GeoEngine settings stored in ~/.geoengine/settings.yaml
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Settings {
    /// Registered projects (name -> path)
    #[serde(default)]
    pub projects: HashMap<String, PathBuf>,

    /// Default GCP project ID
    pub gcp_project: Option<String>,

    /// Default GCP region
    pub gcp_region: Option<String>,

    /// Service port (when running)
    pub service_port: Option<u16>,

    /// Maximum concurrent containers for proxy service
    pub max_workers: Option<usize>,
}

impl Settings {
    /// Load settings from disk, creating default if not exists
    pub fn load() -> Result<Self> {
        let settings_path = paths::get_settings_file()?;

        if !settings_path.exists() {
            let settings = Self::default();
            settings.save()?;
            return Ok(settings);
        }

        let content = std::fs::read_to_string(&settings_path)
            .with_context(|| format!("Failed to read settings: {}", settings_path.display()))?;

        let settings: Settings = serde_yaml::from_str(&content)
            .with_context(|| "Failed to parse settings file")?;

        Ok(settings)
    }

    /// Save settings to disk
    pub fn save(&self) -> Result<()> {
        let settings_path = paths::get_settings_file()?;

        // Ensure directory exists
        if let Some(parent) = settings_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let content = serde_yaml::to_string(self)?;
        std::fs::write(&settings_path, content)?;

        Ok(())
    }

    /// Register a new project
    pub fn register_project(&mut self, name: &str, path: &PathBuf) -> Result<()> {
        // Check if name already exists with different path
        if let Some(existing) = self.projects.get(name) {
            if existing != path {
                anyhow::bail!(
                    "Project '{}' already registered at {}. Unregister it first.",
                    name,
                    existing.display()
                );
            }
        }

        self.projects.insert(name.to_string(), path.clone());
        Ok(())
    }

    /// Unregister a project
    pub fn unregister_project(&mut self, name: &str) -> Result<()> {
        if self.projects.remove(name).is_none() {
            anyhow::bail!("Project '{}' is not registered", name);
        }
        Ok(())
    }

    /// Get the path of a registered project
    pub fn get_project_path(&self, name: &str) -> Result<PathBuf> {
        // First check if it's a registered project name
        if let Some(path) = self.projects.get(name) {
            return Ok(path.clone());
        }

        // Check if it's a path
        let path = PathBuf::from(name);
        if path.exists() && path.join("geoengine.yaml").exists() {
            return Ok(path.canonicalize()?);
        }

        anyhow::bail!(
            "Project '{}' not found. Register it with: geoengine project register <path>",
            name
        )
    }

    /// List all registered projects
    pub fn list_projects(&self) -> Vec<(&str, &PathBuf)> {
        self.projects
            .iter()
            .map(|(k, v)| (k.as_str(), v))
            .collect()
    }
}
