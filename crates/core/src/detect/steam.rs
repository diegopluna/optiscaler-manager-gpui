use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::model::{Game, GameId, Store};
use crate::vdf::{self, Value};

/// Appids that are runtimes, redistributables or tools rather than games.
/// Steam lists these as ordinary apps in `appmanifest_*.acf`.
const NON_GAME_APP_IDS: &[u32] = &[
    228980,  // Steamworks Common Redistributables
    1070560, // Steam Linux Runtime 1.0 (scout)
    1391110, // Steam Linux Runtime 2.0 (soldier)
    1628350, // Steam Linux Runtime 3.0 (sniper)
    1493710, // Proton Experimental
    2180100, // Proton Hotfix
];

/// Steam install roots to probe when the registry is unavailable or wrong.
fn candidate_steam_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    let home = dirs_home();

    // Escape hatch for installs we cannot guess, and what the tests point at.
    if let Some(root) = std::env::var_os("OPTISCALER_STEAM_ROOT") {
        roots.push(PathBuf::from(root));
    }

    #[cfg(target_os = "windows")]
    {
        if let Some(path) = windows_registry_steam_path() {
            roots.push(path);
        }
        for drive in ["C:", "D:"] {
            roots.push(PathBuf::from(format!(r"{drive}\Program Files (x86)\Steam")));
            roots.push(PathBuf::from(format!(r"{drive}\Program Files\Steam")));
            roots.push(PathBuf::from(format!(r"{drive}\Steam")));
        }
    }

    #[cfg(target_os = "linux")]
    if let Some(home) = &home {
        roots.push(home.join(".local/share/Steam"));
        roots.push(home.join(".steam/steam"));
        roots.push(home.join(".steam/root"));
        roots.push(home.join(".var/app/com.valvesoftware.Steam/.local/share/Steam"));
    }

    #[cfg(target_os = "macos")]
    if let Some(home) = &home {
        roots.push(home.join("Library/Application Support/Steam"));
    }

    let _ = &home;
    roots
}

fn dirs_home() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

#[cfg(target_os = "windows")]
fn windows_registry_steam_path() -> Option<PathBuf> {
    // Written by the Steam client for the current user; the most reliable
    // source when Steam lives somewhere non-standard.
    let value: String = windows_registry::CURRENT_USER
        .open("Software\\Valve\\Steam")
        .ok()?
        .get_string("SteamPath")
        .ok()?;
    Some(PathBuf::from(value.replace('/', "\\")))
}

/// Detects Steam games across every configured library folder.
pub fn detect() -> Vec<Game> {
    let Some(steam_root) = candidate_steam_roots()
        .into_iter()
        .find(|root| root.join("steamapps").is_dir())
    else {
        log::debug!("no Steam installation found");
        return Vec::new();
    };

    log::info!("found Steam at {}", steam_root.display());

    let mut games = Vec::new();
    let mut seen = BTreeMap::new();

    for library in library_folders(&steam_root) {
        for game in games_in_library(&library) {
            // The same appid can appear in several libraries if a manifest was
            // left behind by a move; first one with a real directory wins.
            if seen.insert(game.steam_app_id, ()).is_none() {
                games.push(game);
            }
        }
    }

    games.sort_by_key(|a| a.title.to_lowercase());
    games
}

/// All library folders, including the Steam install itself.
pub fn library_folders(steam_root: &Path) -> Vec<PathBuf> {
    let mut libraries = vec![steam_root.to_path_buf()];

    let vdf_path = steam_root.join("steamapps/libraryfolders.vdf");
    let Ok(text) = std::fs::read_to_string(&vdf_path) else {
        return libraries;
    };

    let root = Value::Block(vdf::parse(&text));
    // Modern format: { "libraryfolders": { "0": { "path": "..." } } }.
    // Very old clients nested one level less, so accept both shapes.
    let entries = root
        .get(["libraryfolders"])
        .and_then(Value::as_block)
        .or_else(|| root.as_block());

    if let Some(entries) = entries {
        for value in entries.values() {
            let path = match value {
                Value::Block(_) => value.get_str(["path"]).map(PathBuf::from),
                // Pre-2021 clients stored "1" "D:\\SteamLibrary" directly.
                Value::String(s) if !s.is_empty() => Some(PathBuf::from(s)),
                Value::String(_) => None,
            };

            if let Some(path) = path
                && path.join("steamapps").is_dir()
                && !libraries.contains(&path)
            {
                libraries.push(path);
            }
        }
    }

    libraries
}

