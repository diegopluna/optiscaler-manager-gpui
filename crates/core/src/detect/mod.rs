pub mod custom;
pub mod epic;
pub mod steam;
pub mod steam_apptype;
pub mod xbox;

use std::collections::HashSet;
use std::path::PathBuf;

use crate::model::{Game, Store};

/// Scans every supported storefront plus the user's own locations and returns
/// the merged catalog, sorted by title. Failures in one source never stop the
/// others.
///
/// `manual_dirs` are individually added game folders; `scan_folders` are
/// library folders whose subdirectories count as games.
pub fn detect_all(manual_dirs: &[PathBuf], scan_folders: &[PathBuf]) -> Vec<Game> {
    // Store scanners run first so that when a user's scan folder overlaps a
    // store library, the store's richer entry (real title, Steam appid for
    // artwork) wins the dedup below.
    let mut games = Vec::new();
    games.extend(steam::detect());
    games.extend(epic::detect());
    games.extend(xbox::detect());
    games.extend(custom::detect(manual_dirs, scan_folders));

    merge(games)
}

/// Drops entries that point at a directory already claimed by an earlier
/// entry, then sorts by title. A game owned on two stores is genuinely two
/// installs, but the same folder showing up twice is not.
fn merge(games: Vec<Game>) -> Vec<Game> {
    let mut seen: HashSet<PathBuf> = HashSet::new();
    let mut merged: Vec<Game> = Vec::new();

    for game in games {
        let key = game
            .install_dir
            .canonicalize()
            .unwrap_or_else(|_| game.install_dir.clone());
        if seen.insert(key) {
            merged.push(game);
        }
    }

    merged.sort_by(|a, b| {
        a.title
            .to_lowercase()
            .cmp(&b.title.to_lowercase())
            .then_with(|| a.install_dir.cmp(&b.install_dir))
    });
    merged
}

/// True when a manual entry duplicates nothing — exposed so the UI can warn
/// before adding a folder a store scanner already covers.
pub fn is_store_detected(games: &[Game], dir: &std::path::Path) -> bool {
    let canonical = dir.canonicalize().unwrap_or_else(|_| dir.to_path_buf());
    games.iter().any(|game| {
        game.store != Store::Manual
            && game
                .install_dir
                .canonicalize()
                .unwrap_or_else(|_| game.install_dir.clone())
                == canonical
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::GameId;

    #[test]
    fn merge_prefers_the_earlier_richer_entry_for_the_same_folder() {
        let temp = tempfile::tempdir().unwrap();
        let dir = temp.path().join("ELDEN RING");
        std::fs::create_dir_all(&dir).unwrap();

        let mut steam = Game::new(
            GameId::new(Store::Steam, "1245620"),
            "ELDEN RING",
            Store::Steam,
            dir.clone(),
        );
        steam.steam_app_id = Some(1245620);
        let manual = Game::new(
            GameId::new(Store::Manual, dir.to_string_lossy()),
            "ELDEN RING",
            Store::Manual,
            dir,
        );

        let merged = merge(vec![steam, manual]);
        assert_eq!(merged.len(), 1, "one folder, one entry");
        assert_eq!(merged[0].store, Store::Steam, "store entry wins");
        assert_eq!(merged[0].steam_app_id, Some(1245620));
    }

    #[test]
    fn merge_keeps_distinct_folders_even_with_the_same_title() {
        let temp = tempfile::tempdir().unwrap();
        let a = temp.path().join("a/Portal");
        let b = temp.path().join("b/Portal");
        std::fs::create_dir_all(&a).unwrap();
        std::fs::create_dir_all(&b).unwrap();

        let merged = merge(vec![
            Game::new(GameId::new(Store::Steam, "400"), "Portal", Store::Steam, a),
            Game::new(GameId::new(Store::Manual, "x"), "Portal", Store::Manual, b),
        ]);
        assert_eq!(merged.len(), 2);
    }
}
