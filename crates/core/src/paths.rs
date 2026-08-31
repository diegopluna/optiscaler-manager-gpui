use std::path::PathBuf;

use anyhow::{Context, Result};
use directories::ProjectDirs;

fn project_dirs() -> Result<ProjectDirs> {
    ProjectDirs::from("dev", "OptiScalerManager", "OptiScaler Manager")
        .context("could not determine platform data directories")
}

/// Directory for settings and install manifests. Created on demand.
pub fn config_dir() -> Result<PathBuf> {
    let dir = project_dirs()?.config_dir().to_path_buf();
    std::fs::create_dir_all(&dir)
        .with_context(|| format!("creating config dir {}", dir.display()))?;
    Ok(dir)
}

/// Directory for downloaded artwork and OptiScaler release archives.
pub fn cache_dir() -> Result<PathBuf> {
    let dir = project_dirs()?.cache_dir().to_path_buf();
    std::fs::create_dir_all(&dir)
        .with_context(|| format!("creating cache dir {}", dir.display()))?;
    Ok(dir)
}

/// `<cache>/artwork`, where resolved banner images are stored.
pub fn artwork_dir() -> Result<PathBuf> {
    let dir = cache_dir()?.join("artwork");
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// `<cache>/releases`, where downloaded OptiScaler archives are kept so
/// installing into several games only downloads once.
pub fn releases_dir() -> Result<PathBuf> {
    let dir = cache_dir()?.join("releases");
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}
