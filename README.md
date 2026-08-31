# OptiScaler Manager

A desktop app that finds your installed games, catalogs them with cover art, and
manages [OptiScaler](https://github.com/optiscaler/OptiScaler) in each of them —
install, update, remove, and edit the whole `OptiScaler.ini` without opening a
text editor.

Built with [GPUI](https://crates.io/crates/gpui) and
[gpui-component](https://github.com/longbridge/gpui-component). Windows is the
primary target; Linux is supported and macOS runs for development.

## What it does

- **Finds games** from Steam, the Epic Games Launcher and the Xbox app.
- **Shows cover art** from Steam's CDN, or SteamGridDB for non-Steam games once
  you add a free API key in Settings. Anything unmatched gets a generated
  placeholder, and every cover is cached on disk.
- **Flags anti-cheat** before you install. Games shipping Easy Anti-Cheat,
  BattlEye, PunkBuster and a dozen others are tagged in the catalog and carry a
  warning on their page, because OptiScaler loads itself into the game the same
  way a cheat would and can get an account banned. A clean scan is reported as
  "nothing found", never as "safe" — server-side systems like VAC leave nothing
  on disk to detect.
- **Installs any published OptiScaler version.** Pick a release from the
  dropdown, read its changelog in the app, and install it: the `.7z` is
  downloaded once, extracted, and copied into the game with `OptiScaler.dll`
  renamed to the proxy DLL the game loads (`dxgi.dll` by default).
- **Tracks what it installed.** Every install writes a manifest listing each
  file and its hash, so uninstall removes exactly what was added and leaves
  anything you edited in place. The catalog tags each game with the version it
  has, or marks a hand-made install as "Manual".
- **Leaves configuration to OptiScaler itself.** Once installed, press Insert
  in game to open OptiScaler's own settings overlay. Updates and reinstalls
  preserve the `OptiScaler.ini` the overlay writes.

## Download

Prebuilt binaries for Windows and Linux are attached to each
[release](https://github.com/diegopluna/optiscaler-manager-gpui/releases).
Unpack and run `optiscaler-manager`; there is nothing to install.

Linux builds are made on Ubuntu 22.04 and need a Vulkan driver plus the usual
X11/Wayland client libraries, which a desktop system will already have.

## Building it

```sh
cargo run -p optiscaler-manager
```

`OPTISCALER_STEAM_ROOT` points Steam detection at a specific install, which is
useful for unusual setups and for testing against a fixture library.

## Layout

| Crate | What's in it |
| --- | --- |
| `crates/core` (`opti-core`) | Detection, artwork, GitHub releases, install/uninstall, ini editing. No UI dependency, so it is testable without a display. |
| `crates/app` | The GPUI application: library grid, game detail, config editor, settings. |

## Tests

```sh
cargo test --workspace
```

Covers the VDF and manifest parsers, the exe-directory heuristics, anti-cheat
signature matching, changelog rendering, and install/uninstall against
temporary directories.

One test is network-bound and ignored by default. It downloads the current
OptiScaler release, extracts it and runs a full install/uninstall cycle — worth
running whenever OptiScaler publishes a new version:

```sh
cargo test -p opti-core --test real_release -- --ignored --nocapture
```

## Notes

- Do not use OptiScaler in online games with anti-cheat. The detection here
  covers what ships on disk; it cannot see server-side systems such as VAC.
- Anti-cheat signatures come from SteamDB's
  [FileDetectionRuleSets](https://github.com/SteamDatabase/FileDetectionRuleSets).
- Installing into `Program Files` or an Xbox `Content` directory can need
  elevated permissions; the app reports the failure rather than escalating.
