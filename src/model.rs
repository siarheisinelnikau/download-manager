use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const REPOSITORY: &str = "siarheisinelnikau/download-manager";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Status {
    Queued,
    Downloading,
    Pausing,
    Paused,
    Canceled,
    Failed,
    Completed,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Download {
    pub id: u64,
    pub url: String,
    pub destination: PathBuf,
    pub status: Status,
    pub bytes: u64,
    pub total: Option<u64>,
    pub error: Option<String>,
    #[serde(skip)]
    pub speed: f64,
}

impl Download {
    pub fn part_path(&self) -> PathBuf {
        self.destination
            .with_file_name(format!(".download-manager-{}.part", self.id))
    }

    pub fn meta_path(&self) -> PathBuf {
        self.part_path().with_extension("json")
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub concurrency: usize,
    pub bandwidth_kib: u64,
    pub startup_update_check: bool,
    pub default_directory: PathBuf,
    pub close_to_tray: bool,
}

impl Default for Settings {
    fn default() -> Self {
        let home = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME"));
        Self {
            concurrency: 2,
            bandwidth_kib: 0,
            startup_update_check: true,
            close_to_tray: true,
            default_directory: home
                .map(PathBuf::from)
                .unwrap_or_default()
                .join("Downloads"),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct State {
    pub schema: u32,
    pub next_id: u64,
    pub settings: Settings,
    pub downloads: Vec<Download>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            schema: 1,
            next_id: 1,
            settings: Settings::default(),
            downloads: Vec::new(),
        }
    }
}

pub fn validate_url(value: &str) -> Result<String> {
    if value.len() > 8192 {
        bail!("URL exceeds the supported 8 KiB limit");
    }
    let mut url = url::Url::parse(value.trim())?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
        bail!("Use an absolute HTTP or HTTPS URL");
    }
    if !url.username().is_empty() || url.password().is_some() {
        bail!("Credentials in URLs are not supported");
    }
    url.set_fragment(None);
    Ok(url.into())
}

pub fn validate_filename(name: &str) -> Result<()> {
    if name.is_empty()
        || name.len() > 180
        || name.ends_with([' ', '.'])
        || name
            .chars()
            .any(|c| c.is_control() || "<>:\"/\\|?*".contains(c))
        || name == "."
        || name == ".."
        || name.to_ascii_lowercase().starts_with(".download-manager-")
    {
        bail!("Choose a plain filename without reserved characters (maximum 180 bytes)");
    }
    let stem = name
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase();
    if matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL" | "CLOCK$")
        || (stem.len() == 4
            && (stem.starts_with("COM") || stem.starts_with("LPT"))
            && matches!(stem.as_bytes()[3], b'1'..=b'9'))
    {
        bail!("This filename is reserved by Windows");
    }
    Ok(())
}

pub fn suggested_filename(url: &str) -> String {
    url::Url::parse(url)
        .ok()
        .and_then(|u| u.path_segments()?.next_back().map(str::to_owned))
        .filter(|s| validate_filename(s).is_ok())
        .unwrap_or_else(|| "download.bin".into())
}

pub fn validate_destination(directory: &Path, filename: &str) -> Result<PathBuf> {
    validate_filename(filename)?;
    if !directory.is_absolute() || !directory.is_dir() {
        bail!("Destination must be an existing absolute folder");
    }
    Ok(directory.canonicalize()?.join(filename))
}
