use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// The launcher/storefront a game was detected from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Store {
    Steam,
    Epic,
    Xbox,
    /// Added by hand through the Settings view.
    Manual,
}

impl Store {
    /// Name shown in the UI.
    pub fn label(self) -> &'static str {
        match self {
            Store::Steam => "Steam",
            Store::Epic => "Epic Games",
            Store::Xbox => "Xbox",
            Store::Manual => "Custom",
        }
    }

    /// Stable identifier used in [`GameId`]s and cache filenames. Kept separate
    /// from [`Store::label`] so renaming a label never invalidates saved data.
    pub fn slug(self) -> &'static str {
        match self {
            Store::Steam => "steam",
            Store::Epic => "epic",
            Store::Xbox => "xbox",
            Store::Manual => "manual",
        }
    }
}

/// Stable identity for a game across scans, used as the key for settings
/// overrides, artwork cache files and install manifests.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct GameId(String);

impl GameId {
    pub fn new(store: Store, key: impl AsRef<str>) -> Self {
        let key: String = key
            .as_ref()
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                    c.to_ascii_lowercase()
                } else {
                    '-'
                }
            })
            .collect();
        GameId(format!("{}-{}", store.slug(), key))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for GameId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// A detected game installation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Game {
    pub id: GameId,
    pub title: String,
    pub store: Store,
    /// Root of the install as reported by the launcher.
    pub install_dir: PathBuf,
    /// Executable the launcher associates with the game, when it tells us.
    pub launch_exe: Option<PathBuf>,
    /// Steam appid, used for artwork on the Steam CDN.
    pub steam_app_id: Option<u32>,
}

impl Game {
    pub fn new(id: GameId, title: impl Into<String>, store: Store, install_dir: PathBuf) -> Self {
        Game {
            id,
            title: title.into(),
            store,
            install_dir,
            launch_exe: None,
            steam_app_id: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn game_id_is_slugified_and_store_scoped() {
        let id = GameId::new(Store::Steam, "Cyberpunk 2077");
        assert_eq!(id.as_str(), "steam-cyberpunk-2077");

        let epic = GameId::new(Store::Epic, "Cyberpunk 2077");
        assert_ne!(id, epic);
    }
}
