use crate::{
    model::{REPOSITORY, VERSION},
    storage,
};
#[cfg(not(windows))]
use anyhow::bail;
use anyhow::{Context, Result, ensure};
use semver::Version;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

const MAX_ASSET: u64 = 256 * 1024 * 1024;

#[derive(Clone, Debug)]
pub struct Release {
    pub version: String,
    pub name: String,
    pub url: String,
    pub size: u64,
    pub sha256: String,
}

#[derive(Clone, Debug)]
pub struct StagedUpdate {
    pub path: PathBuf,
    pub sha256: String,
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

#[derive(Deserialize)]
struct ApiAsset {
    name: String,
    browser_download_url: String,
    size: u64,
    digest: Option<String>,
}
#[derive(Deserialize)]
struct ApiRelease {
    tag_name: String,
    draft: bool,
    prerelease: bool,
    assets: Vec<ApiAsset>,
}

fn https_agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .http_status_as_error(false)
        .https_only(true)
        .max_redirects(8)
        .timeout_resolve(Some(Duration::from_secs(10)))
        .timeout_connect(Some(Duration::from_secs(10)))
        .timeout_recv_response(Some(Duration::from_secs(15)))
        .timeout_per_read(Some(Duration::from_secs(15)))
        .timeout_per_write(Some(Duration::from_secs(10)))
        .user_agent(concat!("DownloadManager/", env!("CARGO_PKG_VERSION")))
        .build()
        .into()
}

pub fn check() -> Result<UpdateStatus> {
    let agent = https_agent();
    let mut response = agent
        .get(&format!(
            "https://api.github.com/repos/{REPOSITORY}/releases/latest"
        ))
        .header("Accept", "application/vnd.github+json")
        .call()
        .context("Update check failed; check connectivity/TLS")?;
    ensure!(
        response.status().as_u16() != 404,
        "Repository or stable release is not published/available yet"
    );
    ensure!(
        response.status().as_u16() == 200,
        "Update check returned HTTP {} (network/rate-limit errors do not mean up to date)",
        response.status()
    );
    let json = response
        .body_mut()
        .with_config()
        .limit(2 * 1024 * 1024)
        .read_to_vec()?;
    select_release(&json, &agent)
}

fn select_release(json: &[u8], agent: &ureq::Agent) -> Result<UpdateStatus> {
    let release: ApiRelease =
        serde_json::from_slice(json).context("Invalid GitHub release response")?;
    ensure!(
        !release.draft && !release.prerelease,
        "No compatible stable release is available"
    );
    let version = Version::parse(
        release
            .tag_name
            .strip_prefix('v')
            .unwrap_or(&release.tag_name),
    )?;
    ensure!(
        version.pre.is_empty(),
        "Prerelease versions are not supported on the stable channel"
    );
    if version <= Version::parse(VERSION)? {
        return Ok(UpdateStatus::Current);
    }
    ensure!(
        cfg!(all(windows, target_arch = "x86_64")),
        "Automatic installation currently supports Windows x64 portable builds only"
    );
    let name = format!("DownloadManager-v{version}.exe");
    let asset = release
        .assets
        .iter()
        .find(|a| a.name == name)
        .context("Release has no compatible portable executable")?;
    validate_asset_url(&asset.browser_download_url, &name)?;
    ensure!(
        asset.size > 0 && asset.size <= MAX_ASSET,
        "Update asset size is invalid or exceeds 256 MiB"
    );
    let hash = if let Some(digest) = asset
        .digest
        .as_deref()
        .and_then(|s| s.strip_prefix("sha256:"))
    {
        validate_hash(digest)?;
        digest.to_ascii_lowercase()
    } else {
        let checksums = release
            .assets
            .iter()
            .find(|a| a.name == "SHA256SUMS.txt")
            .context(
                "Release supplies neither SHA-256 digest nor SHA256SUMS.txt; installation refused",
            )?;
        validate_asset_url(&checksums.browser_download_url, &checksums.name)?;
        let mut response = agent.get(&checksums.browser_download_url).call()?;
        ensure!(
            response.status().as_u16() == 200,
            "Checksum download returned HTTP {}",
            response.status()
        );
        let text = response
            .body_mut()
            .with_config()
            .limit(64 * 1024)
            .read_to_string()?;
        checksum_for(&text, &name)?
    };
    Ok(UpdateStatus::Available(Release {
        version: version.to_string(),
        name,
        url: asset.browser_download_url.clone(),
        size: asset.size,
        sha256: hash,
    }))
}

fn validate_asset_url(value: &str, name: &str) -> Result<()> {
    let url = url::Url::parse(value)?;
    ensure!(
        url.scheme() == "https"
            && url.host_str() == Some("github.com")
            && url.username().is_empty()
            && url.password().is_none()
            && url.port().is_none()
            && url
                .path()
                .starts_with(&format!("/{REPOSITORY}/releases/download/"))
            && url.path_segments().and_then(|mut s| s.next_back()) == Some(name)
            && url.query().is_none()
            && url.fragment().is_none(),
        "Update asset URL is outside the canonical repository"
    );
    Ok(())
}

