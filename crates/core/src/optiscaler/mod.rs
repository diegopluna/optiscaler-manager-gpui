pub mod archive;
pub mod github;
pub mod ini_edit;
pub mod install;
pub mod optipatcher;

use std::path::PathBuf;

use anyhow::Result;

pub use github::{Release, latest_release, list_releases};

/// Ensures a release is downloaded and extracted, returning the payload
/// directory ready to install from.
///
/// The extracted payload is cached per release tag, so installing into several
/// games downloads and unpacks only once. `progress` receives download
/// progress as `(bytes_so_far, total_bytes)`.
pub fn prepare_payload(release: &Release, progress: impl Fn(u64, u64)) -> Result<PathBuf> {
    let staging = crate::paths::cache_dir()?
        .join("staging")
        .join(&release.tag);

    // Reuse a previous extraction, but only if it still validates.
    if let Ok(payload) = archive::payload_root(&staging) {
        log::info!("reusing extracted payload for {}", release.tag);
        return Ok(payload);
    }

    let archive_path = github::download(release, progress)?;
    archive::extract(&archive_path, &staging)
}
pub use install::{
    DEFAULT_PROXY_DLL, InstallManifest, InstallStatus, PROXY_DLL_NAMES, install, status, uninstall,
};
