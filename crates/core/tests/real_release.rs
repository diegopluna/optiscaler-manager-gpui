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
