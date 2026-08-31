//! User settings, persisted as JSON next to the install manifests.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::model::GameId;
use crate::paths::config_dir;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Optional SteamGridDB API key, used for cover art on non-Steam games.
    pub steamgriddb_key: Option<String>,
    /// Per-game overrides for the directory OptiScaler is installed into,
    /// for the cases where the heuristic picks the wrong executable.
    pub exe_dir_overrides: BTreeMap<GameId, PathBuf>,
    /// Proxy DLL name chosen per game, remembered so updates reuse it.
    pub proxy_names: BTreeMap<GameId, String>,
    /// UI theme: "light", "dark", or absent to follow the system.
    pub theme: Option<String>,
    /// Set once the user has acknowledged the anti-cheat warning.
    pub anticheat_warning_acknowledged: bool,
    /// Games where the user turned DLSS inputs (Nvidia spoofing) off.
    /// Relevant on AMD/Intel GPUs only; absent means the default, on.
    pub dlss_inputs_disabled: Vec<GameId>,
    /// Game folders the user added by hand, one game each.
    pub manual_games: Vec<PathBuf>,
    /// Library folders whose subdirectories are scanned as game installs,
    /// covering stores without a dedicated scanner (GOG, EA, Ubisoft, ...).
    pub scan_folders: Vec<PathBuf>,
}

impl Settings {
    fn file() -> Result<PathBuf> {
        Ok(config_dir()?.join("settings.json"))
    }

    /// Loads settings, falling back to defaults when the file is missing or
    /// unreadable — a corrupt settings file should not stop the app starting.
    pub fn load() -> Self {
        match Self::file().and_then(|path| Self::load_from(&path)) {
            Ok(settings) => settings,
            Err(err) => {
                log::warn!("using default settings: {err:#}");
                Settings::default()
            }
        }
    }

    pub fn load_from(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Settings::default());
        }
        let text =
            std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        serde_json::from_str(&text).with_context(|| format!("parsing {}", path.display()))
    }

    pub fn save(&self) -> Result<()> {
        self.save_to(&Self::file()?)
    }

    pub fn save_to(&self, path: &Path) -> Result<()> {
        let json = serde_json::to_string_pretty(self)?;
        let temp = path.with_extension("json.part");
        std::fs::write(&temp, json).with_context(|| format!("writing {}", temp.display()))?;
        std::fs::rename(&temp, path)?;
        Ok(())
    }

    /// The SteamGridDB key, if the user has set a non-empty one.
    pub fn steamgriddb_key(&self) -> Option<&str> {
        self.steamgriddb_key
            .as_deref()
            .map(str::trim)
            .filter(|key| !key.is_empty())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Store;

    #[test]
    fn round_trips_through_disk() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("settings.json");

        let mut settings = Settings {
            steamgriddb_key: Some("  abc123  ".into()),
            ..Default::default()
        };
        settings
            .proxy_names
            .insert(GameId::new(Store::Steam, "1091500"), "dxgi.dll".into());
        settings.save_to(&path).unwrap();

        let loaded = Settings::load_from(&path).unwrap();
        assert_eq!(loaded.steamgriddb_key(), Some("abc123"), "key is trimmed");
        assert_eq!(
            loaded.proxy_names[&GameId::new(Store::Steam, "1091500")],
            PathBuf::from("dxgi.dll").to_string_lossy()
        );
    }

    #[test]
    fn missing_file_yields_defaults() {
        let temp = tempfile::tempdir().unwrap();
        let settings = Settings::load_from(&temp.path().join("nope.json")).unwrap();
        assert!(settings.steamgriddb_key().is_none());
    }

    #[test]
    fn blank_key_counts_as_unset() {
        let settings = Settings {
            steamgriddb_key: Some("   ".into()),
            ..Default::default()
        };
        assert!(settings.steamgriddb_key().is_none());
    }
}
