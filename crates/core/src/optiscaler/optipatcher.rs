//! OptiPatcher support: an OptiScaler plugin that unlocks DLSS/DLSS-FG inputs
//! in specific games without GPU spoofing or its overhead.
//!
//! There is no published compatibility table as data; the source of truth is
//! the game check in OptiPatcher's own `dllmain.cpp`, which is exactly what
//! OptiScaler's `setup_windows.bat` parses. This module does the same: pull
//! the source, collect the executable names it matches on, and compare them
//! with the executables in the game's install directory.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result, bail};

const SOURCE_URL: &str =
    "https://raw.githubusercontent.com/optiscaler/OptiPatcher/main/OptiPatcher/dllmain.cpp";
const ASI_URL: &str =
    "https://github.com/optiscaler/OptiPatcher/releases/download/rolling/OptiPatcher.asi";
const USER_AGENT: &str = concat!("optiscaler-manager/", env!("CARGO_PKG_VERSION"));
const TIMEOUT: Duration = Duration::from_secs(30);

/// Where the plugin lives inside a game directory, relative to the target.
pub const ASI_RELATIVE_PATH: &str = "plugins/OptiPatcher.asi";

/// Downloads the current compatibility source and returns the lowercase
/// executable names OptiPatcher knows how to patch.
pub fn fetch_supported_exes() -> Result<Vec<String>> {
    let body = ureq::get(SOURCE_URL)
        .config()
        .timeout_global(Some(TIMEOUT))
        .build()
        .header("User-Agent", USER_AGENT)
        .call()
        .context("fetching the OptiPatcher compatibility source")?
        .body_mut()
        .read_to_string()?;

    let supported = parse_supported_exes(&body);
    if supported.is_empty() {
        bail!("no supported games found in the OptiPatcher source; format may have changed");
    }
    Ok(supported)
}

/// Extracts supported executable names from OptiPatcher's `dllmain.cpp`.
///
/// Two patterns, mirroring the official setup script:
/// - `CHECK_UE(base)` matches `base-win64-shipping.exe` and
///   `base-wingdk-shipping.exe`
/// - `exeName == "literal.exe"` matches that name directly
pub fn parse_supported_exes(source: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();

    for line in source.lines() {
        // The #define of CHECK_UE itself would parse as a bogus game entry.
        if line.trim_start().starts_with("#define") {
            continue;
        }

        let mut rest = line;
        while let Some(ix) = rest.find("CHECK_UE") {
            rest = &rest[ix + "CHECK_UE".len()..];
            let Some(open) = rest.find('(') else { break };
            let Some(close) = rest[open..].find(')') else {
                break;
            };
            let base = rest[open + 1..open + close].trim();
            if !base.is_empty() && base.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                out.push(format!("{}-win64-shipping.exe", base.to_lowercase()));
                out.push(format!("{}-wingdk-shipping.exe", base.to_lowercase()));
            }
            rest = &rest[open + close..];
        }

        let mut rest = line;
        while let Some(ix) = rest.find("exeName ==") {
            rest = &rest[ix + "exeName ==".len()..];
            let trimmed = rest.trim_start();
            if let Some(stripped) = trimmed.strip_prefix('"')
                && let Some(end) = stripped.find('"')
            {
                let name = &stripped[..end];
                if name.ends_with(".exe") {
                    out.push(name.to_lowercase());
                }
                rest = &stripped[end..];
            } else {
                break;
            }
        }
    }

    out.sort();
    out.dedup();
    out
}

/// The executable in `dir` that OptiPatcher supports, if any. Like the
/// official script, only the directory itself is checked — the plugin loads
/// from beside the game binary, which is where OptiScaler is installed.
pub fn matching_exe(dir: &Path, supported: &[String]) -> Option<String> {
    let entries = std::fs::read_dir(dir).ok()?;
    for entry in entries.flatten() {
        if entry.file_type().is_ok_and(|ft| ft.is_file()) {
            let name = entry.file_name().to_string_lossy().to_lowercase();
            if name.ends_with(".exe") && supported.iter().any(|s| s == &name) {
                return Some(name);
            }
        }
    }
    None
}

