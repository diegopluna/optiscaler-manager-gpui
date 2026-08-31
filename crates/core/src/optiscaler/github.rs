//! Talks to the OptiScaler GitHub releases API and caches downloaded archives.

use std::io::Write;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, Serialize};

use crate::paths::releases_dir;

const RELEASES_URL: &str = "https://api.github.com/repos/optiscaler/OptiScaler/releases";
const USER_AGENT: &str = concat!("optiscaler-manager/", env!("CARGO_PKG_VERSION"));
const TIMEOUT: Duration = Duration::from_secs(60);

/// A published OptiScaler release and the archive to download for it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Release {
    pub tag: String,
    pub name: String,
    pub published_at: String,
    pub asset_name: String,
    pub download_url: String,
    pub size: u64,
    pub prerelease: bool,
}

impl Release {
    /// Local path the archive is cached at.
    pub fn archive_path(&self) -> Result<PathBuf> {
        Ok(releases_dir()?.join(&self.asset_name))
    }
}

/// The newest non-prerelease release.
pub fn latest_release() -> Result<Release> {
    let releases = list_releases()?;
    releases
        .into_iter()
        .find(|release| !release.prerelease)
        .ok_or_else(|| anyhow!("no published OptiScaler release found"))
}

/// Recent releases, newest first, so the user can pin an older build.
pub fn list_releases() -> Result<Vec<Release>> {
    let body = ureq::get(RELEASES_URL)
        .config()
        .timeout_global(Some(TIMEOUT))
        .build()
        .header("User-Agent", USER_AGENT)
        .header("Accept", "application/vnd.github+json")
        .call()
        .context("querying GitHub for OptiScaler releases")?
        .body_mut()
        .read_to_vec()?;

    let json: serde_json::Value = serde_json::from_slice(&body)?;
    let entries = json
        .as_array()
        .ok_or_else(|| anyhow!("unexpected response from GitHub releases API"))?;

    Ok(entries.iter().filter_map(parse_release).collect())
}

/// Turns one release object into a [`Release`], skipping any that carry no
/// downloadable archive.
fn parse_release(value: &serde_json::Value) -> Option<Release> {
    let assets = value["assets"].as_array()?;

    // Releases ship a single .7z; accept .zip too in case that ever changes.
    let asset = assets
        .iter()
        .find(|asset| asset_name(asset).is_some_and(|name| name.ends_with(".7z")))
        .or_else(|| {
            assets
                .iter()
                .find(|asset| asset_name(asset).is_some_and(|name| name.ends_with(".zip")))
        })?;

    let tag = value["tag_name"].as_str()?.to_string();
    Some(Release {
        name: value["name"].as_str().unwrap_or(&tag).to_string(),
        tag,
        published_at: value["published_at"]
            .as_str()
            .unwrap_or_default()
            .to_string(),
        asset_name: asset["name"].as_str()?.to_string(),
        download_url: asset["browser_download_url"].as_str()?.to_string(),
        size: asset["size"].as_u64().unwrap_or(0),
        prerelease: value["prerelease"].as_bool().unwrap_or(false),
    })
}

fn asset_name(asset: &serde_json::Value) -> Option<String> {
    Some(asset["name"].as_str()?.to_lowercase())
}

/// Downloads the release archive, reusing the cached copy when its size
/// already matches. `progress` receives `(bytes_so_far, total_bytes)`.
pub fn download(release: &Release, progress: impl Fn(u64, u64)) -> Result<PathBuf> {
    let path = release.archive_path()?;

    if let Ok(meta) = std::fs::metadata(&path)
        && (release.size == 0 || meta.len() == release.size)
    {
        log::info!("using cached {}", release.asset_name);
        return Ok(path);
    }

    log::info!(
        "downloading {} ({} bytes)",
        release.asset_name,
        release.size
    );

    let mut response = ureq::get(&release.download_url)
        .config()
        .timeout_global(Some(Duration::from_secs(600)))
        .build()
        .header("User-Agent", USER_AGENT)
        .call()
        .with_context(|| format!("downloading {}", release.download_url))?;

    let total = response
        .headers()
        .get("content-length")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(release.size);

    // Download beside the target and rename, so an interrupted download is
    // never mistaken for a complete archive on the next run.
    let temp = path.with_extension("part");
    let mut reader = response.body_mut().as_reader();
    let mut file = std::io::BufWriter::new(
        std::fs::File::create(&temp).with_context(|| format!("creating {}", temp.display()))?,
    );

    let mut buffer = vec![0u8; 64 * 1024];
    let mut written = 0u64;
    loop {
        let read = std::io::Read::read(&mut reader, &mut buffer)?;
        if read == 0 {
            break;
        }
        file.write_all(&buffer[..read])?;
        written += read as u64;
        progress(written, total);
    }
    file.flush()?;
    drop(file);

    if total > 0 && written != total {
        let _ = std::fs::remove_file(&temp);
        bail!("download truncated: got {written} of {total} bytes");
    }

    std::fs::rename(&temp, &path)?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn release_json(assets: &str, prerelease: bool) -> serde_json::Value {
        serde_json::from_str(&format!(
            r#"{{
                "tag_name": "v0.9.4",
                "name": "OptiScaler v0.9.4",
                "published_at": "2026-07-18T00:00:00Z",
                "prerelease": {prerelease},
                "assets": [{assets}]
            }}"#
        ))
        .unwrap()
    }

    fn asset(name: &str, size: u64) -> String {
        format!(
            r#"{{"name": "{name}", "size": {size},
                 "browser_download_url": "https://example.test/{name}"}}"#
        )
    }

    #[test]
    fn picks_the_seven_zip_asset() {
        let json = release_json(
            &format!(
                "{}, {}",
                asset("notes.txt", 10),
                asset("Opti_0.9.4.7z", 5000)
            ),
            false,
        );
        let release = parse_release(&json).expect("release parsed");

        assert_eq!(release.tag, "v0.9.4");
        assert_eq!(release.asset_name, "Opti_0.9.4.7z");
        assert_eq!(release.size, 5000);
        assert!(!release.prerelease);
    }

    #[test]
    fn skips_releases_without_an_archive() {
        let json = release_json(&asset("source-notes.txt", 1), false);
        assert!(parse_release(&json).is_none());
    }
}
