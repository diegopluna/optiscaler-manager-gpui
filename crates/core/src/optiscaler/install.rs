//! Installs, updates and removes OptiScaler in a game directory.
//!
//! Every install records a manifest listing exactly what was written and the
//! hash of each file. Uninstall consults that manifest so it only ever deletes
//! files this app put there, and leaves anything the user has since edited.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::archive::{PAYLOAD_DLL, PAYLOAD_INI};

/// Names OptiScaler can masquerade as. The game loads whichever of these it
/// looks for, so the right choice is game-dependent.
pub const PROXY_DLL_NAMES: &[&str] = &[
    "dxgi.dll",
    "winmm.dll",
    "d3d12.dll",
    "dbghelp.dll",
    "version.dll",
    "wininet.dll",
    "winhttp.dll",
];

pub const DEFAULT_PROXY_DLL: &str = "dxgi.dll";

/// Written into the game directory to record what we installed.
pub const MANIFEST_NAME: &str = "optiscaler-manager.json";

/// Where originals of files an install displaced are kept, inside the game
/// directory itself so the pair can never drift apart.
pub const BACKUP_DIR: &str = "optiscaler-manager.backup";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstalledFile {
    /// Path relative to the install directory.
    pub path: String,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstallManifest {
    pub manager_version: String,
    pub release_tag: String,
    pub proxy_name: String,
    /// Seconds since the Unix epoch.
    pub installed_at: u64,
    pub files: Vec<InstalledFile>,
    /// Files that existed before the install and were moved into
    /// [`BACKUP_DIR`] — a ReShade dxgi.dll, another mod's proxy. Restored on
    /// uninstall. Absent in old manifests.
    #[serde(default)]
    pub backed_up: Vec<String>,
}

impl InstallManifest {
    fn read(dir: &Path) -> Option<Self> {
        let text = std::fs::read_to_string(dir.join(MANIFEST_NAME)).ok()?;
        serde_json::from_str(&text).ok()
    }

    fn write(&self, dir: &Path) -> Result<()> {
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(dir.join(MANIFEST_NAME), json)
            .with_context(|| format!("writing manifest to {}", dir.display()))
    }
}

/// What, if anything, is installed in a game directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstallStatus {
    NotInstalled,
    /// Installed by this app, with the manifest to prove it.
    Managed(Box<InstallManifest>),
    /// OptiScaler files are present but we did not put them there, so we must
    /// not delete anything.
    Unmanaged {
        proxy_name: Option<String>,
    },
}

impl InstallStatus {
    pub fn is_installed(&self) -> bool {
        !matches!(self, InstallStatus::NotInstalled)
    }

    pub fn version(&self) -> Option<&str> {
        match self {
            InstallStatus::Managed(manifest) => Some(&manifest.release_tag),
            _ => None,
        }
    }
}

/// Inspects `dir` and reports whether OptiScaler is installed there.
pub fn status(dir: &Path) -> InstallStatus {
    if let Some(manifest) = InstallManifest::read(dir) {
        // Trust the manifest only while the DLL it claims to have installed is
        // still present; otherwise the user removed it by hand.
        if dir.join(&manifest.proxy_name).is_file() {
            return InstallStatus::Managed(Box::new(manifest));
        }
    }

    if dir.join(PAYLOAD_INI).is_file() {
        let proxy_name = PROXY_DLL_NAMES
            .iter()
            .find(|name| dir.join(name).is_file())
            .map(|name| (*name).to_string());
        return InstallStatus::Unmanaged { proxy_name };
    }

    InstallStatus::NotInstalled
}

fn sha256(path: &Path) -> Result<String> {
    let bytes = std::fs::read(path).with_context(|| format!("hashing {}", path.display()))?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    Ok(format!("{:x}", hasher.finalize()))
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Collects every file under `root` as a path relative to it.
fn relative_files(root: &Path) -> Result<Vec<PathBuf>> {
    fn walk(dir: &Path, base: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
        for entry in std::fs::read_dir(dir)?.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, base, out)?;
            } else if let Ok(relative) = path.strip_prefix(base) {
                out.push(relative.to_path_buf());
            }
        }
        Ok(())
    }

    let mut out = Vec::new();
    walk(root, root, &mut out)?;
    out.sort();
    Ok(out)
}

/// Files in the release archive that exist only for manual installs. The
/// setup script deletes them (and itself) when it finishes, so leaving them
/// in a game folder makes a completed install look like an aborted one.
fn is_manual_setup_helper(relative: &Path) -> bool {
    if relative.components().count() != 1 {
        return false;
    }
    let name = relative.to_string_lossy().to_lowercase();
    name == "setup_windows.bat" || name == "setup_linux.sh" || name.starts_with("!!")
}