fn validate_hash(hash: &str) -> Result<()> {
    ensure!(
        hash.len() == 64 && hash.bytes().all(|b| b.is_ascii_hexdigit()),
        "Invalid SHA-256 digest"
    );
    Ok(())
}

pub fn checksum_for(text: &str, filename: &str) -> Result<String> {
    let mut found = None;
    for line in text.lines() {
        let fields: Vec<_> = line.split_whitespace().collect();
        if fields.len() == 2 && fields[1].trim_start_matches('*') == filename {
            ensure!(found.is_none(), "Duplicate checksum entry");
            validate_hash(fields[0])?;
            found = Some(fields[0].to_ascii_lowercase());
        }
    }
    found.context("No matching SHA-256 checksum in release")
}

pub fn download(
    release: &Release,
    directory: &Path,
    mut progress: impl FnMut(u64, u64),
) -> Result<StagedUpdate> {
    validate_asset_url(&release.url, &release.name)?;
    validate_hash(&release.sha256)?;
    ensure!(
        release.size > 0 && release.size <= MAX_ASSET,
        "Invalid update size"
    );
    let updates = directory.join("updates");
    fs::create_dir_all(&updates)?;
    let path = updates.join(&release.name);
    let temp = updates.join(format!("{}.partial", release.name));
    let result = (|| -> Result<()> {
        let mut response = https_agent()
            .get(&release.url)
            .header("Accept-Encoding", "identity")
            .call()?;
        ensure!(
            response.status().as_u16() == 200,
            "Update download returned HTTP {}",
            response.status()
        );
        let mut file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(&temp)?;
        let mut reader = response.body_mut().as_reader();
        let mut buffer = [0u8; 32 * 1024];
        let mut bytes = 0;
        let mut hash = Sha256::new();
        let mut last = Instant::now();
        loop {
            let count = reader.read(&mut buffer)?;
            if count == 0 {
                break;
            }
            bytes += count as u64;
            ensure!(bytes <= release.size, "Update exceeds published size");
            file.write_all(&buffer[..count])?;
            hash.update(&buffer[..count]);
            if last.elapsed() >= Duration::from_millis(250) {
                progress(bytes, release.size);
                last = Instant::now();
            }
        }
        ensure!(
            bytes == release.size,
            "Update is incomplete; current installation is unchanged"
        );
        ensure!(
            format!("{:x}", hash.finalize()) == release.sha256,
            "Update SHA-256 verification failed; current installation is unchanged"
        );
        file.sync_all()?;
        drop(file);
        verify_executable(&temp)?;
        storage::replace(&temp, &path)?;
        progress(bytes, release.size);
        Ok(())
    })();
    if let Err(error) = result {
        let _ = fs::remove_file(&temp);
        return Err(error);
    }
    Ok(StagedUpdate {
        path,
        sha256: release.sha256.clone(),
    })
}

pub fn verify_file(path: &Path, expected: &str) -> Result<()> {
    validate_hash(expected)?;
    let mut file = File::open(path)?;
    let mut hash = Sha256::new();
    let mut buffer = [0; 32 * 1024];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
    }
    ensure!(
        format!("{:x}", hash.finalize()) == expected.to_ascii_lowercase(),
        "Staged update checksum mismatch"
    );
    Ok(())
}

fn verify_executable(path: &Path) -> Result<()> {
    let mut file = File::open(path)?;
    let mut dos = [0; 64];
    file.read_exact(&mut dos)?;
    ensure!(&dos[..2] == b"MZ", "Update is not a Windows executable");
    let offset = u32::from_le_bytes(dos[60..64].try_into().unwrap()) as u64;
    ensure!(
        offset >= 64 && offset + 6 <= file.metadata()?.len(),
        "Invalid PE header"
    );
    file.seek(SeekFrom::Start(offset))?;
    let mut pe = [0; 6];
    file.read_exact(&mut pe)?;
    ensure!(
        &pe[..4] == b"PE\0\0" && u16::from_le_bytes([pe[4], pe[5]]) == 0x8664,
        "Update is not a Windows x64 executable"
    );
    Ok(())
}

pub fn launch_installer(staged: &StagedUpdate) -> Result<()> {
    #[cfg(windows)]
    {
        use std::{os::windows::process::CommandExt, process::Command};
        verify_file(&staged.path, &staged.sha256)?;
        verify_executable(&staged.path)?;
        let target = std::env::current_exe()?.canonicalize()?;
        let helper = staged
            .path
            .with_file_name("DownloadManager-update-helper.exe");
        fs::copy(&staged.path, &helper)
            .context("Cannot prepare update helper; check data folder permissions")?;
        Command::new(helper)
            .arg("--apply-update")
            .arg(&target)
            .arg(&staged.path)
            .arg(&staged.sha256)
            .arg(std::process::id().to_string())
            .creation_flags(0x08000000)
            .spawn()
            .context("Cannot launch update helper")?;
        Ok(())
    }
    #[cfg(not(windows))]
    {
        let _ = staged;
        bail!("In-app installation currently supports Windows only")
    }
}

