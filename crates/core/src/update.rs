//! Self-update: checks this app's own GitHub releases for a newer version
//! and applies it.
//!
//! Windows installs come from an Inno Setup installer, so updating means
//! downloading the new setup executable and launching it — a running
//! executable cannot replace itself on Windows, and the installer already
//! knows how to close the app and swap the files. On Linux the app is a
//! single portable binary, which can be swapped in place while running and
//! picked up on the next launch.

use std::io::Write;
// `Path` only appears in the binary-swap path, which Windows does not compile
// (updates there go through the installer instead).
#[cfg(not(target_os = "windows"))]
use std::path::Path;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};

const REPO: &str = "diegopluna/optiscaler-manager-gpui";
const USER_AGENT: &str = concat!("optiscaler-manager/", env!("CARGO_PKG_VERSION"));
const TIMEOUT: Duration = Duration::from_secs(30);

/// The version this binary was built as.
pub const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// A newer release of this app, ready to download.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateInfo {
    /// Release tag, e.g. `v0.2.0`.
    pub tag: String,
    pub asset_name: String,
    pub download_url: String,
    pub size: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateCheck {
    UpToDate,
    Available(UpdateInfo),
}

/// What applying an update did, which decides what the UI asks of the user.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Applied {
    /// The platform installer was launched; the app should quit so the
    /// installer can replace it.
    InstallerLaunched,
    /// The binary was swapped in place; the change takes effect on restart.
    RestartRequired,
}

/// Parses `v1.2.3` or `1.2.3` into a comparable triple.
fn parse_version(text: &str) -> Option<(u64, u64, u64)> {
    let text = text.trim().trim_start_matches(['v', 'V']);
    let mut parts = text.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    // Tolerate suffixes like `3-beta` on the patch component.
    let patch_text = parts.next().unwrap_or("0");
    let patch = patch_text.split(['-', '+']).next()?.parse().ok()?;
    Some((major, minor, patch))
}

/// Whether `tag` is a strictly newer version than this binary.
pub fn is_newer(tag: &str) -> bool {
    match (parse_version(tag), parse_version(CURRENT_VERSION)) {
        (Some(remote), Some(local)) => remote > local,
        _ => false,
    }
}

/// Picks the right release asset for an OS, by the naming convention the
/// release workflow uses. Split out from [`check`] so every platform's
/// selection is testable from one host.
fn select_asset(os: &str, names: &[&str]) -> Option<usize> {
    let suffix: &dyn Fn(&str) -> bool = match os {
        // The installer both installs and updates; the zip is the portable
        // fallback and cannot replace a running exe, so it is never used
        // for self-update.
        "windows" => &|name| name.ends_with("-setup.exe"),
        "linux" => &|name| name.ends_with("linux-x86_64.tar.gz"),
        _ => return None,
    };
    names.iter().position(|name| suffix(name))
}

/// Asks GitHub for the latest release and compares it with this binary.
pub fn check() -> Result<UpdateCheck> {
    let url = format!("https://api.github.com/repos/{REPO}/releases/latest");
    let body = ureq::get(&url)
        .config()
        .timeout_global(Some(TIMEOUT))
        .build()
        .header("User-Agent", USER_AGENT)
        .header("Accept", "application/vnd.github+json")
        .call()
        .context("checking for updates")?
        .body_mut()
        .read_to_vec()?;

    let json: serde_json::Value = serde_json::from_slice(&body)?;
    let tag = json["tag_name"]
        .as_str()
        .ok_or_else(|| anyhow!("release has no tag"))?
        .to_string();

    if !is_newer(&tag) {
        return Ok(UpdateCheck::UpToDate);
    }

    let assets = json["assets"].as_array().cloned().unwrap_or_default();
    let names: Vec<&str> = assets
        .iter()
        .filter_map(|asset| asset["name"].as_str())
        .collect();
    let ix = select_asset(std::env::consts::OS, &names).ok_or_else(|| {
        anyhow!(
            "{tag} is out but has no build for this platform; \
             see https://github.com/{REPO}/releases"
        )
    })?;

    let asset = &assets[ix];
    Ok(UpdateCheck::Available(UpdateInfo {
        tag,
        asset_name: asset["name"].as_str().unwrap_or_default().to_string(),
        download_url: asset["browser_download_url"]
            .as_str()
            .ok_or_else(|| anyhow!("asset has no download URL"))?
            .to_string(),
        size: asset["size"].as_u64().unwrap_or(0),
    }))
}