/// Whether the plugin is already present in a game directory.
pub fn is_installed(dir: &Path) -> bool {
    dir.join(ASI_RELATIVE_PATH).is_file()
}

/// Downloads OptiPatcher into `dir/plugins` and enables ASI loading in the
/// game's `OptiScaler.ini`. Returns the paths written, relative to `dir`, so
/// the caller can record them in the install manifest.
pub fn install(dir: &Path) -> Result<Vec<PathBuf>> {
    let ini_path = dir.join(super::archive::PAYLOAD_INI);
    if !ini_path.is_file() {
        bail!(
            "OptiScaler is not installed in {} (no OptiScaler.ini)",
            dir.display()
        );
    }

    let mut response = ureq::get(ASI_URL)
        .config()
        .timeout_global(Some(Duration::from_secs(120)))
        .build()
        .header("User-Agent", USER_AGENT)
        .call()
        .context("downloading OptiPatcher.asi")?;
    let bytes = response.body_mut().read_to_vec()?;
    if bytes.is_empty() {
        bail!("OptiPatcher download was empty");
    }

    let asi_path = dir.join(ASI_RELATIVE_PATH);
    std::fs::create_dir_all(asi_path.parent().unwrap())?;
    let temp = asi_path.with_extension("asi.part");
    {
        let mut file = std::fs::File::create(&temp)?;
        file.write_all(&bytes)?;
        file.flush()?;
    }
    std::fs::rename(&temp, &asi_path)?;

    super::ini_edit::set_ini_value(&ini_path, "Plugins", "LoadAsiPlugins", "true")
        .with_context(|| format!("enabling ASI loading in {}", ini_path.display()))?;

    Ok(vec![PathBuf::from(ASI_RELATIVE_PATH)])
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A faithful excerpt of the real dllmain.cpp.
    const SOURCE: &str = r#"
#define CHECK_UE(name) exeName == (#name "-win64-shipping.exe") || exeName == (#name "-wingdk-shipping.exe")

    if (CHECK_UE(fmf2) || CHECK_UE(themidnightwalk))
    {
    }
    else if (exeName == "bloom&rage.exe" || exeName == "f1manager24.exe")
    {
    }
    else if (CHECK_UE(stalker2))
    {
    }
    else if (exeName == "hogwartslegacy.exe" || CHECK_UE(witchfire))
    {
    }
"#;

    #[test]
    fn parses_both_patterns_from_the_real_source_shape() {
        let supported = parse_supported_exes(SOURCE);

        for expected in [
            "fmf2-win64-shipping.exe",
            "fmf2-wingdk-shipping.exe",
            "themidnightwalk-win64-shipping.exe",
            "stalker2-win64-shipping.exe",
            "witchfire-win64-shipping.exe",
            "bloom&rage.exe",
            "f1manager24.exe",
            "hogwartslegacy.exe",
        ] {
            assert!(
                supported.iter().any(|s| s == expected),
                "missing {expected} in {supported:?}"
            );
        }

        // The #define line must not produce a phantom "name" game.
        assert!(
            !supported.iter().any(|s| s.starts_with("name-")),
            "the CHECK_UE #define leaked into the list: {supported:?}"
        );
    }

    #[test]
    fn matches_a_supported_exe_in_the_game_directory() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("Stalker2-Win64-Shipping.exe"), b"").unwrap();
        std::fs::write(temp.path().join("other.exe"), b"").unwrap();

        let supported = parse_supported_exes(SOURCE);
        assert_eq!(
            matching_exe(temp.path(), &supported).as_deref(),
            Some("stalker2-win64-shipping.exe"),
            "match is case-insensitive"
        );

        let unsupported = tempfile::tempdir().unwrap();
        std::fs::write(unsupported.path().join("game.exe"), b"").unwrap();
        assert_eq!(matching_exe(unsupported.path(), &supported), None);
    }
}
