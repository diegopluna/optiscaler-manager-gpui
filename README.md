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
- **Installs OptiScaler** from its latest GitHub release: downloads the `.7z`
  once, extracts it, and copies it into the game with `OptiScaler.dll` renamed
  to the proxy DLL the game loads (`dxgi.dll` by default).
- **Tracks what it installed.** Every install writes a manifest listing each
  file and its hash, so uninstall removes exactly what was added and leaves
  anything you edited in place.
- **Edits the config.** The full `OptiScaler.ini` — every section and key — with
  the mod's own documentation shown inline and the right control per key.
  Saving preserves all ~1500 lines and comments; only changed values are
  rewritten.

## Running it

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

Covers the VDF and manifest parsers, the exe-directory heuristics, install and
uninstall against temporary directories, and a byte-identical round trip of the
real shipped `OptiScaler.ini`.

One test is network-bound and ignored by default. It downloads the current
OptiScaler release, extracts it and runs a full install/uninstall cycle — worth
running whenever OptiScaler publishes a new version:

```sh
cargo test -p opti-core --test real_release -- --ignored --nocapture
```

## Notes

- Do not use OptiScaler in online games with anti-cheat.
- Installing into `Program Files` or an Xbox `Content` directory can need
  elevated permissions; the app reports the failure rather than escalating.
