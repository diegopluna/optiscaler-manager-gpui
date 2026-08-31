//! Detects which upscaling technologies a game ships, from the runtime DLLs
//! in its install folder.
//!
//! OptiScaler works by hijacking the upscaler inputs a game already exposes,
//! so this answers the question the catalog should answer at a glance: is
//! installing OptiScaler here worth it, and what will it hook? A game with
//! `nvngx_dlss.dll` has DLSS inputs to take over; one with only FSR2 can be
//! upgraded; one with nothing has no inputs to hijack.
//!
//! Callers must exclude files OptiScaler Manager itself installed (the
//! payload ships FSR and XeSS DLLs), or every managed game would appear to
//! support everything. The install manifest lists exactly those files.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// An upscaling technology family. Frame-generation variants count toward
/// their family; the evidence file names the specific DLL.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Tech {
    Dlss,
    Fsr,
    Xess,
}

impl Tech {
    pub fn label(self) -> &'static str {
        match self {
            Tech::Dlss => "DLSS",
            Tech::Fsr => "FSR",
            Tech::Xess => "XeSS",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Detection {
    pub tech: Tech,
    /// Path of the DLL, relative to the scanned directory.
    pub file: PathBuf,
}

/// Classifies a file name as upscaler runtime evidence. Names follow the
/// SDKs' own conventions (also what SteamDB's detection rules match on).
pub fn classify(file_name: &str) -> Option<Tech> {
    let name = file_name.to_lowercase();
    if !name.ends_with(".dll") {
        return None;
    }

    if name == "nvngx_dlss.dll" || name == "nvngx_dlssg.dll" || name == "nvngx_dlssd.dll" {
        return Some(Tech::Dlss);
    }
    if name.starts_with("ffx_fsr2") || name.starts_with("amd_fidelityfx_") {
        return Some(Tech::Fsr);
    }
    if name.starts_with("libxess") {
        return Some(Tech::Xess);
    }
    None
}

/// Unreal buries `nvngx_dlss.dll` under
/// `Engine/Plugins/.../DLSS/Binaries/ThirdParty/Win64/`, so this walk goes
/// much deeper than the anti-cheat scan; the entry budget keeps it bounded on
/// enormous installs.
const MAX_DEPTH: usize = 8;
const MAX_ENTRIES: usize = 20_000;

/// Scans a game directory for upscaler runtime DLLs. `exclude` holds absolute
/// paths to skip — the files OptiScaler Manager installed itself.
pub fn scan(install_dir: &Path, exclude: &HashSet<PathBuf>) -> Vec<Detection> {
    let mut found = Vec::new();
    let mut budget = MAX_ENTRIES;
    walk(
        install_dir,
        install_dir,
        0,
        &mut budget,
        exclude,
        &mut found,
    );
    found.sort_by_key(|detection| (detection.tech, detection.file.clone()));
    found
}

/// The distinct technologies in a detection list, for badges.
pub fn techs(detections: &[Detection]) -> Vec<Tech> {
    let mut techs: Vec<Tech> = detections.iter().map(|d| d.tech).collect();
    techs.sort();
    techs.dedup();
    techs
}

fn walk(
    dir: &Path,
    base: &Path,
    depth: usize,
    budget: &mut usize,
    exclude: &HashSet<PathBuf>,
    found: &mut Vec<Detection>,
) {
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
        if is_dir {
            walk(&path, base, depth + 1, budget, exclude, found);
            continue;
        }

        let name = entry.file_name();
        if let Some(tech) = classify(&name.to_string_lossy())
            && !exclude.contains(&path)
        {
            found.push(Detection {
                tech,
                file: path.strip_prefix(base).unwrap_or(&path).to_path_buf(),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_the_known_runtimes() {
        assert_eq!(classify("nvngx_dlss.dll"), Some(Tech::Dlss));
        assert_eq!(classify("NVNGX_DLSSG.DLL"), Some(Tech::Dlss));
        assert_eq!(classify("ffx_fsr2_api_x64.dll"), Some(Tech::Fsr));
        assert_eq!(classify("amd_fidelityfx_dx12.dll"), Some(Tech::Fsr));
        assert_eq!(classify("libxess.dll"), Some(Tech::Xess));
        assert_eq!(classify("libxess_fg.dll"), Some(Tech::Xess));

        assert_eq!(
            classify("nvngx.dll"),
            None,
            "the game-side stub is not evidence"
        );
        assert_eq!(classify("d3d12.dll"), None);
        assert_eq!(classify("nvngx_dlss.txt"), None);
    }

    #[test]
    fn finds_a_deeply_nested_unreal_dlss_dll() {
        let temp = tempfile::tempdir().unwrap();
        let deep = temp
            .path()
            .join("Engine/Plugins/Marketplace/DLSS/Binaries/ThirdParty/Win64");
        std::fs::create_dir_all(&deep).unwrap();
        std::fs::write(deep.join("nvngx_dlss.dll"), b"").unwrap();

        let found = scan(temp.path(), &HashSet::new());
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].tech, Tech::Dlss);
    }

    #[test]
    fn excluded_files_do_not_count() {
        // The situation after our own install: the game shipped DLSS, and we
        // added FSR/XeSS DLLs of our own, which must not read as game support.
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("nvngx_dlss.dll"), b"game").unwrap();
        std::fs::write(temp.path().join("amd_fidelityfx_dx12.dll"), b"ours").unwrap();
        std::fs::write(temp.path().join("libxess.dll"), b"ours").unwrap();

        let exclude: HashSet<PathBuf> = [
            temp.path().join("amd_fidelityfx_dx12.dll"),
            temp.path().join("libxess.dll"),
        ]
        .into();

        let found = scan(temp.path(), &exclude);
        assert_eq!(techs(&found), vec![Tech::Dlss], "{found:?}");
    }

    #[test]
    fn techs_dedupes_families() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("nvngx_dlss.dll"), b"").unwrap();
        std::fs::write(temp.path().join("nvngx_dlssg.dll"), b"").unwrap();
        std::fs::write(temp.path().join("ffx_fsr2_api_x64.dll"), b"").unwrap();

        let found = scan(temp.path(), &HashSet::new());
        assert_eq!(techs(&found), vec![Tech::Dlss, Tech::Fsr]);
    }
}
