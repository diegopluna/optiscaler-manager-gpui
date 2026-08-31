//! Detection for games installed by the Xbox app / Microsoft Store.
//!
//! Each drive that holds Xbox games carries a `.GamingRoot` file at its root,
//! naming the folder (usually `XboxGames`) that contains them. Every game is a
//! `<root>/<Game>/Content` directory holding the executable and a
//! `MicrosoftGame.config` describing it.

use std::path::{Path, PathBuf};

use quick_xml::Reader;
use quick_xml::events::Event;

use crate::model::{Game, GameId, Store};

/// Reads the folder name out of a `.GamingRoot` file.
///
/// Layout is a 4-byte `RGBX` magic, a 4-byte count, then a NUL-terminated
/// UTF-16LE relative path.
pub fn parse_gaming_root(bytes: &[u8]) -> Option<String> {
    if bytes.len() < 10 || &bytes[0..4] != b"RGBX" {
        return None;
    }

    let units: Vec<u16> = bytes[8..]
        .chunks_exact(2)
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
        .take_while(|&unit| unit != 0)
        .collect();

    let name = String::from_utf16(&units).ok()?;
    let name = name.trim_end_matches(['\\', '/']).trim();
    (!name.is_empty()).then(|| name.to_string())
}

/// Pulls the display name out of a `MicrosoftGame.config`, preferring the
/// shell name the Xbox app itself shows.
pub fn display_name_from_config(xml: &str) -> Option<String> {
    let mut reader = Reader::from_str(xml);
    let mut identity_name = None;

    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) => {
                let tag = e.name().as_ref().to_string();

                let attr = |name: &str| -> Option<String> {
                    e.attributes().flatten().find_map(|a| {
                        (a.key.as_ref() == name)
                            .then(|| a.value.trim().to_string())
                            .filter(|v| !v.is_empty())
                    })
                };

                if tag.eq_ignore_ascii_case("ShellVisuals")
                    && let Some(name) = attr("DefaultDisplayName")
                {
                    return Some(name);
                }
                if tag.eq_ignore_ascii_case("Identity") {
                    identity_name = attr("Name");
                }
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }

    identity_name
}

/// Roots to look for `.GamingRoot` on. Windows gets every drive letter; other
/// platforms have no Xbox installs, so detection is a no-op there.
fn drive_roots() -> Vec<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        ('A'..='Z')
            .map(|letter| PathBuf::from(format!("{letter}:\\")))
            .collect()
    }
    #[cfg(not(target_os = "windows"))]
    {
        Vec::new()
    }
}

pub fn detect() -> Vec<Game> {
    let mut games = Vec::new();
    for drive in drive_roots() {
        let marker = drive.join(".GamingRoot");
        let Ok(bytes) = std::fs::read(&marker) else {
            continue;
        };
        let Some(folder) = parse_gaming_root(&bytes) else {
            continue;
        };
        games.extend(games_in_root(&drive.join(folder)));
    }

    games.sort_by_key(|a| a.title.to_lowercase());
    games
}

/// Enumerates `<root>/<Game>/Content` directories.
pub fn games_in_root(root: &Path) -> Vec<Game> {
    let Ok(entries) = std::fs::read_dir(root) else {
        return Vec::new();
    };

    let mut games = Vec::new();
    for entry in entries.flatten() {
        let content = entry.path().join("Content");
        if !content.is_dir() {
            continue;
        }

        let folder_name = entry.file_name().to_string_lossy().to_string();
        let title = std::fs::read_to_string(content.join("MicrosoftGame.config"))
            .ok()
            .and_then(|xml| display_name_from_config(&xml))
            .or_else(|| {
                std::fs::read_to_string(content.join("appxmanifest.xml"))
                    .ok()
                    .and_then(|xml| display_name_from_config(&xml))
            })
            .unwrap_or_else(|| folder_name.clone());

        // The executable lives in Content, so that is also where OptiScaler goes.
        games.push(Game::new(
            GameId::new(Store::Xbox, &folder_name),
            title,
            Store::Xbox,
            content,
        ));
    }
    games
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gaming_root_bytes(folder: &str) -> Vec<u8> {
        let mut bytes = b"RGBX".to_vec();
        bytes.extend_from_slice(&1u32.to_le_bytes());
        for unit in folder.encode_utf16() {
            bytes.extend_from_slice(&unit.to_le_bytes());
        }
        bytes.extend_from_slice(&0u16.to_le_bytes());
        bytes
    }

    #[test]
    fn reads_folder_from_gaming_root() {
        assert_eq!(
            parse_gaming_root(&gaming_root_bytes("XboxGames\\")).as_deref(),
            Some("XboxGames")
        );
        assert_eq!(parse_gaming_root(b"not a gaming root"), None);
    }

    #[test]
    fn prefers_shell_display_name_over_identity() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
            <Game configVersion="1">
              <Identity Name="Bethesda.Starfield" Version="1.0.0.0" />
              <ShellVisuals DefaultDisplayName="Starfield" PublisherDisplayName="Bethesda" />
            </Game>"#;
        assert_eq!(display_name_from_config(xml).as_deref(), Some("Starfield"));

        let identity_only = r#"<Game><Identity Name="Bethesda.Starfield" /></Game>"#;
        assert_eq!(
            display_name_from_config(identity_only).as_deref(),
            Some("Bethesda.Starfield")
        );
    }

    #[test]
    fn finds_games_by_content_dir() {
        let temp = tempfile::tempdir().unwrap();
        let content = temp.path().join("Starfield/Content");
        std::fs::create_dir_all(&content).unwrap();
        std::fs::write(
            content.join("MicrosoftGame.config"),
            r#"<Game><ShellVisuals DefaultDisplayName="Starfield" /></Game>"#,
        )
        .unwrap();
        // A folder without Content is not a game.
        std::fs::create_dir_all(temp.path().join("NotAGame")).unwrap();

        let games = games_in_root(temp.path());
        assert_eq!(games.len(), 1);
        assert_eq!(games[0].title, "Starfield");
        assert_eq!(games[0].install_dir, content);
    }
}