/// Files already present in `dir` that installing `payload` would displace
/// and that we did not install ourselves — a ReShade dxgi.dll, another mod's
/// proxy. They are backed up and restored on uninstall, but the mod they
/// belong to stops working meanwhile, so callers should confirm with the
/// user before proceeding.
pub fn conflicts(payload: &Path, dir: &Path, proxy_name: &str) -> Result<Vec<String>> {
    let managed: Vec<String> = match status(dir) {
        InstallStatus::Managed(manifest) => manifest.files.iter().map(|f| f.path.clone()).collect(),
        _ => Vec::new(),
    };

    let mut conflicts = Vec::new();
    for relative in relative_files(payload)? {
        if is_manual_setup_helper(&relative) {
            continue;
        }
        let target = destination(&relative, proxy_name);
        let name = target.to_string_lossy().replace('\\', "/");
        if dir.join(&target).exists() && !managed.contains(&name) {
            conflicts.push(name);
        }
    }
    Ok(conflicts)
}

/// Maps a payload-relative path to its destination, renaming the main DLL to
/// the chosen proxy name.
fn destination(relative: &Path, proxy_name: &str) -> PathBuf {
    if relative.to_string_lossy().eq_ignore_ascii_case(PAYLOAD_DLL) {
        PathBuf::from(proxy_name)
    } else {
        relative.to_path_buf()
    }
}

/// Copies an extracted payload into `dir` and records a manifest.
///
/// An existing `OptiScaler.ini` is preserved so updates keep the user's
/// settings.
pub fn install(
    payload: &Path,
    dir: &Path,
    proxy_name: &str,
    release_tag: &str,
) -> Result<InstallManifest> {
    if !PROXY_DLL_NAMES.contains(&proxy_name) {
        bail!("{proxy_name} is not a supported proxy DLL name");
    }
    if !payload.join(PAYLOAD_DLL).is_file() {
        bail!("payload at {} has no {PAYLOAD_DLL}", payload.display());
    }
    std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;

    // Files a previous install of ours put there are ours to overwrite; the
    // backups it took are carried into the new manifest so an eventual
    // uninstall still restores the true originals.
    let previous = InstallManifest::read(dir);
    let ours: Vec<String> = previous
        .as_ref()
        .map(|m| m.files.iter().map(|f| f.path.clone()).collect())
        .unwrap_or_default();
    let mut backed_up: Vec<String> = previous.map(|m| m.backed_up).unwrap_or_default();

    let keep_existing_ini = dir.join(PAYLOAD_INI).is_file();
    let mut files = Vec::new();

    for relative in relative_files(payload)? {
        if is_manual_setup_helper(&relative) {
            continue;
        }
        let target = destination(&relative, proxy_name);
        let target_path = dir.join(&target);
        let name = target.to_string_lossy().replace('\\', "/");

        // Never clobber settings the user has already tuned.
        if keep_existing_ini && target.to_string_lossy().eq_ignore_ascii_case(PAYLOAD_INI) {
            files.push(InstalledFile {
                path: name,
                sha256: sha256(&target_path)?,
            });
            continue;
        }

        // A file that is present but not ours belongs to something else —
        // move it aside so uninstall can put it back.
        if target_path.exists() && !ours.contains(&name) && !backed_up.contains(&name) {
            let backup_path = dir.join(BACKUP_DIR).join(&target);
            std::fs::create_dir_all(backup_path.parent().unwrap())?;
            std::fs::rename(&target_path, &backup_path).with_context(|| {
                format!(
                    "backing up {} to {}",
                    target_path.display(),
                    backup_path.display()
                )
            })?;
            backed_up.push(name.clone());
        }

        if let Some(parent) = target_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::copy(payload.join(&relative), &target_path).with_context(|| {
            format!(
                "copying {} to {}",
                relative.display(),
                target_path.display()
            )
        })?;

        files.push(InstalledFile {
            path: name,
            sha256: sha256(&target_path)?,
        });
    }

    let manifest = InstallManifest {
        manager_version: env!("CARGO_PKG_VERSION").to_string(),
        release_tag: release_tag.to_string(),
        proxy_name: proxy_name.to_string(),
        installed_at: now_secs(),
        files,
        backed_up,
    };
    manifest.write(dir)?;
    Ok(manifest)
}

