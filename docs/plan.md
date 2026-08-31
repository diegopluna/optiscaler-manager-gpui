# OptiScaler Manager — GPUI Desktop App

## Context

Greenfield build in an empty repo. Goal: a GPUI desktop app (Windows-first, Linux supported, developed/runnable on macOS) that autodetects installed games from Steam, Epic Games Launcher, and Xbox/Microsoft Store, catalogs them with banner artwork, and manages OptiScaler per game: install / uninstall / update / **full OptiScaler.ini config editor**.

Decisions made with the user:
- **UI stack:** `gpui` + `gpui-component` (crates.io).
- **Artwork:** Steam CDN for Steam games; SteamGridDB (optional user API key in Settings) for others; generated placeholder fallback.
- **Scope:** full config editor in v1, plus install/uninstall/update, spoof-DLL choice, add-on handling.

## Verified facts (from research — do not re-derive)

- **Pin exactly** `gpui = "=0.2.2"`, `gpui-component = "=0.5.1"`, `gpui-component-assets = "=0.5.1"` (assets crate provides embedded Lucide icons; required for `IconName`). **Trap:** gpui-component *main branch* README uses `gpui_platform` (git-only, not on crates.io) — crib only from the **`v0.5.1` tag** examples and its `crates/story` sources.
- Bootstrap: `Application::new().with_assets(gpui_component_assets::Assets)` → `app.run` → `gpui_component::init(cx)` → `cx.open_window(...)` where the window root is `gpui_component::Root::new(view, window, cx)` (Root hosts dialog/sheet/notification layers). Theme via `cx.theme()`.
- Components (verified in 0.5.1): `sidebar::{Sidebar, SidebarGroup, SidebarMenu, SidebarMenuItem}`, `input::{Input, InputState, InputEvent, NumberInput}`, `select::{Select, SelectState, SearchableVec}`, `switch`, `checkbox`, `slider`, `tab`, `form::{v_form, field}`, `dialog` via `window.open_dialog(...)`, toasts via `window.push_notification(...)`, `badge`, `skeleton`, `v_virtual_list()` + `VirtualListScrollHandle`, `scroll::Scrollbar`. Inputs/selects are state entities (`Entity<InputState>`) with `cx.subscribe_in` — keep `Vec<Subscription>` on views.
- Async: `cx.background_spawn(...)` (thread pool) for I/O + `cx.spawn(async move |this, cx| { ...; this.update(cx, ...) })` to apply results. Use blocking `ureq` inside `background_spawn`; **no tokio** (gpui is smol-based).
- Images: render `gpui::img(PathBuf)` from our own disk cache; do **not** rely on `img(url)` (no published HttpClient impl for gpui 0.2).
- OptiScaler: latest release `v0.9.4`, single `.7z` asset (~55 MB) on GitHub `optiscaler/OptiScaler`. Proxy DLL names from its own setup script: `dxgi.dll` (default), `winmm.dll`, `d3d12.dll`, `dbghelp.dll`, `version.dll`, `wininet.dll`, `winhttp.dll`. Fakenvapi + Nukem dlssg bundled since 0.9.
- `OptiScaler.ini`: 1546 lines, 37 sections, ~305 keys; `;` comment blocks document allowed values and "Default (auto) is X"; value types are `auto|true|false`, numbers, enum tokens, paths. No Rust ini crate round-trips comments → write a line-preserving editor.

## Workspace layout

```
Cargo.toml                        # [workspace] members = ["crates/core", "crates/app"]
crates/core/                      # opti-core: pure logic, no gpui, unit-testable
  src/
    lib.rs
    model.rs                      # Game { id, title, store, install_dir, exe_dir_override, steam_app_id, ... }
    detect/{mod,steam,epic,xbox}.rs
    exe_heuristics.rs
    artwork/{mod,steam_cdn,steamgriddb,placeholder}.rs
    optiscaler/{github,archive,install,ini,ini_schema}.rs
    paths.rs                      # directories crate: config/cache dirs
    settings.rs                   # JSON settings (steamgriddb key, overrides, manual games)
crates/app/                       # optiscaler-manager: GPUI binary
  src/
    main.rs                       # bootstrap per verified pattern
    app_state.rs                  # Entity<AppState>: games, artwork paths, install statuses, task queue
    assets.rs                     # rust_embed AssetSource for app assets
    views/{shell,game_grid,game_card,game_detail,config_editor,settings_view}.rs
```

Core deps: `steamlocate = "2.1"`, `serde`/`serde_json`, `quick-xml = "0.42"`, `sevenz-rust2 = "0.22"`, `ureq = "3"` (json), `directories = "6"`, `image`, `sha2`, `anyhow`, `thiserror`.

State pattern: one `Entity<AppState>` cloned into views; views `cx.observe` it. Navigation enum `Route { Library, GameDetail(id), ConfigEditor(id), Settings }` on the shell view, driven by sidebar.

## Detection

