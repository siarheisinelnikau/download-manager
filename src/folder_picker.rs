use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    NewDownload,
    Defaults,
}

#[derive(Debug)]
pub struct Request {
    pub generation: u64,
    pub target: Target,
    pub initial: PathBuf,
    pub language: download_manager::i18n::Language,
}

#[cfg(windows)]
mod native {
    use super::*;
    use anyhow::{Context, Result};
    use std::{ffi::OsString, os::windows::ffi::OsStringExt};
    use windows::{
        Win32::{
            Foundation::HWND,
            System::Com::{
                CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE,
                CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize,
            },
            UI::Shell::{
                FOS_FORCEFILESYSTEM, FOS_NOCHANGEDIR, FOS_PATHMUSTEXIST, FOS_PICKFOLDERS,
                FileOpenDialog, IFileOpenDialog, IShellItem, SHCreateItemFromParsingName,
                SIGDN_FILESYSPATH,
            },
        },
        core::HSTRING,
    };

    struct Apartment;
    impl Apartment {
        fn new() -> Result<Self> {
            unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE) }
                .ok()
                .context("Cannot initialize the Windows folder dialog")?;
            Ok(Self)
        }
    }
    impl Drop for Apartment {
        fn drop(&mut self) {
            unsafe { CoUninitialize() };
        }
    }

    fn configured(
        initial: &Path,
        language: download_manager::i18n::Language,
    ) -> Result<IFileOpenDialog> {
        // All COM objects stay on this dedicated STA until they are released.
        let dialog: IFileOpenDialog =
            unsafe { CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER) }
                .context("Cannot create the Windows folder dialog")?;
        unsafe {
            dialog.SetOptions(
                dialog.GetOptions()?
                    | FOS_PICKFOLDERS
                    | FOS_FORCEFILESYSTEM
                    | FOS_PATHMUSTEXIST
                    | FOS_NOCHANGEDIR,
            )?;
            dialog.SetTitle(&HSTRING::from(language.text("Choose download folder")))?;
            dialog.SetOkButtonLabel(&HSTRING::from(language.text("Select folder")))?;
            if let Some(folder) = initial.ancestors().find(|p| p.is_absolute() && p.is_dir()) {
                let item: IShellItem =
                    SHCreateItemFromParsingName(&HSTRING::from(folder.as_os_str()), None)?;
                dialog.SetFolder(&item)?;
            }
        }
        Ok(dialog)
    }

    fn path(item: &IShellItem) -> Result<PathBuf> {
        let text = unsafe { item.GetDisplayName(SIGDN_FILESYSPATH) }?;
        // Shell allocates this null-terminated UTF-16 string with the COM allocator.
        let result = PathBuf::from(OsString::from_wide(unsafe { text.as_wide() }));
        unsafe { CoTaskMemFree(Some(text.0.cast())) };
        Ok(result)
    }

    pub fn show(
        owner: usize,
        initial: &Path,
        language: download_manager::i18n::Language,
    ) -> Result<Option<PathBuf>> {
        let _apartment = Apartment::new()?;
        let dialog = configured(initial, language)?;
        if let Err(error) = unsafe { dialog.Show(Some(HWND(owner as _))) } {
            if error.code().0 == 0x800704c7u32 as i32 {
                // HRESULT_FROM_WIN32(ERROR_CANCELLED)
                return Ok(None);
            }
            return Err(error).context("Windows could not show the folder dialog");
        }
        let chosen = path(&unsafe { dialog.GetResult() }?)?;
        anyhow::ensure!(
            chosen.is_absolute() && chosen.is_dir(),
            "The selected folder is unavailable"
        );
        Ok(Some(chosen))
    }

    pub fn probe(initial: &Path) -> Result<()> {
        let _apartment = Apartment::new()?;
        let dialog = configured(initial, download_manager::i18n::Language::English)?;
        let actual = path(&unsafe { dialog.GetFolder() }?)?;
        anyhow::ensure!(
            actual.canonicalize()? == initial.canonicalize()?,
            "Windows did not retain the initial folder"
        );
        Ok(())
    }
}

#[cfg(windows)]
pub use native::{probe, show};

#[cfg(target_os = "linux")]
pub fn show(
    _: usize,
    initial: &Path,
    language: download_manager::i18n::Language,
) -> anyhow::Result<Option<PathBuf>> {
    use anyhow::{Context, ensure};
    use std::{ffi::OsString, os::unix::ffi::OsStringExt, process::Command};

    let mut command = Command::new("zenity");
    command.args([
        "--file-selection",
        "--directory",
        "--title",
        language.text("Choose a folder"),
    ]);
    if let Some(folder) = initial
        .ancestors()
        .find(|path| path.is_absolute() && path.is_dir())
    {
        let mut path = folder.as_os_str().to_os_string();
        path.push("/");
        command.arg("--filename").arg(path);
    }
    let output = command
        .output()
        .context("Cannot start the Ubuntu folder picker (zenity)")?;
    if output.status.code() == Some(1) && output.stdout.is_empty() {
        return Ok(None);
    }
    ensure!(
        output.status.success(),
        "Ubuntu folder picker failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let chosen = output.stdout.strip_suffix(b"\n").unwrap_or(&output.stdout);
    let folder = PathBuf::from(OsString::from_vec(chosen.to_vec()));
    ensure!(
        folder.is_absolute() && folder.is_dir(),
        "The selected folder is unavailable"
    );
    Ok(Some(folder))
}

#[cfg(not(any(windows, target_os = "linux")))]
pub fn show(
    _: usize,
    _: &Path,
    _: download_manager::i18n::Language,
) -> anyhow::Result<Option<PathBuf>> {
    anyhow::bail!("The folder dialog is unavailable on this platform")
}
#[cfg(not(windows))]
pub fn probe(_: &Path) -> anyhow::Result<()> {
    anyhow::bail!("Folder dialog configuration probe is available on Windows only")
}
