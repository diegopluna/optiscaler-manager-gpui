//! End-to-end check against the real OptiScaler release on GitHub: query the
//! API, download the archive, extract it and install it into a scratch game
//! directory.
//!
//! Ignored by default because it hits the network and downloads ~55 MB. Run
//! with `cargo test -p opti-core --test real_release -- --ignored --nocapture`,
//! and before offering a newly published OptiScaler version to users.

use opti_core::optiscaler::{archive, github, install};

#[test]
#[ignore = "network: downloads the real OptiScaler release"]
fn downloads_extracts_and_installs_the_latest_release() {
    let release = github::latest_release().expect("querying the latest release");
    println!(
        "latest: {} ({}, {} bytes)",
        release.tag, release.asset_name, release.size
    );
    assert!(release.asset_name.ends_with(".7z"));
    assert!(!release.download_url.is_empty());

    let archive_path = github::download(&release, |done, total| {
        if total > 0 && done % (8 * 1024 * 1024) < 65_536 {
            println!("  {} / {} bytes", done, total);
        }
    })
    .expect("downloading the release archive");
    assert!(archive_path.is_file());

    let staging = tempfile::tempdir().unwrap();
    let payload = archive::extract(&archive_path, &staging.path().join("payload"))
        .expect("extracting and validating the payload");

    println!("payload root: {}", payload.display());
    assert!(payload.join(archive::PAYLOAD_DLL).is_file());
    assert!(payload.join(archive::PAYLOAD_INI).is_file());

    // Install into a scratch "game" directory and take it back out again.
    let game = tempfile::tempdir().unwrap();
    let manifest = install::install(&payload, game.path(), "dxgi.dll", &release.tag)
        .expect("installing into the game directory");

    println!("installed {} files", manifest.files.len());
    assert!(game.path().join("dxgi.dll").is_file(), "proxy dll written");
    // The archive's manual-setup helpers must not reach the game folder; a
    // finished install that still shows setup scripts looks aborted.
    for entry in std::fs::read_dir(game.path()).unwrap().flatten() {
        let name = entry.file_name().to_string_lossy().to_lowercase();
        assert!(
            !name.ends_with(".bat") && !name.ends_with(".sh") && !name.starts_with("!!"),
            "setup helper leaked into the game folder: {name}"
        );
    }
    assert!(!game.path().join(archive::PAYLOAD_DLL).exists());
    assert_eq!(
        install::status(game.path()).version(),
        Some(release.tag.as_str())
    );

    // The AMD/Intel opt-out edits the real shipped ini: exactly one key in
    // [Spoofing] flips, and its prefix-sharing siblings stay untouched.
    opti_core::optiscaler::ini_edit::set_dxgi_spoofing(game.path(), false)
        .expect("disabling spoofing in the shipped ini");
    let ini = std::fs::read_to_string(game.path().join(archive::PAYLOAD_INI)).unwrap();
    assert!(ini.contains("Dxgi=false"));
    assert!(
        ini.contains("DxgiFactoryWrapping=auto"),
        "sibling keys untouched"
    );
    opti_core::optiscaler::ini_edit::set_dxgi_spoofing(game.path(), true)
        .expect("re-enabling spoofing");

    let report = install::uninstall(game.path(), false).expect("uninstalling");
    assert!(
        report.kept_modified.is_empty(),
        "nothing should look modified right after install: {:?}",
        report.kept_modified
    );
    assert!(!game.path().join("dxgi.dll").exists(), "proxy dll removed");
    assert_eq!(
        install::status(game.path()),
        install::InstallStatus::NotInstalled
    );
}

#[test]
#[ignore = "network: fetches the live OptiPatcher compatibility source"]
fn parses_the_live_optipatcher_compatibility_list() {
    let supported = opti_core::optiscaler::optipatcher::fetch_supported_exes()
        .expect("fetching and parsing the OptiPatcher source");
    println!("{} supported executables", supported.len());

    // Long-standing entries that should always be present; if these vanish
    // the source format probably changed and the parser needs revisiting.
    for expected in ["hogwartslegacy.exe", "stalker2-win64-shipping.exe"] {
        assert!(
            supported.iter().any(|s| s == expected),
            "missing {expected}"
        );
    }
    assert!(
        supported.len() > 50,
        "suspiciously few entries: {}",
        supported.len()
    );
}

#[test]
#[ignore = "network: downloads the real OptiPatcher plugin"]
fn installs_optipatcher_into_a_managed_install() {
    use opti_core::optiscaler::optipatcher;

    let release = github::latest_release().unwrap();
    let payload = opti_core::optiscaler::prepare_payload(&release, |_, _| {}).unwrap();

    // A fake Stalker 2: its shipping exe is on OptiPatcher's supported list.
    let game = tempfile::tempdir().unwrap();
    std::fs::write(game.path().join("Stalker2-Win64-Shipping.exe"), b"game").unwrap();
    install::install(&payload, game.path(), "dxgi.dll", &release.tag).unwrap();

    let supported = optipatcher::fetch_supported_exes().unwrap();
    assert_eq!(
        optipatcher::matching_exe(game.path(), &supported).as_deref(),
        Some("stalker2-win64-shipping.exe")
    );

    let written = optipatcher::install(game.path()).unwrap();
    install::record_extra_files(game.path(), &written).unwrap();

    assert!(game.path().join("plugins/OptiPatcher.asi").is_file());
    let ini = std::fs::read_to_string(game.path().join("OptiScaler.ini")).unwrap();
    assert!(
        ini.contains("LoadAsiPlugins=true"),
        "ASI loading enabled in the ini"
    );
    assert!(optipatcher::is_installed(game.path()));

    // Uninstall takes the plugin out with everything else.
    let report = install::uninstall(game.path(), false).unwrap();
    assert!(
        report
            .removed
            .iter()
            .any(|f| f == "plugins/OptiPatcher.asi")
    );
    assert!(!game.path().join("plugins").exists());
}

#[test]
#[ignore = "network: queries this app's own releases"]
fn update_check_reaches_github() {
    let result = opti_core::update::check().expect("update check should parse");
    println!(
        "current {} -> {result:?}",
        opti_core::update::CURRENT_VERSION
    );
}