fn games_in_library(library: &Path) -> Vec<Game> {
    let steamapps = library.join("steamapps");
    let Ok(entries) = std::fs::read_dir(&steamapps) else {
        return Vec::new();
    };

    let mut games = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !name.starts_with("appmanifest_") || !name.ends_with(".acf") {
            continue;
        }

        let Ok(text) = std::fs::read_to_string(entry.path()) else {
            continue;
        };
        if let Some(game) = game_from_manifest(&text, &steamapps) {
            games.push(game);
        }
    }
    games
}

/// Builds a [`Game`] from the contents of an `appmanifest_*.acf`, skipping
/// tools and entries whose install directory is missing.
pub fn game_from_manifest(text: &str, steamapps: &Path) -> Option<Game> {
    let root = Value::Block(vdf::parse(text));

    let app_id: u32 = root.get_str(["AppState", "appid"])?.trim().parse().ok()?;
    if NON_GAME_APP_IDS.contains(&app_id) {
        return None;
    }

    let install_dir_name = root.get_str(["AppState", "installdir"])?;
    let install_dir = steamapps.join("common").join(install_dir_name);
    if !install_dir.is_dir() {
        return None;
    }

    let title = root
        .get_str(["AppState", "name"])
        .filter(|n| !n.trim().is_empty())
        .unwrap_or(install_dir_name)
        .to_string();

    let mut game = Game::new(
        GameId::new(Store::Steam, app_id.to_string()),
        title,
        Store::Steam,
        install_dir,
    );
    game.steam_app_id = Some(app_id);
    Some(game)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_library_paths_from_modern_vdf() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let other = root.join("D_SteamLibrary");
        std::fs::create_dir_all(root.join("steamapps")).unwrap();
        std::fs::create_dir_all(other.join("steamapps")).unwrap();

        std::fs::write(
            root.join("steamapps/libraryfolders.vdf"),
            format!(
                r#"
                "libraryfolders"
                {{
                    "0" {{ "path" "{}" }}
                    "1" {{ "path" "{}" }}
                    "2" {{ "path" "{}" }}
                }}
                "#,
                root.display(),
                other.display(),
                root.join("does-not-exist").display(),
            ),
        )
        .unwrap();

        let libraries = library_folders(root);
        assert_eq!(libraries.len(), 2, "missing library dirs are dropped");
        assert!(libraries.contains(&other));
    }

    #[test]
    fn builds_game_from_manifest() {
        let temp = tempfile::tempdir().unwrap();
        let steamapps = temp.path().join("steamapps");
        std::fs::create_dir_all(steamapps.join("common/Cyberpunk 2077")).unwrap();

        let manifest = r#"
            "AppState"
            {
                "appid"      "1091500"
                "name"       "Cyberpunk 2077"
                "installdir" "Cyberpunk 2077"
            }
        "#;

        let game = game_from_manifest(manifest, &steamapps).expect("game parsed");
        assert_eq!(game.title, "Cyberpunk 2077");
        assert_eq!(game.steam_app_id, Some(1091500));
        assert_eq!(game.store, Store::Steam);
        assert!(game.install_dir.ends_with("common/Cyberpunk 2077"));
    }

    #[test]
    fn skips_redistributables_and_missing_dirs() {
        let temp = tempfile::tempdir().unwrap();
        let steamapps = temp.path().join("steamapps");
        std::fs::create_dir_all(steamapps.join("common/Redist")).unwrap();

        let redist = r#""AppState" { "appid" "228980" "name" "Redist" "installdir" "Redist" }"#;
        assert!(game_from_manifest(redist, &steamapps).is_none());

        let missing = r#""AppState" { "appid" "42" "name" "Ghost" "installdir" "Ghost" }"#;
        assert!(game_from_manifest(missing, &steamapps).is_none());
    }
}
