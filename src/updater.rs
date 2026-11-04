//! DownloadManager product adapter. Release validation and installation live in rust-auto-updator.
use crate::model::VERSION;
use anyhow::{Context, Result};
use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

pub fn config() -> Result<rust_auto_updator::Config> {
    #[cfg(target_os = "linux")]
    let mut config = rust_auto_updator::Config::debian(
        "siarheisinelnikau",
        "download-manager",
        VERSION,
        "download-manager",
    )
    .map_err(anyhow::Error::msg)?;
    #[cfg(not(target_os = "linux"))]
    let mut config = rust_auto_updator::Config::new(
        "siarheisinelnikau",
        "download-manager",
        VERSION,
        "DownloadManager",
    )
    .map_err(anyhow::Error::msg)?;

    #[cfg(target_os = "linux")]
    {
        config.product_name = "DownloadManager".into();
        config.asset_platform = Some("ubuntu".into());
        config.install.helper_name = "download-manager-update-helper".into();
    }
    #[cfg(not(target_os = "linux"))]
    {
        config.asset_platform = Some("windows".into());
        config.expected_subsystem = Some(2);
        config.install.helper_name = "DownloadManager-update-helper.exe".into();
    }
    config.allow_asset_digest = true;
    config.max_executable_size = 256 * 1024 * 1024;
    config.install.stage_prefix = ".DownloadManager-update-".into();
    config.validate().map_err(anyhow::Error::msg)?;
    Ok(config)
}

#[derive(Clone, Debug)]
pub struct Release {
    pub version: String,
    pub size: u64,
    source: Box<rust_auto_updator::Release>,
}

/// Snapshot clones retain one verified payload until the queue has saved and stopped.
#[derive(Clone)]
pub struct StagedUpdate(Arc<Mutex<Option<rust_auto_updator::Prepared>>>);
impl std::fmt::Debug for StagedUpdate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("StagedUpdate(verified payload)")
    }
}

#[derive(Clone, Debug, Default)]
pub enum UpdateStatus {
    #[default]
    Idle,
    Checking,
    Current,
    Available(Release),
    Downloading {
        bytes: u64,
        total: u64,
    },
    Staged(StagedUpdate),
    Error(String),
}

pub fn check() -> Result<UpdateStatus> {
    Ok(
        match rust_auto_updator::check_latest(&config()?).map_err(anyhow::Error::msg)? {
            Some(source) => UpdateStatus::Available(Release {
                version: source.version.to_string(),
                size: source.size(),
                source: Box::new(source),
            }),
            None => UpdateStatus::Current,
        },
    )
}

pub fn download(
    release: &Release,
    _directory: &Path,
    progress: impl FnMut(u64, u64),
) -> Result<StagedUpdate> {
    let prepared = rust_auto_updator::prepare_download(&release.source, &config()?, progress)
        .map_err(anyhow::Error::msg)?;
    Ok(StagedUpdate(Arc::new(Mutex::new(Some(prepared)))))
}

pub fn launch_installer(staged: &StagedUpdate) -> Result<()> {
    let mut guard = staged
        .0
        .lock()
        .map_err(|_| anyhow::anyhow!("Update state is unavailable"))?;
    let prepared = guard.as_mut().context("Update was already handed off")?;
    rust_auto_updator::handoff(prepared).map_err(anyhow::Error::msg)?;
    *guard = None;
    Ok(())
}

pub fn apply_update() -> Result<()> {
    rust_auto_updator::apply_update(&config()?).map_err(anyhow::Error::msg)
}

pub fn cleanup_pending() {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() == 2
        && args[0] == "--cleanup-update"
        && let Ok(config) = config()
    {
        rust_auto_updator::cleanup_after_update(PathBuf::from(&args[1]), config.install);
    }
}

/// Compatibility for releases that invoked the newly downloaded EXE with positional paths.
pub fn apply_legacy(target: &Path, staged: &Path, hash: &str, parent_pid: u32) -> Result<()> {
    let updates = crate::storage::data_directory()?
        .join("updates")
        .canonicalize()?;
    let helper = std::env::current_exe()?.canonicalize()?;
    let staged = staged.canonicalize()?;
    anyhow::ensure!(
        staged.parent() == Some(updates.as_path())
            && staged.file_name().is_some_and(
                |name| name == std::ffi::OsStr::new(&format!("DownloadManager-v{VERSION}.exe"))
            )
            && helper.parent() == Some(updates.as_path())
            && helper
                .file_name()
                .is_some_and(|name| name == "DownloadManager-update-helper.exe"),
        "Unexpected legacy update staging location"
    );
    rust_auto_updator::apply_legacy_update(&config()?, target, &staged, hash, parent_pid, None)
        .map_err(anyhow::Error::msg)
}

/// Report recognized modern/legacy helper failures where the coordinator already reads them.
pub fn report_helper_failure(error: &anyhow::Error) {
    let Ok(config) = config() else {
        return;
    };
    let Ok(helper) = std::env::current_exe() else {
        return;
    };
    let Some(stage) = helper.parent() else {
        return;
    };
    let Ok(directory) = crate::storage::data_directory() else {
        return;
    };
    let updates = directory.join("updates");
    let modern = stage.file_name().is_some_and(|name| {
        name.to_string_lossy()
            .starts_with(&config.install.stage_prefix)
    });
    let legacy = updates
        .canonicalize()
        .ok()
        .zip(stage.canonicalize().ok())
        .is_some_and(|(expected, actual)| expected == actual);
    if helper
        .file_name()
        .is_none_or(|name| name != std::ffi::OsStr::new(&config.install.helper_name))
        || !(modern || legacy)
    {
        return;
    }
    let report = updates.join("last-update-error.txt");
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if [stage, updates.as_path(), report.as_path()]
            .iter()
            .any(|path| {
                std::fs::symlink_metadata(path)
                    .is_ok_and(|meta| meta.file_attributes() & 0x400 != 0)
            })
        {
            return;
        }
    }
    let message: String = format!("Update installation failed: {error:#}. The working executable or previous.exe backup is preserved when available. Queue, settings and downloads were not removed.").chars().take(8192).collect();
    if std::fs::create_dir_all(&updates).is_ok() {
        let _ = std::fs::write(report, message);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn product_adapter_accepts_canonical_digest_assets_and_rejects_other_products() {
        let config = config().unwrap();
        let platform = config.asset_platform.as_deref().unwrap();
        let extension = if cfg!(target_os = "linux") {
            "deb"
        } else {
            "exe"
        };
        let arch = config.architecture.debian_name();
        let name = format!("DownloadManager-v99.0.0-{platform}-{arch}.{extension}");
        let url = config.repository_url();
        let mut release = serde_json::json!({
            "tag_name":"v99.0.0", "html_url":format!("{url}/releases/tag/v99.0.0"),
            "draft":false, "prerelease":false, "body":"Fixture release",
            "assets":[{"name":name,"size":2048,
                "browser_download_url":format!("{url}/releases/download/v99.0.0/{name}"),
                "digest":format!("sha256:{}", "ab".repeat(32))}]
        });
        assert!(
            rust_auto_updator::parse_release(&release.to_string(), &config)
                .unwrap()
                .is_some()
        );
        release["assets"][0]["name"] =
            format!("OtherProduct-v99.0.0-{platform}-{arch}.{extension}").into();
        assert!(rust_auto_updator::parse_release(&release.to_string(), &config).is_err());
    }
}