/// Adds files to an existing managed install's manifest, so later uninstalls
/// remove them too. Used for extras added after the main install, such as
/// OptiPatcher. Paths are relative to `dir` and must exist.
pub fn record_extra_files(dir: &Path, relative_paths: &[PathBuf]) -> Result<()> {
    let Some(mut manifest) = InstallManifest::read(dir) else {
        bail!("no install manifest in {}", dir.display());
    };

    for relative in relative_paths {
        let name = relative.to_string_lossy().replace('\\', "/");
        let hash = sha256(&dir.join(relative))?;
        match manifest.files.iter_mut().find(|f| f.path == name) {
            Some(existing) => existing.sha256 = hash,
            None => manifest.files.push(InstalledFile {
                path: name,
                sha256: hash,
            }),
        }
    }
    manifest.write(dir)
}

/// Report of what an uninstall actually did.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct UninstallReport {
    pub removed: Vec<String>,
    /// Files left alone because they changed since install.
    pub kept_modified: Vec<String>,
    /// Pre-install originals put back from the backup folder.
    pub restored: Vec<String>,
}

/// Removes a managed install. Files whose contents changed since installation
/// are left in place and reported, since they may be the user's own work.
///
/// `keep_ini` preserves `OptiScaler.ini`, which is always modified in practice
/// because OptiScaler rewrites it from its in-game overlay.
pub fn uninstall(dir: &Path, keep_ini: bool) -> Result<UninstallReport> {
    let InstallStatus::Managed(manifest) = status(dir) else {
        bail!(
            "no OptiScaler install managed by this app in {}",
            dir.display()
        );
    };

    let mut report = UninstallReport::default();

    for file in &manifest.files {
        let path = dir.join(&file.path);
        let is_ini = file.path.eq_ignore_ascii_case(PAYLOAD_INI);

        if is_ini && keep_ini {
            continue;
        }
        if !path.exists() {
            continue;
        }

        // The ini is expected to have changed, so its hash is not a veto.
        let changed = sha256(&path)
            .map(|hash| hash != file.sha256)
            .unwrap_or(true);
        if changed && !is_ini {
            report.kept_modified.push(file.path.clone());
            continue;
        }

        std::fs::remove_file(&path).with_context(|| format!("removing {}", path.display()))?;
        report.removed.push(file.path.clone());
    }

    // Put displaced originals back where they were, unless a kept-modified
    // file of ours still occupies the spot — the user's edit wins, and the
    // backup stays available on disk.
    for name in &manifest.backed_up {
        let backup_path = dir.join(BACKUP_DIR).join(name);
        if !backup_path.is_file() {
            continue;
        }
        let original = dir.join(name);
        if original.exists() {
            continue;
        }
        if let Some(parent) = original.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::rename(&backup_path, &original)
            .with_context(|| format!("restoring {}", original.display()))?;
        report.restored.push(name.clone());
    }
    remove_empty_tree(&dir.join(BACKUP_DIR));

    std::fs::remove_file(dir.join(MANIFEST_NAME)).ok();
    remove_empty_dirs(dir, &manifest);
    Ok(report)
}

/// Removes a directory tree that contains only empty directories; anything
/// holding a file survives untouched.
fn remove_empty_tree(root: &Path) {
    fn is_empty_after_cleanup(dir: &Path) -> bool {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return false;
        };
        let mut empty = true;
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() && is_empty_after_cleanup(&path) {
                let _ = std::fs::remove_dir(&path);
            } else {
                empty = false;
            }
        }
        empty
    }
    if root.is_dir() && is_empty_after_cleanup(root) {
        let _ = std::fs::remove_dir(root);
    }
}

