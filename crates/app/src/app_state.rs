use std::collections::HashMap;
use std::path::{Path, PathBuf};

use gpui::{App, AppContext, Context, Entity};
use opti_core::anticheat::Detection;
use opti_core::gpu::GpuInfo;
use opti_core::optiscaler::{InstallStatus, Release, install as installer};
use opti_core::update::{Applied, UpdateCheck, UpdateInfo};
use opti_core::{Game, GameId, Settings};

/// What the background install work produced.
enum InstallFlow {
    Done(Box<opti_core::optiscaler::install::InstallManifest>),
    NeedsConfirmation(Vec<String>),
}

/// How many covers to download at once. Enough to keep the grid filling in
/// quickly without hammering the CDN.
const ARTWORK_LANES: usize = 4;

/// Where the self-update flow currently stands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateState {
    Idle,
    Checking,
    UpToDate,
    Available(UpdateInfo),
    Downloading,
    /// Windows only: the silent installer is replacing the app, which will
    /// close and reopen by itself in a few seconds.
    Installing,
    /// The new binary is in place; it runs on the next launch.
    RestartRequired,
    Failed(String),
}

impl UpdateState {
    pub fn is_busy(&self) -> bool {
        matches!(
            self,
            UpdateState::Checking | UpdateState::Downloading | UpdateState::Installing
        )
    }
}

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
    /// Anti-cheat software found in the game. Empty means nothing was found
    /// on disk, which does not rule out server-side systems like VAC.
    pub anticheat: Vec<Detection>,
    /// Files an install would displace, waiting for the user's go-ahead.
    pub conflicts: Option<Vec<String>>,
    /// Upscaler runtime DLLs the game itself ships (ours excluded).
    pub upscalers: Vec<opti_core::upscalers::Detection>,
    /// The supported executable name when OptiPatcher can patch this game.
    pub optipatcher_supported: Option<String>,
    pub optipatcher_installed: bool,
    /// Set while an install or uninstall is running, describing the step.
    pub busy: Option<String>,
    pub error: Option<String>,
}

impl GameStatus {
    pub fn is_busy(&self) -> bool {
        self.busy.is_some()
    }

    pub fn has_anticheat(&self) -> bool {
        !self.anticheat.is_empty()
    }

    /// The detected systems, for a one-line summary.
    pub fn anticheat_names(&self) -> String {
        self.anticheat
            .iter()
            .map(|detection| detection.name)
            .collect::<Vec<_>>()
            .join(", ")
    }
}

/// Application-wide state, held in a single entity that every view observes.
pub struct AppState {
    pub games: Vec<Game>,
    pub scan: ScanState,
    pub settings: Settings,
    /// Published OptiScaler releases, newest first.
    pub releases: Vec<Release>,
    pub release_error: Option<String>,
    artwork: HashMap<GameId, PathBuf>,
    statuses: HashMap<GameId, GameStatus>,
    /// Lowercase executable names OptiPatcher supports, once fetched.
    optipatcher_exes: Option<Vec<String>>,
    pub update: UpdateState,
    /// The GPU games render on, when detection recognised one.
    pub gpu: Option<GpuInfo>,
    /// Release tag the user picked per game; absent means "use the newest".
    selected_release: HashMap<GameId, String>,
}

impl AppState {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let mut this = AppState {
            games: Vec::new(),
            scan: ScanState::Idle,
            settings: Settings::load(),
            releases: Vec::new(),
            release_error: None,
            artwork: HashMap::new(),
            statuses: HashMap::new(),
            selected_release: HashMap::new(),
            optipatcher_exes: None,
            update: UpdateState::Idle,
            gpu: None,
        };
        this.rescan(cx);
        this.refresh_releases(cx);
        this.refresh_optipatcher_list(cx);
        this.detect_gpu(cx);
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

