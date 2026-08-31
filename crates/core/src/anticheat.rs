//! Detects anti-cheat systems shipped with a game.
//!
//! This matters because OptiScaler works by loading itself into the game as a
//! proxy DLL, which is exactly the behaviour anti-cheat software is built to
//! catch. Using it in a protected online game risks a ban, so the catalog
//! flags anything it finds before the user installs.
//!
//! The signatures are ported from SteamDB's `FileDetectionRuleSets`
//! (<https://github.com/SteamDatabase/FileDetectionRuleSets>), which is the
//! maintained list Steam's own store pages are built from.
//!
//! A clean scan is *not* proof a game is safe: server-side systems, and Valve
//! Anti-Cheat in particular, leave nothing on disk to find. Callers must
//! present a negative result as "nothing found", never as "safe".

use std::path::{Path, PathBuf};

/// How a file or directory identifies an anti-cheat system.
#[derive(Debug, Clone, Copy)]
enum Pattern {
    /// An exact file name, compared case-insensitively.
    File(&'static str),
    /// An exact directory name.
    Dir(&'static str),
    /// Any file with this extension.
    Extension(&'static str),
}

/// Anti-cheat systems and the files that give them away.
const SIGNATURES: &[(&str, &[Pattern])] = &[
    (
        "Easy Anti-Cheat",
        &[
            Pattern::File("EasyAntiCheat_Setup.exe"),
            Pattern::File("EasyAntiCheat_EOS_Setup.exe"),
            Pattern::File("EasyAntiCheat.dll"),
            Pattern::File("EasyAntiCheat_x64.dll"),
            Pattern::File("eac_server64.dll"),
            Pattern::Dir("EasyAntiCheat"),
            Pattern::Dir("EasyAntiCheat_EOS"),
        ],
    ),
    (
        "BattlEye",
        &[
            Pattern::File("BEService.exe"),
            Pattern::File("BEService_x64.exe"),
            Pattern::Dir("BattlEye"),
        ],
    ),
    (
        "EA AntiCheat",
        &[Pattern::File("EAAntiCheat.Installer.exe")],
    ),
    (
        "Anti-Cheat Expert",
        &[
            Pattern::Dir("AntiCheatExpert"),
            Pattern::Dir("AceAntibotClient"),
        ],
    ),
    (
        "PunkBuster",
        &[
            Pattern::File("PnkBstrA.exe"),
            Pattern::File("pbsvc.exe"),
            Pattern::File("pbsv.dll"),
            Pattern::Dir("PunkBuster"),
        ],
    ),
    ("EQU8", &[Pattern::File("equ8_conf.json")]),
    ("nProtect GameGuard", &[Pattern::File("gameguard.des")]),
    (
        "BlackCipher",
        &[
            Pattern::File("BlackCall.aes"),
            Pattern::File("BlackCall64.aes"),
            Pattern::File("BlackCat64.sys"),
        ],
    ),
    (
        "HackShield",
        &[Pattern::File("HSInst.dll"), Pattern::Dir("HShield")],
    ),
    (
        "NetEase Anti-Cheat Experts",
        &[
            Pattern::File("NeacSafe64.sys"),
            Pattern::File("NeacSafe64_ex.sys"),
        ],
    ),
    ("NetEase Yidun", &[Pattern::File("NEP2.dll")]),
    ("Ricochet", &[Pattern::File("Randgrid.sys")]),
    ("TenProtect", &[Pattern::File("TP3Helper.exe")]),
    (
        "AnyBrain",
        &[
            Pattern::File("anybrainSDK.dll"),
            Pattern::File("Cerebro.dll"),
        ],
    ),
    (
        "Fredaikis Anti-Cheat",
        &[Pattern::Dir("FredaikisAntiCheat")],
    ),
    ("XIGNCODE3", &[Pattern::Extension("xem")]),
];

/// Anti-cheat software found in a game, and the file that proves it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Detection {
    pub name: &'static str,
    /// Path of the matching file, relative to the game's install directory.
    pub evidence: PathBuf,
}

/// Anti-cheat files sit near the top of an install, so the walk stays shallow
/// rather than crawling an entire multi-gigabyte game.
const MAX_DEPTH: usize = 3;
const MAX_ENTRIES: usize = 4000;

/// Scans a game directory for known anti-cheat software.
///
/// An empty result means nothing was found on disk, which does not rule out
/// server-side anti-cheat such as VAC.
pub fn scan(install_dir: &Path) -> Vec<Detection> {
    let mut found: Vec<Detection> = Vec::new();
    let mut budget = MAX_ENTRIES;
    walk(install_dir, install_dir, 0, &mut budget, &mut found);
    found.sort_by_key(|detection| detection.name);
    found
}

fn walk(dir: &Path, base: &Path, depth: usize, budget: &mut usize, found: &mut Vec<Detection>) {
    if depth > MAX_DEPTH || *budget == 0 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };

    for entry in entries.flatten() {
        if *budget == 0 {
            return;
        }
        *budget -= 1;

        let path = entry.path();
        let is_dir = entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false);
        let name = entry.file_name();
        let name = name.to_string_lossy();

        if let Some(vendor) = match_signature(&name, is_dir)
            && !found.iter().any(|d| d.name == vendor)
        {
            found.push(Detection {
                name: vendor,
                evidence: path.strip_prefix(base).unwrap_or(&path).to_path_buf(),
            });
        }

        if is_dir {
            walk(&path, base, depth + 1, budget, found);
        }
    }
}

