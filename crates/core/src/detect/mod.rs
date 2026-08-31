pub mod epic;
pub mod steam;
pub mod xbox;

use crate::model::Game;

/// Scans every supported storefront and returns the merged catalog, sorted by
/// title. Failures in one store never stop the others.
pub fn detect_all() -> Vec<Game> {
    let mut games = Vec::new();
    games.extend(steam::detect());
    games.extend(epic::detect());
    games.extend(xbox::detect());

    // A game owned on two stores is genuinely two installs, but the same
    // install dir showing up twice is not.
    games.sort_by(|a, b| {
        a.title
            .to_lowercase()
            .cmp(&b.title.to_lowercase())
            .then_with(|| a.install_dir.cmp(&b.install_dir))
    });
    games.dedup_by(|a, b| a.install_dir == b.install_dir);
    games
}
