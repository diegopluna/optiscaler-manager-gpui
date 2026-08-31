//! Minimal, line-preserving edits to a game's `OptiScaler.ini`.
//!
//! The app deliberately has no config editor — OptiScaler's in-game overlay
//! owns that — but a couple of installer-level decisions live in the ini
//! (ASI plugin loading, Nvidia spoofing). Those edits change exactly one
//! `key=value` line inside one section and leave every other byte alone,
//! matching what OptiScaler's own setup script does with its search-and-
//! replace.

use std::path::Path;

use anyhow::{Context, Result, bail};

/// Sets `key=value` inside `[section]` of the ini at `path`.
///
/// The key must already exist in that section: this is for flipping shipped
/// settings, not inventing new ones — a missing key means the ini layout
/// changed and silently appending could put the key in the wrong place.
/// Matching is exact on the key name, so `Dxgi` never touches
/// `DxgiFactoryWrapping`. Returns `Ok(false)` when the value already matched.
pub fn set_ini_value(path: &Path, section: &str, key: &str, value: &str) -> Result<bool> {
    let source =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;

    let mut in_section = false;
    let mut found = false;
    let mut changed = false;

    let updated: Vec<String> = source
        .lines()
        .map(|line| {
            let trimmed = line.trim();

            if let Some(name) = trimmed.strip_prefix('[').and_then(|r| r.strip_suffix(']')) {
                in_section = name.trim().eq_ignore_ascii_case(section);
                return line.to_string();
            }

            if in_section
                && !found
                && let Some((line_key, line_value)) = trimmed.split_once('=')
                && line_key.trim().eq_ignore_ascii_case(key)
            {
                found = true;
                if line_value.trim() != value {
                    changed = true;
                    let indent = &line[..line.len() - line.trim_start().len()];
                    return format!("{indent}{key}={value}");
                }
            }
            line.to_string()
        })
        .collect();

    if !found {
        bail!(
            "{} has no `{key}` in [{section}]; the ini layout may have changed",
            path.display()
        );
    }
    if !changed {
        return Ok(false);
    }

    let mut text = updated.join("\n");
    if source.ends_with('\n') {
        text.push('\n');
    }
    std::fs::write(path, text).with_context(|| format!("writing {}", path.display()))?;
    Ok(true)
}

/// Turns Nvidia GPU spoofing on (`auto`, OptiScaler's own vendor-aware
/// default) or off (`false`) in a game's ini. AMD/Intel users who do not
/// want DLSS inputs turn it off, exactly the choice the official setup
/// script offers.
pub fn set_dxgi_spoofing(game_dir: &Path, enabled: bool) -> Result<()> {
    let ini = game_dir.join(super::archive::PAYLOAD_INI);
    let value = if enabled { "auto" } else { "false" };
    set_ini_value(&ini, "Spoofing", "Dxgi", value)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
[Plugins]
; docs
LoadAsiPlugins=auto

[Spoofing]
StreamlineSpoofing=auto
; Enables Nvidia GPU spoofing for DXGI
Dxgi=auto
DxgiFactoryWrapping=auto
";

    fn write_sample(dir: &Path) -> std::path::PathBuf {
        let path = dir.join("OptiScaler.ini");
        std::fs::write(&path, SAMPLE).unwrap();
        path
    }

    #[test]
    fn edits_only_the_exact_key_in_the_right_section() {
        let temp = tempfile::tempdir().unwrap();
        let path = write_sample(temp.path());

        assert!(set_ini_value(&path, "Spoofing", "Dxgi", "false").unwrap());

        let updated = std::fs::read_to_string(&path).unwrap();
        assert_eq!(updated, SAMPLE.replace("Dxgi=auto", "Dxgi=false"));
        assert!(
            updated.contains("DxgiFactoryWrapping=auto"),
            "prefix-sharing keys stay untouched"
        );
    }

    #[test]
    fn is_a_noop_when_the_value_already_matches() {
        let temp = tempfile::tempdir().unwrap();
        let path = write_sample(temp.path());

        assert!(!set_ini_value(&path, "Spoofing", "Dxgi", "auto").unwrap());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), SAMPLE);
    }

    #[test]
    fn refuses_a_key_outside_its_section() {
        let temp = tempfile::tempdir().unwrap();
        let path = write_sample(temp.path());

        let err = set_ini_value(&path, "Plugins", "Dxgi", "false").unwrap_err();
        assert!(err.to_string().contains("no `Dxgi`"), "{err}");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), SAMPLE, "unchanged");
    }

    #[test]
    fn spoofing_round_trips_between_auto_and_false() {
        let temp = tempfile::tempdir().unwrap();
        write_sample(temp.path());

        set_dxgi_spoofing(temp.path(), false).unwrap();
        let text = std::fs::read_to_string(temp.path().join("OptiScaler.ini")).unwrap();
        assert!(text.contains("Dxgi=false"));

        set_dxgi_spoofing(temp.path(), true).unwrap();
        let text = std::fs::read_to_string(temp.path().join("OptiScaler.ini")).unwrap();
        assert!(text.contains("Dxgi=auto"));
    }
}