/// The anti-cheat a file or directory name identifies, if any.
fn match_signature(name: &str, is_dir: bool) -> Option<&'static str> {
    for (vendor, patterns) in SIGNATURES {
        for pattern in *patterns {
            let hit = match pattern {
                Pattern::File(expected) => !is_dir && name.eq_ignore_ascii_case(expected),
                Pattern::Dir(expected) => is_dir && name.eq_ignore_ascii_case(expected),
                Pattern::Extension(ext) => {
                    !is_dir
                        && name
                            .rsplit_once('.')
                            .is_some_and(|(_, found)| found.eq_ignore_ascii_case(ext))
                }
            };
            if hit {
                return Some(vendor);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_easy_anti_cheat_by_file() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("EasyAntiCheat_x64.dll"), b"").unwrap();

        let found = scan(temp.path());
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].name, "Easy Anti-Cheat");
        assert_eq!(found[0].evidence, PathBuf::from("EasyAntiCheat_x64.dll"));
    }

    #[test]
    fn finds_battleye_nested_in_a_subdirectory() {
        let temp = tempfile::tempdir().unwrap();
        let dir = temp.path().join("Game/Binaries/Win64");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("BEService_x64.exe"), b"").unwrap();

        let found = scan(temp.path());
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].name, "BattlEye");
    }

    #[test]
    fn matches_directories_and_is_case_insensitive() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(temp.path().join("easyanticheat")).unwrap();

        let found = scan(temp.path());
        assert_eq!(found[0].name, "Easy Anti-Cheat");
    }

    #[test]
    fn reports_each_system_once_but_several_together() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("EasyAntiCheat.dll"), b"").unwrap();
        std::fs::write(temp.path().join("EasyAntiCheat_Setup.exe"), b"").unwrap();
        std::fs::write(temp.path().join("BEService.exe"), b"").unwrap();

        let found = scan(temp.path());
        let names: Vec<&str> = found.iter().map(|d| d.name).collect();
        assert_eq!(names, vec!["BattlEye", "Easy Anti-Cheat"]);
    }

    #[test]
    fn ordinary_game_files_are_not_flagged() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(temp.path().join("Engine/Binaries")).unwrap();
        std::fs::write(temp.path().join("Game.exe"), b"").unwrap();
        std::fs::write(temp.path().join("Engine/Binaries/d3d12.dll"), b"").unwrap();
        // A near-miss name must not trigger a false positive.
        std::fs::write(temp.path().join("anticheat-notes.txt"), b"").unwrap();

        assert!(scan(temp.path()).is_empty());
    }

    #[test]
    fn detects_xigncode_by_extension() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("x3.xem"), b"").unwrap();

        assert_eq!(scan(temp.path())[0].name, "XIGNCODE3");
    }
}
