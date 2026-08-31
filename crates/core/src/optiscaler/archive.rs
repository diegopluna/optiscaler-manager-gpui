//! Extracts an OptiScaler release archive into a staging directory.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

/// The two files every valid OptiScaler payload must contain.
pub const PAYLOAD_DLL: &str = "OptiScaler.dll";
pub const PAYLOAD_INI: &str = "OptiScaler.ini";

/// Extracts `archive` into a fresh directory under `staging_root` and returns
/// the directory holding `OptiScaler.dll`.
///
/// The payload is validated before any of it is copied into a game, so a
/// corrupt or unexpected archive can never leave a game directory half-modified.
pub fn extract(archive: &Path, staging_root: &Path) -> Result<PathBuf> {
    if staging_root.exists() {
        std::fs::remove_dir_all(staging_root)
            .with_context(|| format!("clearing {}", staging_root.display()))?;
    }
    std::fs::create_dir_all(staging_root)?;

    sevenz_rust2::decompress_file(archive, staging_root)
        .with_context(|| format!("extracting {}", archive.display()))?;

    payload_root(staging_root)
}

/// Finds the directory containing `OptiScaler.dll`. Releases put it at the top
/// level, but a nested layout would still work.
pub fn payload_root(staging_root: &Path) -> Result<PathBuf> {
    fn find(dir: &Path, depth: usize) -> Option<PathBuf> {
        if depth > 3 {
            return None;
        }
        let entries: Vec<_> = std::fs::read_dir(dir).ok()?.flatten().collect();

        if entries.iter().any(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .eq_ignore_ascii_case(PAYLOAD_DLL)
        }) {
            return Some(dir.to_path_buf());
        }

        entries
            .iter()
            .filter(|entry| entry.path().is_dir())
            .find_map(|entry| find(&entry.path(), depth + 1))
    }

    let root = find(staging_root, 0)
        .ok_or_else(|| anyhow::anyhow!("archive does not contain {PAYLOAD_DLL}"))?;

    if !root.join(PAYLOAD_INI).is_file() {
        bail!("archive does not contain {PAYLOAD_INI}");
    }

    Ok(root)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_payload_at_top_level() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join(PAYLOAD_DLL), b"dll").unwrap();
        std::fs::write(temp.path().join(PAYLOAD_INI), b"; ini").unwrap();

        assert_eq!(payload_root(temp.path()).unwrap(), temp.path());
    }

    #[test]
    fn finds_payload_in_a_nested_folder() {
        let temp = tempfile::tempdir().unwrap();
        let nested = temp.path().join("OptiScaler_v0.9.4");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::write(nested.join(PAYLOAD_DLL), b"dll").unwrap();
        std::fs::write(nested.join(PAYLOAD_INI), b"; ini").unwrap();

        assert_eq!(payload_root(temp.path()).unwrap(), nested);
    }

    #[test]
    fn rejects_a_payload_missing_its_ini() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join(PAYLOAD_DLL), b"dll").unwrap();

        let err = payload_root(temp.path()).unwrap_err().to_string();
        assert!(err.contains(PAYLOAD_INI), "unexpected error: {err}");
    }

    #[test]
    fn rejects_an_archive_without_optiscaler() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("readme.txt"), b"nope").unwrap();

        assert!(payload_root(temp.path()).is_err());
    }
}