/// Called only by the separately spawned, verified update helper after a user action.
pub fn apply_update(target: &Path, staged: &Path, hash: &str, parent_pid: u32) -> Result<()> {
    #[cfg(windows)]
    {
        use windows_sys::Win32::{
            Foundation::{CloseHandle, WAIT_OBJECT_0},
            System::Threading::{OpenProcess, PROCESS_SYNCHRONIZE, WaitForSingleObject},
        };
        ensure!(
            parent_pid != 0 && parent_pid != std::process::id(),
            "Invalid parent process"
        );
        ensure!(
            target.is_absolute()
                && staged.is_absolute()
                && target != staged
                && target
                    .extension()
                    .is_some_and(|s| s.eq_ignore_ascii_case("exe")),
            "Invalid installation paths"
        );
        verify_file(staged, hash)?;
        verify_executable(staged)?;
        // Wait on the actual process handle; a reused PID cannot cause an unbounded wait.
        let handle = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, parent_pid) };
        if handle.is_null() {
            ensure!(
                std::io::Error::last_os_error().raw_os_error() == Some(87),
                "Cannot wait for running application; installation aborted"
            );
            // ERROR_INVALID_PARAMETER means the parent already exited before OpenProcess.
        } else {
            let result = unsafe { WaitForSingleObject(handle, 60_000) };
            unsafe {
                CloseHandle(handle);
            }
            ensure!(
                result == WAIT_OBJECT_0,
                "Application did not exit in time; installation aborted"
            );
        }
        let next = target.with_extension("update-next.exe");
        let backup = target.with_extension("update-backup.exe");
        ensure!(
            !next.exists(),
            "A previous update staging file exists; installation aborted"
        );
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&next)?;
        std::io::copy(&mut File::open(staged)?, &mut file)?;
        file.sync_all()?;
        drop(file);
        verify_file(&next, hash)?;
        fs::copy(target, &backup)
            .context("Cannot back up current executable; installation aborted")?;
        storage::replace(&next, target).context("Cannot replace executable; backup preserved")?;
        std::process::Command::new(target)
            .spawn()
            .context("Update installed; relaunch manually (settings and queue preserved)")?;
        Ok(())
    }
    #[cfg(not(windows))]
    {
        let _ = (target, staged, hash, parent_pid);
        bail!("Windows updater only")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn checksums_match_exact_asset_and_reject_duplicates() {
        let digest = "a".repeat(64);
        assert_eq!(
            checksum_for(
                &format!("{digest}  DownloadManager-v1.0.0.exe\n"),
                "DownloadManager-v1.0.0.exe"
            )
            .unwrap(),
            digest
        );
        assert!(checksum_for(&format!("{digest}  wrong.exe"), "app.exe").is_err());
        assert!(checksum_for("1234 app.exe", "app.exe").is_err());
        assert!(checksum_for(&format!("{digest} app.exe\n{digest} app.exe"), "app.exe").is_err());
    }
    #[test]
    fn asset_urls_are_confined_to_canonical_releases() {
        let name = "DownloadManager-v1.0.0.exe";
        assert!(
            validate_asset_url(
                &format!("https://github.com/{REPOSITORY}/releases/download/v1.0.0/{name}"),
                name
            )
            .is_ok()
        );
        assert!(
            validate_asset_url(
                &format!("https://github.com/other/repo/releases/download/v1.0.0/{name}"),
                name
            )
            .is_err()
        );
        assert!(
            validate_asset_url(
                &format!("http://github.com/{REPOSITORY}/releases/download/v1.0.0/{name}"),
                name
            )
            .is_err()
        );
    }
    #[test]
    fn version_order_and_stable_filtering_use_semver() {
        let agent = https_agent();
        assert!(matches!(
            select_release(
                br#"{"tag_name":"v0.0.9","draft":false,"prerelease":false,"assets":[]}"#,
                &agent
            )
            .unwrap(),
            UpdateStatus::Current
        ));
        assert!(
            select_release(
                br#"{"tag_name":"v1.0.0-beta.1","draft":false,"prerelease":true,"assets":[]}"#,
                &agent
            )
            .is_err()
        );
        assert!(
            select_release(
                br#"{"tag_name":"v1.0.0","draft":false,"prerelease":false,"assets":[]}"#,
                &agent
            )
            .is_err()
        );
        assert!(Version::parse("1.10.0").unwrap() > Version::parse("1.9.0").unwrap());
    }
}