/// Cleans up directories the install created, deepest first, stopping at any
/// that still holds files.
fn remove_empty_dirs(dir: &Path, manifest: &InstallManifest) {
    let mut dirs: Vec<PathBuf> = manifest
        .files
        .iter()
        .filter_map(|file| Path::new(&file.path).parent().map(Path::to_path_buf))
        .filter(|parent| !parent.as_os_str().is_empty())
        .collect();
    dirs.sort_by_key(|path| std::cmp::Reverse(path.components().count()));
    dirs.dedup();

    for relative in dirs {
        let path = dir.join(relative);
        if path.is_dir() && std::fs::read_dir(&path).is_ok_and(|mut e| e.next().is_none()) {
            std::fs::remove_dir(&path).ok();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a payload directory that mirrors a real release layout.
    fn payload() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(PAYLOAD_DLL), b"optiscaler dll").unwrap();
        std::fs::write(dir.path().join(PAYLOAD_INI), b"; defaults").unwrap();
        std::fs::create_dir_all(dir.path().join("plugins")).unwrap();
        std::fs::write(dir.path().join("plugins/fakenvapi.dll"), b"fake").unwrap();
        // Manual-setup helpers shipped in the real archive; these must never
        // reach the game directory.
        std::fs::write(dir.path().join("setup_windows.bat"), b"@echo off").unwrap();
        std::fs::write(dir.path().join("setup_linux.sh"), b"#!/bin/sh").unwrap();
        std::fs::write(
            dir.path()
                .join("!! README_EXTRACT ALL FILES TO GAME FOLDER !!.txt"),
            b"readme",
        )
        .unwrap();
        dir
    }

    #[test]
    fn installs_renaming_the_dll_and_records_a_manifest() {
        let payload = payload();
        let game = tempfile::tempdir().unwrap();

        let manifest = install(payload.path(), game.path(), "dxgi.dll", "v0.9.4").unwrap();

        assert!(game.path().join("dxgi.dll").is_file(), "dll was renamed");
        assert!(!game.path().join(PAYLOAD_DLL).exists());
        assert!(game.path().join("plugins/fakenvapi.dll").is_file());
        assert!(
            !game.path().join("setup_windows.bat").exists()
                && !game.path().join("setup_linux.sh").exists(),
        );
        assert!(
            std::fs::read_dir(game.path())
                .unwrap()
                .flatten()
                .all(|e| !e.file_name().to_string_lossy().starts_with("!!")),
            "the extract-me readme stays out of the game folder"
        );
        assert_eq!(manifest.release_tag, "v0.9.4");
        assert_eq!(manifest.files.len(), 3);

        assert_eq!(status(game.path()).version(), Some("v0.9.4"));
    }

    #[test]
    fn rejects_an_unsupported_proxy_name() {
        let payload = payload();
        let game = tempfile::tempdir().unwrap();

        let err = install(payload.path(), game.path(), "evil.dll", "v1").unwrap_err();
        assert!(err.to_string().contains("not a supported proxy"));
    }

    #[test]
    fn uninstall_removes_our_files_and_keeps_edited_ones() {
        let payload = payload();
        let game = tempfile::tempdir().unwrap();
        install(payload.path(), game.path(), "dxgi.dll", "v0.9.4").unwrap();

        // The user replaced a bundled plugin with their own build.
        std::fs::write(game.path().join("plugins/fakenvapi.dll"), b"user build").unwrap();

        let report = uninstall(game.path(), false).unwrap();

        assert!(!game.path().join("dxgi.dll").exists(), "our dll is gone");
        assert!(
            game.path().join("plugins/fakenvapi.dll").is_file(),
            "modified file is left alone"
        );
        assert_eq!(report.kept_modified, vec!["plugins/fakenvapi.dll"]);
        assert_eq!(status(game.path()), InstallStatus::NotInstalled);
    }

    #[test]
    fn uninstall_can_keep_the_ini() {
        let payload = payload();
        let game = tempfile::tempdir().unwrap();
        install(payload.path(), game.path(), "winmm.dll", "v0.9.4").unwrap();
        std::fs::write(game.path().join(PAYLOAD_INI), b"; user tuned").unwrap();

        uninstall(game.path(), true).unwrap();

        assert!(game.path().join(PAYLOAD_INI).is_file(), "ini preserved");
        assert!(!game.path().join("winmm.dll").exists());
    }

    #[test]
    fn reinstalling_preserves_an_existing_ini() {
        let payload = payload();
        let game = tempfile::tempdir().unwrap();
        install(payload.path(), game.path(), "dxgi.dll", "v0.9.3").unwrap();
        std::fs::write(game.path().join(PAYLOAD_INI), b"; user tuned").unwrap();

        install(payload.path(), game.path(), "dxgi.dll", "v0.9.4").unwrap();

        let ini = std::fs::read_to_string(game.path().join(PAYLOAD_INI)).unwrap();
        assert_eq!(ini, "; user tuned", "update kept the user's settings");
        assert_eq!(status(game.path()).version(), Some("v0.9.4"));
    }

    #[test]
    fn detects_a_manual_install_without_a_manifest() {
        let game = tempfile::tempdir().unwrap();
        std::fs::write(game.path().join(PAYLOAD_INI), b"; ini").unwrap();
        std::fs::write(game.path().join("winmm.dll"), b"dll").unwrap();

        assert_eq!(
            status(game.path()),
            InstallStatus::Unmanaged {
                proxy_name: Some("winmm.dll".into())
            }
        );
    }

    #[test]
    fn extra_files_join_the_manifest_and_are_uninstalled() {
        let payload = payload();
        let game = tempfile::tempdir().unwrap();
        install(payload.path(), game.path(), "dxgi.dll", "v0.9.4").unwrap();

        // Simulate OptiPatcher being added after the install.
        std::fs::create_dir_all(game.path().join("plugins")).unwrap();
        std::fs::write(game.path().join("plugins/OptiPatcher.asi"), b"asi").unwrap();
        record_extra_files(game.path(), &[PathBuf::from("plugins/OptiPatcher.asi")]).unwrap();

        let report = uninstall(game.path(), false).unwrap();
        assert!(
            report
                .removed
                .iter()
                .any(|f| f == "plugins/OptiPatcher.asi")
        );
        assert!(
            !game.path().join("plugins").exists(),
            "empty plugins dir cleaned up"
        );
    }

    #[test]
    fn backs_up_a_foreign_proxy_and_restores_it_on_uninstall() {
        let payload = payload();
        let game = tempfile::tempdir().unwrap();
        // ReShade got there first.
        std::fs::write(game.path().join("dxgi.dll"), b"reshade").unwrap();

        let manifest = install(payload.path(), game.path(), "dxgi.dll", "v0.9.4").unwrap();
        assert_eq!(manifest.backed_up, vec!["dxgi.dll"]);
        assert_eq!(
            std::fs::read(game.path().join(BACKUP_DIR).join("dxgi.dll")).unwrap(),
            b"reshade",
            "the original is parked in the backup folder"
        );
        assert_eq!(
            std::fs::read(game.path().join("dxgi.dll")).unwrap(),
            b"optiscaler dll",
            "ours is the live one"
        );

        let report = uninstall(game.path(), false).unwrap();
        assert_eq!(report.restored, vec!["dxgi.dll"]);
        assert_eq!(
            std::fs::read(game.path().join("dxgi.dll")).unwrap(),
            b"reshade",
            "the original is back"
        );
        assert!(
            !game.path().join(BACKUP_DIR).exists(),
            "backup folder cleaned up"
        );
    }

    #[test]
    fn updates_carry_the_backup_forward() {
        let payload = payload();
        let game = tempfile::tempdir().unwrap();
        std::fs::write(game.path().join("dxgi.dll"), b"reshade").unwrap();

        install(payload.path(), game.path(), "dxgi.dll", "v0.9.3").unwrap();
        let updated = install(payload.path(), game.path(), "dxgi.dll", "v0.9.4").unwrap();
        assert_eq!(
            updated.backed_up,
            vec!["dxgi.dll"],
            "not re-backed-up over the original"
        );
        assert_eq!(
            std::fs::read(game.path().join(BACKUP_DIR).join("dxgi.dll")).unwrap(),
            b"reshade",
            "the true original survives the update"
        );

        uninstall(game.path(), false).unwrap();
        assert_eq!(
            std::fs::read(game.path().join("dxgi.dll")).unwrap(),
            b"reshade"
        );
    }

    #[test]
    fn a_user_modified_file_keeps_its_spot_over_the_backup() {
        let payload = payload();
        let game = tempfile::tempdir().unwrap();
        std::fs::write(game.path().join("dxgi.dll"), b"reshade").unwrap();
        install(payload.path(), game.path(), "dxgi.dll", "v0.9.4").unwrap();

        // The user replaced our dll with their own build after installing.
        std::fs::write(game.path().join("dxgi.dll"), b"user build").unwrap();

        let report = uninstall(game.path(), false).unwrap();
        assert_eq!(report.kept_modified, vec!["dxgi.dll"]);
        assert!(
            report.restored.is_empty(),
            "restore must not clobber the user's file"
        );
        assert_eq!(
            std::fs::read(game.path().join("dxgi.dll")).unwrap(),
            b"user build"
        );
        assert!(
            game.path().join(BACKUP_DIR).join("dxgi.dll").is_file(),
            "the original stays recoverable on disk"
        );
    }

    #[test]
    fn reports_conflicting_files_before_install() {
        let payload = payload();
        let game = tempfile::tempdir().unwrap();
        // A pre-existing dxgi.dll, e.g. from ReShade.
        std::fs::write(game.path().join("dxgi.dll"), b"reshade").unwrap();

        let found = conflicts(payload.path(), game.path(), "dxgi.dll").unwrap();
        assert_eq!(found, vec!["dxgi.dll"]);

        // Once managed by us, it is no longer a conflict.
        install(payload.path(), game.path(), "dxgi.dll", "v0.9.4").unwrap();
        assert!(
            conflicts(payload.path(), game.path(), "dxgi.dll")
                .unwrap()
                .is_empty()
        );
    }
}
