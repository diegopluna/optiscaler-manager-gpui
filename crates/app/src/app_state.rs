use std::collections::HashMap;
use std::path::{Path, PathBuf};

use gpui::{App, AppContext, Context, Entity};
use opti_core::optiscaler::{InstallStatus, Release, install as installer};
use opti_core::{Game, GameId, Settings};

/// How many covers to download at once. Enough to keep the grid filling in
/// quickly without hammering the CDN.
const ARTWORK_LANES: usize = 4;

/// Where the library scan currently stands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScanState {
    Idle,
    Scanning,
    Ready,
}

impl ScanState {
    pub fn is_scanning(&self) -> bool {
        matches!(self, ScanState::Scanning)
    }
}

/// Everything we know about one game's OptiScaler install.
#[derive(Debug, Clone)]
pub struct GameStatus {
    /// The directory OptiScaler goes in, after any user override.
    pub target_dir: PathBuf,
    pub install: InstallStatus,
    /// Set while an install or uninstall is running, describing the step.
    pub busy: Option<String>,
    pub error: Option<String>,
}

impl GameStatus {
    pub fn is_busy(&self) -> bool {
        self.busy.is_some()
    }
}

/// Application-wide state, held in a single entity that every view observes.
pub struct AppState {
    pub games: Vec<Game>,
    pub scan: ScanState,
    pub settings: Settings,
    /// The newest OptiScaler release, once it has been looked up.
    pub latest_release: Option<Release>,
    pub release_error: Option<String>,
    artwork: HashMap<GameId, PathBuf>,
    statuses: HashMap<GameId, GameStatus>,
}

