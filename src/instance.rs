//! Per-data-directory activation without a network listener or UI polling.
#[cfg(windows)]
mod windows {
    use anyhow::{Context, Result, ensure};
    use sha2::{Digest, Sha256};
    use std::{path::Path, sync::Arc, thread};
    use windows_sys::Win32::{
        Foundation::{CloseHandle, ERROR_ALREADY_EXISTS, GetLastError, HANDLE},
        System::Threading::{CreateEventW, CreateMutexW, SetEvent, WaitForMultipleObjects},
        UI::WindowsAndMessaging::AllowSetForegroundWindow,
    };
    pub struct Instance {
        event: HANDLE,
        stop: HANDLE,
        mutex: HANDLE,
        worker: Option<thread::JoinHandle<()>>,
        pid_file: std::path::PathBuf,
    }
    pub enum Acquisition {
        Primary(Instance),
        Activated,
    }
    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(Some(0)).collect()
    }
    pub fn acquire(
        directory: &Path,
        wake: Arc<dyn Fn() + Send + Sync>,
        activate_only: bool,
    ) -> Result<Acquisition> {
        std::fs::create_dir_all(directory)?;
        let normalized = directory.canonicalize()?.to_string_lossy().to_lowercase();
        let key = format!("{:x}", Sha256::digest(normalized.as_bytes()));
        let event_name = wide(&format!("Local\\DownloadManager-{key}-open"));
        let mutex_name = wide(&format!("Local\\DownloadManager-{key}-instance"));
        let event = unsafe { CreateEventW(std::ptr::null(), 0, 0, event_name.as_ptr()) };
        ensure!(
            !event.is_null(),
            "Cannot create instance activation event: {}",
            std::io::Error::last_os_error()
        );
        let mutex = unsafe { CreateMutexW(std::ptr::null(), 0, mutex_name.as_ptr()) };
        let existing = unsafe { GetLastError() } == ERROR_ALREADY_EXISTS;
        if mutex.is_null() {
            unsafe {
                CloseHandle(event);
            }
            anyhow::bail!("Cannot create instance mutex");
        }
        let pid_file = directory.join("instance.pid");
        if existing {
            if let Ok(pid) = std::fs::read_to_string(&pid_file)
                .ok()
                .and_then(|v| v.trim().parse::<u32>().ok())
                .ok_or(())
            {
                unsafe {
                    AllowSetForegroundWindow(pid);
                }
            }
            let success = unsafe { SetEvent(event) } != 0;
            unsafe {
                CloseHandle(mutex);
                CloseHandle(event);
            }
            ensure!(success, "Cannot notify the existing instance");
            return Ok(Acquisition::Activated);
        }
        if activate_only {
            unsafe {
                CloseHandle(mutex);
                CloseHandle(event);
            }
            anyhow::bail!("No running instance for this data directory");
        }
        std::fs::write(&pid_file, std::process::id().to_string())
            .context("Cannot record instance owner")?;
        let stop = unsafe { CreateEventW(std::ptr::null(), 1, 0, std::ptr::null()) };
        if stop.is_null() {
            unsafe {
                CloseHandle(mutex);
                CloseHandle(event);
            }
            anyhow::bail!("Cannot create instance shutdown event");
        }
        let handles = [stop as usize, event as usize];
        let worker = thread::Builder::new()
            .name("instance-activation".into())
            .spawn(move || {
                let handles = [handles[0] as HANDLE, handles[1] as HANDLE];
                loop {
                    match unsafe { WaitForMultipleObjects(2, handles.as_ptr(), 0, u32::MAX) } {
                        0 => break,
                        1 => wake(),
                        _ => break,
                    }
                }
            })?;
        Ok(Acquisition::Primary(Instance {
            event,
            stop,
            mutex,
            worker: Some(worker),
            pid_file,
        }))
    }
    impl Drop for Instance {
        fn drop(&mut self) {
            unsafe {
                SetEvent(self.stop);
            }
            if let Some(worker) = self.worker.take() {
                let _ = worker.join();
            }
            if std::fs::read_to_string(&self.pid_file)
                .ok()
                .is_some_and(|s| s == std::process::id().to_string())
            {
                let _ = std::fs::remove_file(&self.pid_file);
            }
            unsafe {
                CloseHandle(self.event);
                CloseHandle(self.stop);
                CloseHandle(self.mutex);
            }
        }
    }
}
#[cfg(windows)]
pub use windows::*;
#[cfg(not(windows))]
pub struct Instance;
#[cfg(not(windows))]
pub enum Acquisition {
    Primary(Instance),
    Activated,
}
#[cfg(not(windows))]
pub fn acquire(
    _: &std::path::Path,
    _: std::sync::Arc<dyn Fn() + Send + Sync>,
    _: bool,
) -> anyhow::Result<Acquisition> {
    Ok(Acquisition::Primary(Instance))
}
