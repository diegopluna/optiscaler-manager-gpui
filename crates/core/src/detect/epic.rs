use std::path::PathBuf;

use serde::Deserialize;

use crate::model::{Game, GameId, Store};

/// A single `.item` manifest written by the Epic Games Launcher.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct EpicManifest {
    #[serde(default)]
    display_name: String,
    #[serde(default)]
    install_location: String,
    #[serde(default)]
    launch_executable: String,
    #[serde(default)]
    app_name: String,
    /// Epic spells this one in camelCase, unlike every other key.
    #[serde(default, rename = "bIsIncompleteInstall")]
    b_is_incomplete_install: bool,
    /// Present on DLC/add-on entries, which share an install dir with the base
    /// game and would otherwise show up as duplicates.
    #[serde(default)]
    app_categories: Vec<String>,
}

impl EpicManifest {
    fn is_installable_game(&self) -> bool {
        !self.b_is_incomplete_install
            && !self.install_location.is_empty()
            && self
                .app_categories
                .iter()
                .any(|c| c.eq_ignore_ascii_case("games"))
    }
}

fn manifest_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();

    #[cfg(target_os = "windows")]
    {
        let program_data = std::env::var_os("PROGRAMDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(r"C:\ProgramData"));
        dirs.push(program_data.join(r"Epic\EpicGamesLauncher\Data\Manifests"));
    }

    // Heroic on Linux mirrors Epic's own manifest layout for games it installs
    // through Legendary, so the same reader covers it.
    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        dirs.push(home.join(".config/legendary/manifests"));
        dirs.push(home.join(".var/app/com.heroicgameslauncher.hgl/config/legendary/manifests"));
    }

    dirs
}

/// Detects Epic games from launcher manifests, plus Legendary's
/// `installed.json` on Linux.
pub fn detect() -> Vec<Game> {
    let mut games = Vec::new();

    for dir in manifest_dirs() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            if entry.path().extension().is_none_or(|ext| ext != "item") {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(entry.path()) else {
                continue;
            };
            if let Some(game) = game_from_manifest(&text) {
                games.push(game);
            }
        }
    }

    games.extend(legendary_games());

    // Epic writes one manifest per artifact; collapse anything that resolved to
    // the same install directory.
    games.sort_by_key(|a| a.title.to_lowercase());
    games.dedup_by(|a, b| a.install_dir == b.install_dir);
    games
}

/// Parses one Epic `.item` manifest.
pub fn game_from_manifest(text: &str) -> Option<Game> {
    let manifest: EpicManifest = serde_json::from_str(text).ok()?;
    if !manifest.is_installable_game() {
        return None;
    }

    let install_dir = PathBuf::from(&manifest.install_location);
    if !install_dir.is_dir() {
        return None;
    }

    let title = if manifest.display_name.trim().is_empty() {
        manifest.app_name.clone()
    } else {
        manifest.display_name.clone()
    };
    if title.trim().is_empty() {
        return None;
    }

    let key = if manifest.app_name.is_empty() {
        title.clone()
    } else {
        manifest.app_name.clone()
    };

    let mut game = Game::new(
        GameId::new(Store::Epic, key),
        title,
        Store::Epic,
        install_dir,
    );
    if !manifest.launch_executable.is_empty() {
        game.launch_exe = Some(game.install_dir.join(&manifest.launch_executable));
    }
    Some(game)
}

/// Legendary (and Heroic, which wraps it) records installs in a single JSON
/// map rather than per-game manifest files.
fn legendary_games() -> Vec<Game> {
    #[derive(Debug, Deserialize)]
    struct Installed {
        #[serde(default)]
        title: String,
        #[serde(default)]
        install_path: String,
        #[serde(default)]
        executable: String,
        #[serde(default)]
        app_name: String,
    }

    let Some(home) = std::env::var_os("HOME").map(PathBuf::from) else {
        return Vec::new();
    };

    let candidates = [
        home.join(".config/legendary/installed.json"),
        home.join(".config/heroic/legendaryConfig/legendary/installed.json"),
        home.join(".var/app/com.heroicgameslauncher.hgl/config/legendary/installed.json"),
    ];

    let mut games = Vec::new();
    for path in candidates {
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Ok(map) = serde_json::from_str::<std::collections::BTreeMap<String, Installed>>(&text)
        else {
            continue;
        };

        for (key, entry) in map {
            let install_dir = PathBuf::from(&entry.install_path);
            if entry.title.trim().is_empty() || !install_dir.is_dir() {
                continue;
            }
            let id_key = if entry.app_name.is_empty() {
                key
            } else {
                entry.app_name
            };
            let mut game = Game::new(
                GameId::new(Store::Epic, id_key),
                entry.title,
                Store::Epic,
                install_dir,
            );
            if !entry.executable.is_empty() {
                game.launch_exe = Some(game.install_dir.join(&entry.executable));
            }
            games.push(game);
        }
    }
    games
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest_json(dir: &std::path::Path, categories: &str, incomplete: bool) -> String {
        format!(
            r#"{{
                "DisplayName": "Control",
                "InstallLocation": {:?},
                "LaunchExecutable": "Control_DX12.exe",
                "AppName": "Cerulean",
                "bIsIncompleteInstall": {incomplete},
                "AppCategories": [{categories}]
            }}"#,
            dir.to_string_lossy(),
        )
    }

    #[test]
    fn parses_game_manifest() {
        let temp = tempfile::tempdir().unwrap();
        let json = manifest_json(temp.path(), r#""games", "applications""#, false);

        let game = game_from_manifest(&json).expect("game parsed");
        assert_eq!(game.title, "Control");
        assert_eq!(game.store, Store::Epic);
        assert_eq!(game.id.as_str(), "epic-cerulean");
        assert_eq!(game.launch_exe, Some(temp.path().join("Control_DX12.exe")));
    }

    #[test]
    fn skips_addons_and_incomplete_installs() {
        let temp = tempfile::tempdir().unwrap();

        let addon = manifest_json(temp.path(), r#""addons""#, false);
        assert!(game_from_manifest(&addon).is_none(), "DLC is not a game");

        let incomplete = manifest_json(temp.path(), r#""games""#, true);
        assert!(game_from_manifest(&incomplete).is_none());
    }
}