fn download(info: &UpdateInfo, progress: &dyn Fn(u64, u64)) -> Result<PathBuf> {
    let dir = crate::paths::cache_dir()?.join("updates");
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(&info.asset_name);

    let mut response = ureq::get(&info.download_url)
        .config()
        .timeout_global(Some(Duration::from_secs(600)))
        .build()
        .header("User-Agent", USER_AGENT)
        .call()
        .with_context(|| format!("downloading {}", info.asset_name))?;

    let temp = path.with_extension("part");
    let mut reader = response.body_mut().as_reader();
    let mut file = std::io::BufWriter::new(std::fs::File::create(&temp)?);
    let mut buffer = vec![0u8; 64 * 1024];
    let mut written = 0u64;
    loop {
        let read = std::io::Read::read(&mut reader, &mut buffer)?;
        if read == 0 {
            break;
        }
        file.write_all(&buffer[..read])?;
        written += read as u64;
        progress(written, info.size);
    }
    file.flush()?;
    drop(file);

    if info.size > 0 && written != info.size {
        let _ = std::fs::remove_file(&temp);
        bail!(
            "update download truncated: got {written} of {} bytes",
            info.size
        );
    }
    std::fs::rename(&temp, &path)?;
    Ok(path)
}

/// Downloads and applies an update. `progress` receives
/// `(bytes_so_far, total_bytes)`.
pub fn apply(info: &UpdateInfo, progress: impl Fn(u64, u64)) -> Result<Applied> {
    let downloaded = download(info, &progress)?;

    #[cfg(target_os = "windows")]
    {
        // Hand over to the installer; it closes the app and swaps the files.
        std::process::Command::new(&downloaded)
            .spawn()
            .with_context(|| format!("launching {}", downloaded.display()))?;
        Ok(Applied::InstallerLaunched)
    }

    #[cfg(not(target_os = "windows"))]
    {
        replace_current_binary(&downloaded)?;
        Ok(Applied::RestartRequired)
    }
}

/// Unpacks a release tarball and swaps the running binary for the new one.
/// The old file keeps running from its open inode; the new one is picked up
/// on the next launch.
#[cfg(not(target_os = "windows"))]
fn replace_current_binary(archive: &Path) -> Result<()> {
    let staging = crate::paths::cache_dir()?.join("updates/unpacked");
    if staging.exists() {
        std::fs::remove_dir_all(&staging)?;
    }
    std::fs::create_dir_all(&staging)?;

    let file = std::fs::File::open(archive)?;
    let mut tarball = tar::Archive::new(flate2::read::GzDecoder::new(file));
    tarball
        .unpack(&staging)
        .with_context(|| format!("unpacking {}", archive.display()))?;

    let new_binary = find_binary(&staging)
        .ok_or_else(|| anyhow!("no optiscaler-manager binary inside {}", archive.display()))?;

    let current = std::env::current_exe().context("locating the running binary")?;
    let backup = current.with_extension("old");
    let _ = std::fs::remove_file(&backup);
    std::fs::rename(&current, &backup)
        .with_context(|| format!("moving {} aside", current.display()))?;

    // The cache and the install location may be different filesystems, so a
    // rename can fail with EXDEV; fall back to a copy.
    if std::fs::rename(&new_binary, &current).is_err() {
        std::fs::copy(&new_binary, &current)
            .with_context(|| format!("writing {}", current.display()))?;
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&current, std::fs::Permissions::from_mode(0o755))?;
    }
    Ok(())
}

#[cfg(not(target_os = "windows"))]
fn find_binary(dir: &Path) -> Option<PathBuf> {
    fn walk(dir: &Path, depth: usize) -> Option<PathBuf> {
        if depth > 3 {
            return None;
        }
        for entry in std::fs::read_dir(dir).ok()?.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if let Some(found) = walk(&path, depth + 1) {
                    return Some(found);
                }
            } else if entry.file_name() == "optiscaler-manager" {
                return Some(path);
            }
        }
        None
    }
    walk(dir, 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_versions_numerically() {
        assert!(parse_version("v0.10.0") > parse_version("v0.9.9"));
        assert!(parse_version("1.0.0") > parse_version("v0.99.99"));
        assert_eq!(parse_version("v0.2.0"), Some((0, 2, 0)));
        assert_eq!(parse_version("v0.2.1-beta"), Some((0, 2, 1)));
        assert_eq!(parse_version("not-a-version"), None);
    }

    #[test]
    fn newer_means_strictly_newer() {
        // CURRENT_VERSION compares against itself as not newer.
        assert!(!is_newer(CURRENT_VERSION));
        assert!(!is_newer(&format!("v{CURRENT_VERSION}")));
        assert!(is_newer("v999.0.0"));
        assert!(!is_newer("v0.0.1"));
        assert!(!is_newer("garbage"));
    }

    #[test]
    fn selects_the_platform_asset() {
        let names = [
            "optiscaler-manager-v0.2.0-linux-x86_64.tar.gz",
            "optiscaler-manager-v0.2.0-windows-x86_64-setup.exe",
            "optiscaler-manager-v0.2.0-windows-x86_64.zip",
        ];

        assert_eq!(select_asset("linux", &names), Some(0));
        // The installer, never the zip: a running exe cannot replace itself.
        assert_eq!(select_asset("windows", &names), Some(1));
        assert_eq!(select_asset("macos", &names), None);
    }

    #[test]
    fn windows_without_an_installer_asset_selects_nothing() {
        let names = ["optiscaler-manager-v0.2.0-windows-x86_64.zip"];
        assert_eq!(select_asset("windows", &names), None);
    }
}
