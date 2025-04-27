use crate::model::{State, Status};
use anyhow::{Context, Result, bail};
use serde::{Serialize, de::DeserializeOwned};
#[cfg(not(windows))]
use std::fs;
use std::{
    fs::{File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};

pub fn data_directory() -> Result<PathBuf> {
    if let Some(path) = std::env::var_os("DOWNLOAD_MANAGER_DATA_DIR") {
        let path = PathBuf::from(path);
        if !path.is_absolute() {
            bail!("DOWNLOAD_MANAGER_DATA_DIR must be absolute");
        }
        return Ok(path);
    }
    let base = std::env::var_os("LOCALAPPDATA")
        .or_else(|| std::env::var_os("XDG_DATA_HOME"))
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|p| PathBuf::from(p).join(".local/share")))
        .context("Cannot locate application data directory")?;
    Ok(base.join("DownloadManager"))
}

pub fn read_json<T: DeserializeOwned>(path: &Path, max: u64) -> Result<T> {
    let file = File::open(path).with_context(|| format!("Cannot open {}", path.display()))?;
    if file.metadata()?.len() > max {
        bail!("State file is too large: {}", path.display());
    }
    let mut bytes = Vec::new();
    file.take(max + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > max {
        bail!("State file exceeded the size limit");
    }
    serde_json::from_slice(&bytes).context("Invalid JSON; original file has been preserved")
}

pub fn atomic_json<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(value)?;
    let temp = path.with_extension("json.tmp");
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(&temp)?;
    file.write_all(&bytes)?;
    file.sync_all()?;
    drop(file);
    replace(&temp, path).with_context(|| format!("Cannot save {}", path.display()))
}

#[cfg(windows)]
fn windows_move(source: &Path, destination: &Path, flags: u32) -> Result<()> {
    use std::os::windows::ffi::OsStrExt;
    let source: Vec<u16> = source.as_os_str().encode_wide().chain(Some(0)).collect();
    let destination: Vec<u16> = destination
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect();
    // Both paths remain alive and null-terminated during this synchronous Win32 call.
    let ok = unsafe {
        windows_sys::Win32::Storage::FileSystem::MoveFileExW(
            source.as_ptr(),
            destination.as_ptr(),
            flags,
        )
    };
    if ok == 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(())
}

pub fn replace(source: &Path, destination: &Path) -> Result<()> {
    #[cfg(windows)]
    {
        use windows_sys::Win32::Storage::FileSystem::*;
        windows_move(
            source,
            destination,
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    }
    #[cfg(not(windows))]
    {
        fs::rename(source, destination)?;
        Ok(())
    }
}

pub fn finalize(source: &Path, destination: &Path) -> Result<()> {
    #[cfg(windows)]
    {
        windows_move(
            source,
            destination,
            windows_sys::Win32::Storage::FileSystem::MOVEFILE_WRITE_THROUGH,
        )
    }
    #[cfg(not(windows))]
    {
        fs::hard_link(source, destination)?;
        fs::remove_file(source)?;
        Ok(())
    }
}

pub fn load_state(directory: &Path) -> Result<State> {
    let path = directory.join("state.json");
    let mut state: State = if path.exists() {
        read_json(&path, 16 * 1024 * 1024)?
    } else {
        State::default()
    };
    if state.schema != 1 {
        bail!(
            "Unsupported state schema {}; original file has been preserved",
            state.schema
        );
    }
    if state.next_id == 0 {
        bail!("Invalid queue sequence; original state file has been preserved");
    }
    if state.downloads.len() > 10_000 {
        bail!("Queue history exceeds the supported 10,000 entries");
    }
    let mut ids = std::collections::HashSet::new();
    for job in &state.downloads {
        if job.id == 0
            || !ids.insert(job.id)
            || job.id >= state.next_id
            || !job.destination.is_absolute()
        {
            bail!("Invalid queue identity/path; original state file has been preserved");
        }
        crate::model::validate_url(&job.url).context("Invalid URL in stored queue")?;
        crate::model::validate_filename(
            job.destination
                .file_name()
                .and_then(|n| n.to_str())
                .context("Invalid destination filename")?,
        )?;
    }
    state.settings.concurrency = state.settings.concurrency.clamp(1, 4);
    state.settings.bandwidth_kib = state.settings.bandwidth_kib.min(1_048_576);
    for job in &mut state.downloads {
        // Restart never initiates network transfers without a fresh user action.
        if matches!(
            job.status,
            Status::Downloading | Status::Pausing | Status::Queued
        ) {
            job.status = Status::Paused;
        }
    }
    Ok(state)
}
