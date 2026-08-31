//! Resolves a portrait cover image for a game and caches it on disk.
//!
//! Order of preference: an already-cached file, Steam's CDN (free, no key,
//! only for Steam games), then SteamGridDB (needs a user-supplied key). When
//! everything misses the caller draws its own placeholder, so a failure here
//! is never fatal.

use std::io::{Cursor, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result};
use image::imageops::FilterType;

use crate::model::{Game, GameId};
use crate::paths::artwork_dir;

/// Covers are stored at roughly half the source resolution: enough for the
/// 180pt cards, and a big cut in memory when hundreds are on screen.
const MAX_WIDTH: u32 = 300;
const MAX_HEIGHT: u32 = 450;

const TIMEOUT: Duration = Duration::from_secs(20);
const USER_AGENT: &str = concat!("optiscaler-manager/", env!("CARGO_PKG_VERSION"));

/// Where the cover for `id` lives once downloaded.
pub fn cache_path(id: &GameId) -> Result<PathBuf> {
    Ok(artwork_dir()?.join(format!("{id}.jpg")))
}

/// The cached cover, if one has already been downloaded.
pub fn cached(id: &GameId) -> Option<PathBuf> {
    let path = cache_path(id).ok()?;
    path.is_file().then_some(path)
}

/// Fetches and caches a cover. Returns `Ok(None)` when no source had art,
/// which is an ordinary outcome rather than an error.
pub fn fetch(game: &Game, steamgriddb_key: Option<&str>) -> Result<Option<PathBuf>> {
    if let Some(path) = cached(&game.id) {
        return Ok(Some(path));
    }

    let mut bytes = None;

    if let Some(app_id) = game.steam_app_id {
        bytes = steam_cdn_cover(app_id);
    }

    if bytes.is_none()
        && let Some(key) = steamgriddb_key.map(str::trim).filter(|k| !k.is_empty())
    {
        bytes = steamgriddb_cover(&game.title, key)?;
    }

    let Some(bytes) = bytes else {
        return Ok(None);
    };

    let path = cache_path(&game.id)?;
    store(&bytes, &path)
        .with_context(|| format!("writing artwork for {} to {}", game.title, path.display()))?;
    Ok(Some(path))
}

/// Decodes, downscales and writes the cover as JPEG.
fn store(bytes: &[u8], path: &Path) -> Result<()> {
    let image = image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()?
        .decode()?;

    let image = if image.width() > MAX_WIDTH || image.height() > MAX_HEIGHT {
        image.resize(MAX_WIDTH, MAX_HEIGHT, FilterType::CatmullRom)
    } else {
        image
    };

    // Write to a temporary file first so a cancelled download can never leave
    // a half-written image that later loads as a corrupt cover.
    let temp = path.with_extension("jpg.part");
    {
        let mut file = std::io::BufWriter::new(std::fs::File::create(&temp)?);
        // The format is explicit because `.part` tells the encoder nothing.
        image::DynamicImage::ImageRgb8(image.to_rgb8())
            .write_to(&mut file, image::ImageFormat::Jpeg)?;
        file.flush()?;
    }
    std::fs::rename(&temp, path)?;
    Ok(())
}

fn get_bytes(url: &str, headers: &[(&str, &str)]) -> Result<Option<Vec<u8>>> {
    let mut request = ureq::get(url)
        .config()
        .timeout_global(Some(TIMEOUT))
        .build()
        .header("User-Agent", USER_AGENT);

    for (name, value) in headers {
        request = request.header(*name, *value);
    }

    match request.call() {
        Ok(mut response) => Ok(Some(response.body_mut().read_to_vec()?)),
        // A missing cover is expected for delisted or obscure titles.
        Err(ureq::Error::StatusCode(404)) => Ok(None),
        Err(err) => Err(err.into()),
    }
}

/// Steam's public CDN. `library_600x900` is the portrait store card; the
/// landscape `header` is the fallback for older apps that lack one.
fn steam_cdn_cover(app_id: u32) -> Option<Vec<u8>> {
    let base = "https://cdn.cloudflare.steamstatic.com/steam/apps";
    for file in [
        "library_600x900_2x.jpg",
        "library_600x900.jpg",
        "header.jpg",
    ] {
        match get_bytes(&format!("{base}/{app_id}/{file}"), &[]) {
            Ok(Some(bytes)) if !bytes.is_empty() => return Some(bytes),
            Ok(_) => continue,
            Err(err) => {
                log::debug!("steam cdn {file} for {app_id}: {err}");
                continue;
            }
        }
    }
    None
}

/// SteamGridDB: resolve the title to a game id, then take its first portrait
/// grid.
fn steamgriddb_cover(title: &str, key: &str) -> Result<Option<Vec<u8>>> {
    let auth = format!("Bearer {key}");
    let headers = [("Authorization", auth.as_str())];

    let search_url = format!(
        "https://www.steamgriddb.com/api/v2/search/autocomplete/{}",
        urlencode(title)
    );
    let Some(body) = get_bytes(&search_url, &headers)? else {
        return Ok(None);
    };

    let search: serde_json::Value = serde_json::from_slice(&body)?;
    let Some(game_id) = search["data"][0]["id"].as_i64() else {
        return Ok(None);
    };

    let grids_url =
        format!("https://www.steamgriddb.com/api/v2/grids/game/{game_id}?dimensions=600x900");
    let Some(body) = get_bytes(&grids_url, &headers)? else {
        return Ok(None);
    };

    let grids: serde_json::Value = serde_json::from_slice(&body)?;
    let Some(url) = grids["data"][0]["url"].as_str() else {
        return Ok(None);
    };

    get_bytes(url, &[])
}

/// Percent-encodes a path segment. Game titles routinely contain spaces,
/// colons and ampersands.
fn urlencode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_titles_for_urls() {
        assert_eq!(urlencode("The Witcher 3"), "The%20Witcher%203");
        assert_eq!(urlencode("S.T.A.L.K.E.R. 2"), "S.T.A.L.K.E.R.%202");
        assert_eq!(urlencode("Tom Clancy's"), "Tom%20Clancy%27s");
    }

    #[test]
    fn downscales_oversized_covers() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("cover.jpg");

        let source = image::RgbImage::from_pixel(600, 900, image::Rgb([10, 120, 200]));
        let mut bytes = Vec::new();
        image::DynamicImage::ImageRgb8(source)
            .write_to(&mut Cursor::new(&mut bytes), image::ImageFormat::Png)
            .unwrap();

        store(&bytes, &path).unwrap();

        let written = image::open(&path).unwrap();
        assert!(written.width() <= MAX_WIDTH && written.height() <= MAX_HEIGHT);
        assert!(
            !path.with_extension("jpg.part").exists(),
            "temp file cleaned up"
        );
    }
}