    /// How many games have OptiScaler in them right now.
    pub fn installed_count(&self) -> usize {
        self.statuses
            .values()
            .filter(|status| status.install.is_installed())
            .count()
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

    /// Adds a single game folder. Returns an error message instead when the
    /// path is unusable or already covered.
    pub fn add_manual_game(&mut self, dir: PathBuf, cx: &mut Context<Self>) -> Result<(), String> {
        if !dir.is_dir() {
            return Err(format!("{} is not a folder", dir.display()));
        }
        if opti_core::detect::is_store_detected(&self.games, &dir) {
            return Err("That game is already detected from its store.".into());
        }
        if self.settings.manual_games.contains(&dir) {
            return Err("That folder is already in the list.".into());
        }
        self.settings.manual_games.push(dir);
        self.save_settings();
        self.rescan(cx);
        Ok(())
    }

    pub fn remove_manual_game(&mut self, dir: &PathBuf, cx: &mut Context<Self>) {
        self.settings.manual_games.retain(|d| d != dir);
        self.save_settings();
        self.rescan(cx);
    }

    /// Adds a library folder whose subdirectories are scanned as games.
    pub fn add_scan_folder(&mut self, dir: PathBuf, cx: &mut Context<Self>) -> Result<(), String> {
        if !dir.is_dir() {
            return Err(format!("{} is not a folder", dir.display()));
        }
        if self.settings.scan_folders.contains(&dir) {
            return Err("That folder is already scanned.".into());
        }
        self.settings.scan_folders.push(dir);
        self.save_settings();
        self.rescan(cx);
        Ok(())
    }

    pub fn remove_scan_folder(&mut self, dir: &PathBuf, cx: &mut Context<Self>) {
        self.settings.scan_folders.retain(|d| d != dir);
        self.save_settings();
        self.rescan(cx);
    }

    /// Saves the theme choice; the caller applies it to the window.
    pub fn set_theme(&mut self, theme: Option<String>, cx: &mut Context<Self>) {
        self.settings.theme = theme;
        self.save_settings();
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

        let manual_dirs = self.settings.manual_games.clone();
        let scan_folders = self.settings.scan_folders.clone();
        let scan =
            cx.background_spawn(async move { opti_core::detect_all(&manual_dirs, &scan_folders) });

        cx.spawn(async move |this, cx| {
            let games = scan.await;
            this.update(cx, |this, cx| {
                log::info!("detected {} games", games.len());
                this.games = games;
                this.scan = ScanState::Ready;
                this.fetch_artwork(cx);
                this.refresh_statuses(cx);
                this.resolve_steam_app_types(cx);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Looks up the published OptiScaler releases so the user can pick one.
    pub fn refresh_releases(&mut self, cx: &mut Context<Self>) {
        let lookup = cx.background_spawn(async move { opti_core::optiscaler::list_releases() });

        cx.spawn(async move |this, cx| {
            let result = lookup.await;
            this.update(cx, |this, cx| {
                match result {
                    Ok(releases) => {
                        log::info!("found {} OptiScaler releases", releases.len());
                        this.releases = releases;
                        this.release_error = None;
                    }
                    Err(err) => {
                        log::warn!("could not list releases: {err:#}");
                        this.release_error = Some(format!("{err:#}"));
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// The newest stable release, which is what a game installs by default.
    pub fn latest_release(&self) -> Option<&Release> {
        self.releases
            .iter()
            .find(|release| !release.prerelease)
            .or_else(|| self.releases.first())
    }

    /// The release a game will install: the user's pick, else the newest.
    pub fn release_for(&self, id: &GameId) -> Option<&Release> {
        match self.selected_release.get(id) {
            Some(tag) => self
                .releases
                .iter()
                .find(|release| &release.tag == tag)
                .or_else(|| self.latest_release()),
            None => self.latest_release(),
        }
    }

    /// Pins a game to a specific OptiScaler release.
    pub fn select_release(&mut self, id: &GameId, tag: &str, cx: &mut Context<Self>) {
        self.selected_release.insert(id.clone(), tag.to_string());
        cx.notify();
    }

    /// Detects the GPU once at startup; registry/sysfs reads, so background.
    fn detect_gpu(&mut self, cx: &mut Context<Self>) {
        let detect = cx.background_spawn(async move { opti_core::gpu::detect() });
        cx.spawn(async move |this, cx| {
            let gpu = detect.await;
            this.update(cx, |this, cx| {
                if let Some(gpu) = &gpu {
                    log::info!("detected GPU: {gpu}");
                }
                this.gpu = gpu;
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Whether DLSS inputs (Nvidia spoofing) are enabled for a game. On by
    /// default, matching OptiScaler's own setup script.
    pub fn dlss_inputs_enabled(&self, id: &GameId) -> bool {
        !self.settings.dlss_inputs_disabled.contains(id)
    }

    /// Flips the DLSS-inputs choice for a game. Persisted always; when the
    /// game already has a managed install, its ini is edited right away.
    pub fn set_dlss_inputs(&mut self, id: &GameId, enabled: bool, cx: &mut Context<Self>) {
        if enabled {
            self.settings.dlss_inputs_disabled.retain(|d| d != id);
        } else if !self.settings.dlss_inputs_disabled.contains(id) {
            self.settings.dlss_inputs_disabled.push(id.clone());
        }
        self.save_settings();

        let Some(status) = self.statuses.get(id) else {
            cx.notify();
            return;
        };
        if !matches!(status.install, InstallStatus::Managed(_)) {
            cx.notify();
            return;
        }

        let target = status.target_dir.clone();
        let id = id.clone();
        let work = cx.background_spawn(async move {
            opti_core::optiscaler::ini_edit::set_dxgi_spoofing(&target, enabled)
        });
        cx.spawn(async move |this, cx| {
            let result = work.await;
            this.update(cx, |this, cx| {
                if let Err(err) = result
                    && let Some(status) = this.statuses.get_mut(&id)
                {
                    status.error = Some(format!("{err:#}"));
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Fetches OptiPatcher's supported-game list in the background. Statuses
    /// are recomputed when it lands so the catalog can flag supported games.
    fn refresh_optipatcher_list(&mut self, cx: &mut Context<Self>) {
        let fetch = cx.background_spawn(async move {
            opti_core::optiscaler::optipatcher::fetch_supported_exes()
        });

        cx.spawn(async move |this, cx| match fetch.await {
            Ok(exes) => {
                this.update(cx, |this, cx| {
                    log::info!("OptiPatcher supports {} executables", exes.len());
                    this.optipatcher_exes = Some(exes);
                    this.refresh_statuses(cx);
                })
                .ok();
            }
            Err(err) => log::warn!("could not fetch OptiPatcher compatibility: {err:#}"),
        })
        .detach();
    }

    /// Looks up the Steam type (game / dlc / music / demo) of any appid the
    /// cache does not know yet, then drops resolved non-games from the
    /// catalog. Results are cached forever, so this only costs network the
    /// first time an app is seen; failures leave the entry visible.
    fn resolve_steam_app_types(&mut self, cx: &mut Context<Self>) {
        let cache = opti_core::detect::steam_apptype::load_cache();
        let unknown: Vec<u32> = self
            .games
            .iter()
            .filter_map(|game| game.steam_app_id)
            .filter(|id| !cache.contains_key(id))
            .collect();
        if unknown.is_empty() {
            return;
        }
        log::info!("resolving app types for {} Steam entries", unknown.len());

        let work = cx.background_spawn(async move {
            let mut cache = cache;
            for app_id in unknown {
                match opti_core::detect::steam_apptype::fetch_type(app_id) {
                    Ok(Some(app_type)) => {
                        cache.insert(app_id, app_type);
                    }
                    // No store entry (delisted): call it a game so it stays,
                    // and remember that so it is never asked about again.
                    Ok(None) => {
                        cache.insert(app_id, "game".to_string());
                    }
                    // Rate limited or offline: retry on a later scan.
                    Err(err) => log::warn!("app type lookup for {app_id}: {err:#}"),
                }
            }
            if let Err(err) = opti_core::detect::steam_apptype::save_cache(&cache) {
                log::warn!("could not save the app type cache: {err:#}");
            }
            cache
        });

        cx.spawn(async move |this, cx| {
            let cache = work.await;
            this.update(cx, |this, cx| {
                let before = this.games.len();
                this.games.retain(|game| {
                    game.steam_app_id.is_none_or(|id| {
                        !opti_core::detect::steam_apptype::is_known_non_game(&cache, id)
                    })
                });
                let hidden = before - this.games.len();
                if hidden > 0 {
                    log::info!("hid {hidden} DLC/soundtrack/demo entries");
                    let ids: std::collections::HashSet<GameId> =
                        this.games.iter().map(|game| game.id.clone()).collect();
                    this.statuses.retain(|id, _| ids.contains(id));
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    /// Works out each game's install directory and whether OptiScaler is in it.
    fn refresh_statuses(&mut self, cx: &mut Context<Self>) {
        let overrides = self.settings.exe_dir_overrides.clone();
        let games = self.games.clone();
        let optipatcher_exes = self.optipatcher_exes.clone().unwrap_or_default();

        let scan = cx.background_spawn(async move {
            games
                .into_iter()
                .map(|game| {
                    let target = overrides
                        .get(&game.id)
                        .cloned()
                        .unwrap_or_else(|| opti_core::exe_heuristics::install_target(&game));
                    let install = installer::status(&target);
                    // Scan the whole install, not just the target directory:
                    // anti-cheat often sits beside the launcher, a level up
                    // from the executable OptiScaler hooks.
                    let anticheat = opti_core::anticheat::scan(&game.install_dir);
                    let optipatcher_supported = opti_core::optiscaler::optipatcher::matching_exe(
                        &target,
                        &optipatcher_exes,
                    );
                    let optipatcher_installed =
                        opti_core::optiscaler::optipatcher::is_installed(&target);
                    // Files we installed must not read as the game's own
                    // upscaler support; the manifest lists exactly those.
                    let ours: std::collections::HashSet<std::path::PathBuf> = match &install {
                        InstallStatus::Managed(manifest) => manifest
                            .files
                            .iter()
                            .map(|file| target.join(&file.path))
                            .collect(),
                        _ => Default::default(),
                    };
                    let upscalers = opti_core::upscalers::scan(&game.install_dir, &ours);
                    (
                        game.id,
                        target,
                        install,
                        anticheat,
                        optipatcher_supported,
                        optipatcher_installed,
                        upscalers,
                    )
                })
                .collect::<Vec<_>>()
        });

        cx.spawn(async move |this, cx| {
            let results = scan.await;
            this.update(cx, |this, cx| {
                for (
                    id,
                    target_dir,
                    install,
                    anticheat,
                    optipatcher_supported,
                    optipatcher_installed,
                    upscalers,
                ) in results
                {
                    // Do not stomp on a game that is mid-install.
                    if this.statuses.get(&id).is_some_and(GameStatus::is_busy) {
                        continue;
                    }
                    if !upscalers.is_empty() {
                        log::info!(
                            "{id}: ships {}",
                            opti_core::upscalers::techs(&upscalers)
                                .iter()
                                .map(|tech| tech.label())
                                .collect::<Vec<_>>()
                                .join(", ")
                        );
                    }
                    if !anticheat.is_empty() {
                        log::info!(
                            "{id}: anti-cheat detected ({})",
                            anticheat
                                .iter()
                                .map(|d| d.name)
                                .collect::<Vec<_>>()
                                .join(", ")
                        );
                    }
                    this.statuses.insert(
                        id,
                        GameStatus {
                            target_dir,
                            install,
                            anticheat,
                            conflicts: None,
                            upscalers,
                            optipatcher_supported,
                            optipatcher_installed,
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

    /// Downloads (if needed) and installs OptiScaler into a game. When the
    /// install would displace files that are not ours, it stops and parks
    /// them on the status for the UI to confirm.
    pub fn install(&mut self, id: &GameId, cx: &mut Context<Self>) {
        self.install_inner(id, false, cx);
    }

    /// Proceeds past a parked conflict: the files are backed up, replaced,
    /// and restored on uninstall.
    pub fn install_confirmed(&mut self, id: &GameId, cx: &mut Context<Self>) {
        self.install_inner(id, true, cx);
    }

    pub fn dismiss_conflicts(&mut self, id: &GameId, cx: &mut Context<Self>) {
        if let Some(status) = self.statuses.get_mut(id) {
            status.conflicts = None;
        }
        cx.notify();
    }

    fn install_inner(&mut self, id: &GameId, confirmed: bool, cx: &mut Context<Self>) {
        let Some(release) = self.release_for(id).cloned() else {
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

        let dlss_inputs = self.dlss_inputs_enabled(&id);
        let work = cx.background_spawn(async move {
            let payload = opti_core::optiscaler::prepare_payload(&release, |_, _| {})?;

            if !confirmed {
                let found = installer::conflicts(&payload, &target, &proxy)?;
                if !found.is_empty() {
                    return anyhow::Ok(InstallFlow::NeedsConfirmation(found));
                }
            }

            let manifest = installer::install(&payload, &target, &proxy, &release.tag)?;
            // The shipped default (spoofing on for AMD/Intel) is only ever
            // changed when the user opted out of DLSS inputs.
            if !dlss_inputs {
                opti_core::optiscaler::ini_edit::set_dxgi_spoofing(&target, false)?;
            }
            anyhow::Ok(InstallFlow::Done(Box::new(manifest)))
        });

        cx.spawn(async move |this, cx| {
            let result = work.await;
            this.update(cx, |this, cx| {
                if let Some(status) = this.statuses.get_mut(&id) {
                    status.busy = None;
                    match result {
                        Ok(InstallFlow::Done(manifest)) => {
                            log::info!("installed {} into {}", manifest.release_tag, id);
                            status.conflicts = None;
                            status.install = InstallStatus::Managed(manifest);
                        }
                        Ok(InstallFlow::NeedsConfirmation(found)) => {
                            log::info!("{id}: install paused on conflicts: {found:?}");
                            status.conflicts = Some(found);
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

    /// Downloads OptiPatcher into a game that already has a managed
    /// OptiScaler install, records it in the manifest, and enables ASI
    /// loading in the game's ini.
    pub fn install_optipatcher(&mut self, id: &GameId, cx: &mut Context<Self>) {
        let Some(status) = self.statuses.get_mut(id) else {
            return;
        };
        if status.is_busy() || status.optipatcher_installed {
            return;
        }

        let target = status.target_dir.clone();
        let id = id.clone();

        status.busy = Some("Adding OptiPatcher…".into());
        status.error = None;
        cx.notify();

        let work = cx.background_spawn(async move {
            let written = opti_core::optiscaler::optipatcher::install(&target)?;
            installer::record_extra_files(&target, &written)?;
            anyhow::Ok(())
        });

        cx.spawn(async move |this, cx| {
            let result = work.await;
            this.update(cx, |this, cx| {
                if let Some(status) = this.statuses.get_mut(&id) {
                    status.busy = None;
                    match result {
                        Ok(()) => {
                            log::info!("installed OptiPatcher for {id}");
                            status.optipatcher_installed = true;
                            // Re-read so the manifest count shown stays honest.
                            status.install = installer::status(&status.target_dir);
                        }
                        Err(err) => {
                            log::error!("OptiPatcher install failed for {id}: {err:#}");
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

    /// Asks GitHub whether a newer release of this app exists.
    pub fn check_for_updates(&mut self, cx: &mut Context<Self>) {
        if self.update.is_busy() {
            return;
        }
        self.update = UpdateState::Checking;
        cx.notify();

        let check = cx.background_spawn(async move { opti_core::update::check() });

        cx.spawn(async move |this, cx| {
            let result = check.await;
            this.update(cx, |this, cx| {
                this.update = match result {
                    Ok(UpdateCheck::UpToDate) => UpdateState::UpToDate,
                    Ok(UpdateCheck::Available(info)) => {
                        log::info!("update available: {}", info.tag);
                        UpdateState::Available(info)
                    }
                    Err(err) => UpdateState::Failed(format!("{err:#}")),
                };
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Downloads and applies the update found by [`Self::check_for_updates`].
    /// On Windows this launches the new installer and quits the app so the
    /// installer can replace it; on Linux the binary is swapped in place.
    pub fn apply_update(&mut self, cx: &mut Context<Self>) {
        let UpdateState::Available(info) = self.update.clone() else {
            return;
        };
        self.update = UpdateState::Downloading;
        cx.notify();

        let work = cx.background_spawn(async move { opti_core::update::apply(&info, |_, _| {}) });

        cx.spawn(async move |this, cx| {
            let result = work.await;
            this.update(cx, |this, cx| {
                match result {
                    Ok(Applied::InstallerLaunched) => {
                        // The silent installer closes this app itself via
                        // Restart Manager and relaunches it after the swap;
                        // quitting here would stop the relaunch from firing.
                        log::info!("silent installer running; waiting to be restarted");
                        this.update = UpdateState::Installing;
                    }
                    Ok(Applied::RestartRequired) => {
                        this.update = UpdateState::RestartRequired;
                    }
                    Err(err) => this.update = UpdateState::Failed(format!("{err:#}")),
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