- **Steam:** `steamlocate` (`SteamDir::locate()`, libraries → apps: appid, name, installdir). Extra probe roots via `SteamDir::from_dir`: Linux `~/.local/share/Steam`, `~/.steam/steam`, flatpak `~/.var/app/com.valvesoftware.Steam/...`; macOS (dev) `~/Library/Application Support/Steam`. Denylist redists/runtimes (e.g. appid 228980, Proton).
- **Epic:** Windows `C:\ProgramData\Epic\EpicGamesLauncher\Data\Manifests\*.item` (JSON: DisplayName, InstallLocation, LaunchExecutable, AppName); skip `bIsIncompleteInstall` and DLC. Linux: Legendary `~/.config/legendary/installed.json` + Heroic (incl. flatpak) variants.
- **Xbox:** enumerate drive roots → `\.GamingRoot` (UTF-16LE, magic `RGBX`, folder name — typically `XboxGames`) → `<root>\<Game>\Content`; display name from `Content\MicrosoftGame.config` falling back to `appxmanifest.xml` DisplayName, then folder name. Exe dir = `Content`.
- **Exe-dir heuristic** (OptiScaler placement), user-overridable: single top-level exe → install_dir; Unreal `**/Binaries/Win64/*-Win64-Shipping.exe` → that dir; store-provided exe's parent; else largest exe (depth ≤3) excluding crash handlers/redists/anticheat.

## Artwork pipeline

Cache at `cache_dir()/artwork/<game_key>.jpg`; render `img(PathBuf)`.
1. Cache hit → done.
2. Steam: `cdn.cloudflare.steamstatic.com/steam/apps/{appid}/library_600x900.jpg`, fallback `header.jpg`.
3. SteamGridDB (if key set): `search/autocomplete/{title}` with Bearer key → `grids/game/{id}?dimensions=600x900` → first URL.
4. Placeholder: deterministic two-color gradient PNG from title hash; title text drawn by the card UI (no font rasterization in core).
Fetches via `ureq` in `background_spawn`, max ~4 concurrent, dedupe in-flight; downscale to ~300×450 at cache-write time.

## OptiScaler management

- **Releases:** GitHub API `releases/latest` (+ list for pinning), User-Agent required; pick the `.7z` asset; cache at `cache_dir()/releases/<tag>.7z`.
- **Extract** with sevenz-rust2 to a staging dir; **verify** the extracted set contains `OptiScaler.dll` + `OptiScaler.ini` before touching the game dir; codec failure → "download manually" fallback message.
- **Install:** copy payload into exe_dir, writing `OptiScaler.dll` as the chosen proxy name (default `dxgi.dll`). Pre-check: proxy dll already present and not ours → conflict warning dialog (ReShade etc.). First-install anti-cheat warning dialog ("I understand" checkbox).
- **Install manifest** `optiscaler-manager.json` in exe_dir + copy in app data keyed by game: `{ manager_version, release_tag, proxy_name, installed_files: [path + sha256], installed_at }`. Uninstall deletes exactly `installed_files` (skip hash-changed files; ask about keeping `OptiScaler.ini`). Update = uninstall preserving ini + install new tag. Proxy dll + `OptiScaler.ini` present without a manifest → label "manual install".
- **INI editor:** line-preserving parser (`Vec<Line>`: Blank/Comment/Section/KeyValue; edits patch value spans only; serialization reproduces every other byte — test: round-trip the pristine 1546-line file byte-identically). Schema layer `ini_schema.rs` generated once from the ini: `section.key → { BoolAuto | Enum([...]) | Number{min,max,float} | Text, help }`; unknown keys fall back to text field with the raw preceding comment as help (forward-compatible "full" editor). UI: section navigation + search; BoolAuto → 3-state Select, enums → Select, numbers → NumberInput, save → rewrite + toast. Always re-read the ini from disk on open (OptiScaler rewrites it in-game).

## Build order

1. **Workspace + hello window:** bootstrap, Root, sidebar shell with hardcoded routes — proves the pinned crates.io combo builds on macOS.
2. **Core models + Steam detection** with fixture-based unit tests (`.vdf`/`.acf`); wire into AppState, plain list first.
3. **Game grid + artwork:** placeholder generator → disk cache + Steam CDN → SteamGridDB behind settings key. Virtualized rows (`v_virtual_list` of row items, N cards per row from window width).
4. **Game detail + install path:** github client, 7z extraction (integration test against a real downloaded release), install/uninstall with manifest, proxy-name select, exe-dir heuristic + override. Testable on macOS against a fake-game fixture dir.
5. **INI editor:** parser + schema + form UI.
6. **Epic + Xbox detection**, Settings view, manual game add.
7. **Windows/Linux hardening:** real-machine builds (MSVC + Win10 SDK; Linux needs libxcb/libxkbcommon/Vulkan), elevation/ACL handling, app icon, packaging.

## Risks / gotchas

- gpui-component main-branch API drift → pin `=0.5.1`/`=0.2.2`, read tag examples only; docs.rs coverage is partial (~53%) — some builder names come from the `story` crate source.
- Xbox `Content` dirs are ACL-restricted; Program Files installs may need elevation → detect `PermissionDenied`, explain in a dialog (no auto-elevation in v1).
- Steam CDN 404s (delisted) → pipeline falls through to SteamGridDB/placeholder.
- Memory: rely on virtualization + downscaled cached art.

## Verification

- `cargo test -p opti-core`: vdf/acf/item/ini fixtures, ini byte-identical round-trip, manifest install/uninstall on temp dirs, real-release 7z extraction (network integration test, `#[ignore]` by default).
- Run app on macOS (`cargo run -p optiscaler-manager`): Steam library detected on the dev machine, banners load, fake-game fixture accepts a full install/uninstall/update cycle, config editor round-trips.
- Windows validation later on a real machine (out of scope for this session unless a Windows box is available).
