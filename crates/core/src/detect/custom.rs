//! User-supplied game locations: individually added game folders, and scan
//! folders whose subdirectories are treated as game installs.
//!
//! This is the catch-all for every store the scanners do not cover — GOG,
//! EA App, Ubisoft Connect, Battle.net, itch, DRM-free installs. The title is
//! taken from the folder name, which is also what artwork lookup searches by.

use std::path::{Path, PathBuf};

use crate::model::{Game, GameId, Store};

/// How deep to look for an executable when deciding whether a subdirectory
/// is a game. Covers `Game/Binaries/Win64` style layouts.
const EXE_SEARCH_DEPTH: usize = 3;

fn dir_title(dir: &Path) -> Option<String> {
    let name = dir.file_name()?.to_string_lossy().trim().to_string();
    (!name.is_empty()).then_some(name)
}

fn contains_exe(dir: &Path, depth: usize) -> bool {
    if depth > EXE_SEARCH_DEPTH {
        return false;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return false;
    };

    let mut subdirs = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        match entry.file_type() {
            Ok(ft) if ft.is_file() => {
                if path
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("exe"))
                {
                    return true;
                }
            }
            Ok(ft) if ft.is_dir() => subdirs.push(path),
            _ => {}
        }
    }
    subdirs.iter().any(|sub| contains_exe(sub, depth + 1))
}

/// Games the user added one folder at a time. Folders that no longer exist
/// are skipped rather than shown as broken entries.
pub fn manual_games(dirs: &[PathBuf]) -> Vec<Game> {
    dirs.iter()
        .filter(|dir| dir.is_dir())
        .filter_map(|dir| {
            let title = dir_title(dir)?;
            Some(Game::new(
                GameId::new(Store::Manual, dir.to_string_lossy()),
                title,
                Store::Manual,
                dir.clone(),
            ))
        })
        .collect()
}

/// Treats every subdirectory of `root` that contains an executable as a game
/// install. Intended for library folders like `D:\Games` or GOG's default
/// install directory.
pub fn scan_folder(root: &Path) -> Vec<Game> {
    let Ok(entries) = std::fs::read_dir(root) else {
        return Vec::new();
    };

    let mut games = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !entry.file_type().is_ok_and(|ft| ft.is_dir()) {
            continue;
        }
        let Some(title) = dir_title(&path) else {
            continue;
        };
        // Hidden folders and download managers' scratch dirs are never games.
        if title.starts_with('.') || !contains_exe(&path, 0) {
            continue;
        }
        games.push(Game::new(
            GameId::new(Store::Manual, path.to_string_lossy()),
            title,
            Store::Manual,
            path,
        ));
    }
    games
}

/// All user-configured locations: individual games first, then scan folders.
pub fn detect(manual_dirs: &[PathBuf], scan_folders: &[PathBuf]) -> Vec<Game> {
    let mut games = manual_games(manual_dirs);
    for root in scan_folders {
        games.extend(scan_folder(root));
    }
    games
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manual_games_use_the_folder_name_and_skip_missing_dirs() {
        let temp = tempfile::tempdir().unwrap();
        let game = temp.path().join("Cyberpunk 2077");
        std::fs::create_dir_all(&game).unwrap();

        let games = manual_games(&[game.clone(), temp.path().join("gone")]);
        assert_eq!(games.len(), 1);
        assert_eq!(games[0].title, "Cyberpunk 2077");
        assert_eq!(games[0].store, Store::Manual);
        assert_eq!(games[0].install_dir, game);
    }

    #[test]
    fn scan_folder_keeps_subdirs_with_executables_only() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();

        // A GOG-style game with the exe nested two levels down.
        std::fs::create_dir_all(root.join("The Witcher 3/bin/x64")).unwrap();
        std::fs::write(root.join("The Witcher 3/bin/x64/witcher3.exe"), b"").unwrap();
        // A folder with no executable anywhere.
        std::fs::create_dir_all(root.join("Soundtracks")).unwrap();
        std::fs::write(root.join("Soundtracks/album.flac"), b"").unwrap();
        // A hidden folder with an exe must still be skipped.
        std::fs::create_dir_all(root.join(".cache")).unwrap();
        std::fs::write(root.join(".cache/tool.exe"), b"").unwrap();
        // A loose file at the root is not a game.
        std::fs::write(root.join("setup.exe"), b"").unwrap();

        let games = scan_folder(root);
        assert_eq!(games.len(), 1, "{games:?}");
        assert_eq!(games[0].title, "The Witcher 3");
    }

    #[test]
    fn ids_are_stable_per_path() {
        let temp = tempfile::tempdir().unwrap();
        let game = temp.path().join("Stray");
        std::fs::create_dir_all(&game).unwrap();

        let first = manual_games(&[game.clone()]);
        let second = manual_games(&[game]);
        assert_eq!(
            first[0].id, second[0].id,
            "same folder, same id across scans"
        );
    }
}