impl AppState {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let mut this = AppState {
            games: Vec::new(),
            scan: ScanState::Idle,
            settings: Settings::load(),
            latest_release: None,
            release_error: None,
            artwork: HashMap::new(),
            statuses: HashMap::new(),
        };
        this.rescan(cx);
        this.refresh_latest_release(cx);
        this
    }

    pub fn entity(cx: &mut App) -> Entity<Self> {
        cx.new(AppState::new)
    }

    pub fn game(&self, id: &GameId) -> Option<&Game> {
        self.games.iter().find(|game| &game.id == id)
    }

    pub fn artwork_for(&self, id: &GameId) -> Option<&Path> {
        self.artwork.get(id).map(PathBuf::as_path)
    }

    pub fn status_for(&self, id: &GameId) -> Option<&GameStatus> {
        self.statuses.get(id)
    }

    /// The proxy DLL name to use for a game: whatever is installed, else the
    /// user's saved choice, else the default.
    pub fn proxy_name_for(&self, id: &GameId) -> String {
        if let Some(GameStatus {
            install: InstallStatus::Managed(manifest),
            ..
        }) = self.statuses.get(id)
        {
            return manifest.proxy_name.clone();
        }
        self.settings
            .proxy_names
            .get(id)
            .cloned()
            .unwrap_or_else(|| installer::DEFAULT_PROXY_DLL.to_string())
    }

    pub fn set_proxy_name(&mut self, id: &GameId, name: &str, cx: &mut Context<Self>) {
        self.settings
            .proxy_names
            .insert(id.clone(), name.to_string());
        self.save_settings();
        cx.notify();
    }

    /// Overrides where OptiScaler is installed for a game.
    pub fn set_target_dir(&mut self, id: &GameId, dir: PathBuf, cx: &mut Context<Self>) {
        self.settings
            .exe_dir_overrides
            .insert(id.clone(), dir.clone());
        self.save_settings();
        if let Some(status) = self.statuses.get_mut(id) {
            status.target_dir = dir.clone();
            status.install = installer::status(&dir);
        }
        cx.notify();
    }

    pub fn set_steamgriddb_key(&mut self, key: Option<String>, cx: &mut Context<Self>) {
        self.settings.steamgriddb_key = key;
        self.save_settings();
        self.fetch_artwork(cx);
        cx.notify();
    }

    fn save_settings(&self) {
        if let Err(err) = self.settings.save() {
            log::warn!("could not save settings: {err:#}");
        }
    }

    /// Rescans every storefront off the main thread. Repeat calls while a scan
    /// is in flight are ignored.
    pub fn rescan(&mut self, cx: &mut Context<Self>) {
        if self.scan.is_scanning() {
            return;
        }
        self.scan = ScanState::Scanning;
        cx.notify();

        let scan = cx.background_spawn(async move { opti_core::detect_all() });

        cx.spawn(async move |this, cx| {
            let games = scan.await;
            this.update(cx, |this, cx| {
                log::info!("detected {} games", games.len());
                this.games = games;
                this.scan = ScanState::Ready;
                this.fetch_artwork(cx);
                this.refresh_statuses(cx);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Looks up the newest OptiScaler release so the UI can offer it.
    pub fn refresh_latest_release(&mut self, cx: &mut Context<Self>) {
        let lookup = cx.background_spawn(async move { opti_core::optiscaler::latest_release() });

        cx.spawn(async move |this, cx| {
            let result = lookup.await;
            this.update(cx, |this, cx| {
                match result {
                    Ok(release) => {
                        log::info!("latest OptiScaler release: {}", release.tag);
                        this.latest_release = Some(release);
                        this.release_error = None;
                    }
                    Err(err) => {
                        log::warn!("could not look up latest release: {err:#}");
                        this.release_error = Some(format!("{err:#}"));
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Works out each game's install directory and whether OptiScaler is in it.
    fn refresh_statuses(&mut self, cx: &mut Context<Self>) {
        let overrides = self.settings.exe_dir_overrides.clone();
        let games = self.games.clone();

        let scan = cx.background_spawn(async move {
            games
                .into_iter()
                .map(|game| {
                    let target = overrides
                        .get(&game.id)
                        .cloned()
                        .unwrap_or_else(|| opti_core::exe_heuristics::install_target(&game));
                    let install = installer::status(&target);
                    (game.id, target, install)
                })
                .collect::<Vec<_>>()
        });

        cx.spawn(async move |this, cx| {
            let results = scan.await;
            this.update(cx, |this, cx| {
                for (id, target_dir, install) in results {
                    // Do not stomp on a game that is mid-install.
                    if this.statuses.get(&id).is_some_and(GameStatus::is_busy) {
                        continue;
                    }
                    this.statuses.insert(
                        id,
                        GameStatus {
                            target_dir,
                            install,
                            busy: None,
                            error: None,
                        },
                    );
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Downloads (if needed) and installs OptiScaler into a game.
    pub fn install(&mut self, id: &GameId, cx: &mut Context<Self>) {
        let Some(release) = self.latest_release.clone() else {
            self.set_error(id, "No OptiScaler release available yet.".into(), cx);
            return;
        };
        let proxy = self.proxy_name_for(id);

        let Some(status) = self.statuses.get_mut(id) else {
            return;
        };
        if status.is_busy() {
            return;
        }

        let target = status.target_dir.clone();
        let id = id.clone();

        status.busy = Some("Preparing…".into());
        status.error = None;
        cx.notify();

        let work = cx.background_spawn(async move {
            let payload = opti_core::optiscaler::prepare_payload(&release, |_, _| {})?;
            installer::install(&payload, &target, &proxy, &release.tag)
        });

        cx.spawn(async move |this, cx| {
            let result = work.await;
            this.update(cx, |this, cx| {
                if let Some(status) = this.statuses.get_mut(&id) {
                    status.busy = None;
                    match result {
                        Ok(manifest) => {
                            log::info!("installed {} into {}", manifest.release_tag, id);
                            status.install = InstallStatus::Managed(Box::new(manifest));
                        }
                        Err(err) => {
                            log::error!("install failed for {id}: {err:#}");
                            status.error = Some(format!("{err:#}"));
                        }
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Removes a managed install, keeping the user's `OptiScaler.ini`.
    pub fn uninstall(&mut self, id: &GameId, cx: &mut Context<Self>) {
        let Some(status) = self.statuses.get_mut(id) else {
            return;
        };
        if status.is_busy() {
            return;
        }

        let target = status.target_dir.clone();
        let id = id.clone();

        status.busy = Some("Removing…".into());
        status.error = None;
        cx.notify();

        let work = cx.background_spawn(async move { installer::uninstall(&target, false) });

        cx.spawn(async move |this, cx| {
            let result = work.await;
            this.update(cx, |this, cx| {
                if let Some(status) = this.statuses.get_mut(&id) {
                    status.busy = None;
                    match result {
                        Ok(report) => {
                            if !report.kept_modified.is_empty() {
                                status.error = Some(format!(
                                    "Left {} modified file(s) in place: {}",
                                    report.kept_modified.len(),
                                    report.kept_modified.join(", ")
                                ));
                            }
                            status.install = installer::status(&status.target_dir);
                        }
                        Err(err) => status.error = Some(format!("{err:#}")),
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn set_error(&mut self, id: &GameId, message: String, cx: &mut Context<Self>) {
        if let Some(status) = self.statuses.get_mut(id) {
            status.error = Some(message);
        }
        cx.notify();
    }

    /// Downloads any covers we do not already have, a few at a time, updating
    /// the grid as each one lands.
    fn fetch_artwork(&mut self, cx: &mut Context<Self>) {
        let key = self.settings.steamgriddb_key().map(str::to_owned);

        let pending: Vec<Game> = self
            .games
            .iter()
            .filter(|game| !self.artwork.contains_key(&game.id))
            .cloned()
            .collect();

        // Fill in anything already on disk immediately, so a restart shows the
        // catalog without touching the network.
        let mut remaining = Vec::new();
        for game in pending {
            match opti_core::artwork::cached(&game.id) {
                Some(path) => {
                    self.artwork.insert(game.id.clone(), path);
                }
                None => remaining.push(game),
            }
        }
        if remaining.is_empty() {
            return;
        }
        log::info!("fetching artwork for {} games", remaining.len());

        for lane in 0..ARTWORK_LANES.min(remaining.len()) {
            let games: Vec<Game> = remaining
                .iter()
                .skip(lane)
                .step_by(ARTWORK_LANES)
                .cloned()
                .collect();
            let key = key.clone();

            cx.spawn(async move |this, cx| {
                for game in games {
                    let key = key.clone();
                    let title = game.title.clone();
                    let id = game.id.clone();

                    let Ok(task) = cx.update(|cx| {
                        cx.background_spawn(async move {
                            opti_core::artwork::fetch(&game, key.as_deref())
                        })
                    }) else {
                        return;
                    };

                    match task.await {
                        Ok(Some(path)) => {
                            if this
                                .update(cx, |this, cx| {
                                    this.artwork.insert(id, path);
                                    cx.notify();
                                })
                                .is_err()
                            {
                                return; // Window closed; stop the lane.
                            }
                        }
                        Ok(None) => log::debug!("no artwork found for {title}"),
                        Err(err) => log::warn!("artwork for {title}: {err:#}"),
                    }
                }
            })
            .detach();
        }
    }
}
