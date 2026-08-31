//! Classifies Steam appids as game / DLC / soundtrack / demo.
//!
//! `appmanifest_*.acf` files carry no type field, and some DLC, soundtracks
//! and demos get manifests and install folders of their own — which is how
//! they end up in the catalog looking like games. Steam's public appdetails
//! endpoint does know the type, so results are fetched once per appid and
//! cached on disk forever (an app's type never changes).
//!
//! Everything here fails open: an appid whose type is unknown stays in the
//! library until a lookup succeeds, so an offline machine or a rate-limited
//! API never hides a real game.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, Result};

const USER_AGENT: &str = concat!("optiscaler-manager/", env!("CARGO_PKG_VERSION"));
const TIMEOUT: Duration = Duration::from_secs(15);

fn cache_file() -> Result<PathBuf> {
    Ok(crate::paths::config_dir()?.join("steam_app_types.json"))
}

/// The cached appid → type map (`game`, `dlc`, `music`, `demo`, ...).
pub fn load_cache() -> HashMap<u32, String> {
    let Ok(path) = cache_file() else {
        return HashMap::new();
    };
    std::fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

pub fn save_cache(cache: &HashMap<u32, String>) -> Result<()> {
    let path = cache_file()?;
    let json = serde_json::to_string_pretty(cache)?;
    let temp = path.with_extension("json.part");
    std::fs::write(&temp, json).with_context(|| format!("writing {}", temp.display()))?;
    std::fs::rename(&temp, &path)?;
    Ok(())
}

/// Whether a type string names something the catalog should show.
pub fn is_game_type(app_type: &str) -> bool {
    // Unknown or new types stay visible; only the types we positively know
    // are not playable installs get hidden.
    !matches!(app_type, "dlc" | "music" | "demo" | "video" | "advertising")
}

/// True when the cache already proves this appid is not a game.
pub fn is_known_non_game(cache: &HashMap<u32, String>, app_id: u32) -> bool {
    cache
        .get(&app_id)
        .is_some_and(|app_type| !is_game_type(app_type))
}

/// Pulls one appid's type from the store API. `Ok(None)` means the API had
/// no answer (delisted app); the caller should treat that as "game" and may
/// cache it to avoid asking again.
pub fn fetch_type(app_id: u32) -> Result<Option<String>> {
    let url =
        format!("https://store.steampowered.com/api/appdetails?appids={app_id}&filters=basic");
    let body = ureq::get(&url)
        .config()
        .timeout_global(Some(TIMEOUT))
        .build()
        .header("User-Agent", USER_AGENT)
        .call()
        .with_context(|| format!("querying app type for {app_id}"))?
        .body_mut()
        .read_to_vec()?;

    Ok(parse_type(app_id, &body))
}

fn parse_type(app_id: u32, body: &[u8]) -> Option<String> {
    let json: serde_json::Value = serde_json::from_slice(body).ok()?;
    let entry = &json[app_id.to_string()];
    if entry["success"].as_bool() != Some(true) {
        return None;
    }
    entry["data"]["type"].as_str().map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_appdetails_shape() {
        let dlc = br#"{"2778580":{"success":true,"data":{"type":"dlc","name":"Shadow"}}}"#;
        assert_eq!(parse_type(2778580, dlc).as_deref(), Some("dlc"));

        let game = br#"{"1091500":{"success":true,"data":{"type":"game"}}}"#;
        assert_eq!(parse_type(1091500, game).as_deref(), Some("game"));

        let missing = br#"{"42":{"success":false}}"#;
        assert_eq!(parse_type(42, missing), None);
    }

    #[test]
    fn only_positively_known_non_games_are_hidden() {
        assert!(is_game_type("game"));
        assert!(is_game_type("something_new"));
        assert!(!is_game_type("dlc"));
        assert!(!is_game_type("music"));
        assert!(!is_game_type("demo"));

        let mut cache = HashMap::new();
        cache.insert(1u32, "dlc".to_string());
        cache.insert(2u32, "game".to_string());
        assert!(is_known_non_game(&cache, 1));
        assert!(!is_known_non_game(&cache, 2));
        assert!(!is_known_non_game(&cache, 3), "unknown appids stay visible");
    }
}
