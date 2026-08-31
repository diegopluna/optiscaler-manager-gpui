//! Works out which directory OptiScaler should be dropped into.
//!
//! The mod has to sit next to the executable that actually creates the D3D
//! device. For most games that is the only executable in the install root, but
//! Unreal Engine titles hide theirs several levels down under `Binaries`.

use std::path::{Path, PathBuf};

use crate::model::Game;

/// Executables that ship alongside games but never render anything.
const IGNORED_EXE_PREFIXES: &[&str] = &[
    "unitycrashhandler",
    "unrealcefsubprocess",
    "crashreportclient",
    "crashreporter",
    "easyanticheat",
    "battleye",
    "be_service",
    "vcredist",
    "dxsetup",
    "dotnetfx",
    "oalinst",
    "uninstall",
    "launcher_installer",
];

const MAX_DEPTH: usize = 4;

fn is_ignored_exe(path: &Path) -> bool {
    let Some(name) = path.file_stem().map(|n| n.to_string_lossy().to_lowercase()) else {
        return true;
    };
    IGNORED_EXE_PREFIXES
        .iter()
        .any(|prefix| name.starts_with(prefix))
}

fn is_exe(path: &Path) -> bool {
    path.extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("exe"))
}

/// Collects executables under `root`, breadth-limited so a huge install does
/// not turn detection into a full disk walk.
fn walk_exes(root: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    if depth > MAX_DEPTH || out.len() > 512 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        match entry.file_type() {
            Ok(ft) if ft.is_dir() => walk_exes(&path, depth + 1, out),
            Ok(ft) if ft.is_file() && is_exe(&path) && !is_ignored_exe(&path) => out.push(path),
            _ => {}
        }
    }
}

/// The directory OptiScaler should be installed into for `game`, before any
/// user override is applied.
pub fn install_target(game: &Game) -> PathBuf {
    guess_exe_dir(&game.install_dir, game.launch_exe.as_deref())
}

/// The heuristic itself, split out so it can be tested against a fixture tree.
pub fn guess_exe_dir(install_dir: &Path, launch_exe: Option<&Path>) -> PathBuf {
    // The launcher told us the executable outright.
    if let Some(exe) = launch_exe
        && exe.is_file()
        && let Some(parent) = exe.parent()
    {
        return parent.to_path_buf();
    }

    let mut exes = Vec::new();
    walk_exes(install_dir, 0, &mut exes);

    // Unreal Engine: the shipping binary is the one that renders, not the
    // small launcher stub that usually sits in the install root.
    if let Some(shipping) = exes.iter().find(|path| {
        path.file_stem()
            .is_some_and(|stem| stem.to_string_lossy().ends_with("-Win64-Shipping"))
    }) && let Some(parent) = shipping.parent()
    {
        return parent.to_path_buf();
    }

    // A single executable in the root is unambiguous.
    let top_level: Vec<&PathBuf> = exes
        .iter()
        .filter(|path| path.parent() == Some(install_dir))
        .collect();
    if top_level.len() == 1 {
        return install_dir.to_path_buf();
    }

    // Otherwise assume the biggest executable is the game itself.
    let largest = exes
        .iter()
        .max_by_key(|path| std::fs::metadata(path).map(|m| m.len()).unwrap_or(0));

    largest
        .and_then(|path| path.parent())
        .map(Path::to_path_buf)
        .unwrap_or_else(|| install_dir.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(path: &Path, size: usize) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, vec![0u8; size]).unwrap();
    }

    #[test]
    fn prefers_unreal_shipping_binary_over_root_stub() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        write(&root.join("Fortnite.exe"), 1024);
        let shipping = root.join("Game/Binaries/Win64/Game-Win64-Shipping.exe");
        write(&shipping, 64);

        assert_eq!(guess_exe_dir(root, None), shipping.parent().unwrap());
    }

    #[test]
    fn uses_launcher_provided_executable_when_present() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let exe = root.join("bin/Control.exe");
        write(&exe, 10);
        write(&root.join("Other.exe"), 5000);

        assert_eq!(guess_exe_dir(root, Some(&exe)), root.join("bin"));
    }

    #[test]
    fn single_root_executable_wins() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        write(&root.join("Witcher3.exe"), 100);
        // A bigger executable nested elsewhere should not beat the only
        // top-level one.
        write(&root.join("tools/editor.exe"), 9000);

        assert_eq!(guess_exe_dir(root, None), root);
    }

    #[test]
    fn falls_back_to_largest_executable() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        write(&root.join("a.exe"), 10);
        write(&root.join("b.exe"), 20);
        write(&root.join("game/main.exe"), 5000);

        assert_eq!(guess_exe_dir(root, None), root.join("game"));
    }

    #[test]
    fn ignores_crash_handlers_and_redists() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        write(&root.join("UnityCrashHandler64.exe"), 100_000);
        write(&root.join("MyGame.exe"), 50);

        assert_eq!(guess_exe_dir(root, None), root);
    }
}
