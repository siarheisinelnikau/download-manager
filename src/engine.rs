mod segments;

use crate::{model::Download, storage};
use anyhow::{Context, Result, bail, ensure};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU8, AtomicU64, Ordering},
    },
    thread,
    time::{Duration, Instant},
};
use ureq::ResponseExt;

#[derive(Clone, Debug, Serialize, Deserialize)]
struct PartialMetadata {
    url: String,
    effective_url: String,
    etag: Option<String>,
    total: Option<u64>,
    bytes: u64,
    sha256: String,
    complete: bool,
}

#[derive(Clone, Debug)]
pub struct Progress {
    pub bytes: u64,
    pub total: Option<u64>,
    pub speed: f64,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    Completed,
    Paused,
    Canceled,
}

#[derive(Default)]
pub struct Bandwidth {
    pub kib_per_second: AtomicU64,
    next: Mutex<Option<Instant>>,
}

impl Bandwidth {
    fn throttle(&self, count: usize, stopped: impl Fn() -> bool) {
        let limit = self.kib_per_second.load(Ordering::Relaxed);
        if limit == 0 {
            return;
        }
        let deadline = {
            let mut next = self.next.lock().unwrap();
            let start = next.unwrap_or_else(Instant::now).max(Instant::now());
            let end = start + Duration::from_secs_f64(count as f64 / (limit as f64 * 1024.0));
            *next = Some(end);
            end
        };
        while !stopped() && self.kib_per_second.load(Ordering::Relaxed) == limit {
            let Some(wait) = deadline.checked_duration_since(Instant::now()) else {
                break;
            };
            thread::sleep(wait.min(Duration::from_millis(50)));
        }
    }

    pub fn set_limit(&self, kib: u64) {
        self.kib_per_second.store(kib, Ordering::Relaxed);
        *self.next.lock().unwrap() = None;
    }
}

fn agent(https_only: bool) -> ureq::Agent {
    ureq::Agent::config_builder()
        .http_status_as_error(false)
        .https_only(https_only)
        .max_redirects(8)
        .timeout_resolve(Some(Duration::from_secs(10)))
        .timeout_connect(Some(Duration::from_secs(10)))
        .timeout_recv_response(Some(Duration::from_secs(15)))
        .timeout_per_read(Some(Duration::from_secs(10)))
        .timeout_per_write(Some(Duration::from_secs(10)))
        .user_agent(concat!("DownloadManager/", env!("CARGO_PKG_VERSION")))
        .build()
        .into()
}

/// Streaming response boundary, also used by offline fault-injection tests.
pub struct Response {
    pub status: u16,
    pub effective_url: String,
    pub headers: BTreeMap<String, String>,
    pub body: Box<dyn Read>,
}

impl Response {
    fn header(&self, key: &str) -> Option<String> {
        self.headers.get(key).cloned()
    }
}

/// The real network adapter. Range and If-Range are sent together on resume.
fn fetch(url: &str, resume: Option<(u64, &str)>) -> Result<Response> {
    fetch_range(url, resume.map(|(start, etag)| (start, None, etag)))
}

fn fetch_range(url: &str, range: Option<(u64, Option<u64>, &str)>) -> Result<Response> {
    let agent = agent(url.starts_with("https:"));
    let mut request = agent.get(url).header("Accept-Encoding", "identity");
    if let Some((start, end, etag)) = range {
        let range = match end {
            Some(end) => format!("bytes={start}-{end}"),
            None => format!("bytes={start}-"),
        };
        request = request.header("Range", &range).header("If-Range", etag);
    }
    let response = request.call().context(
        "Request failed (network, TLS certificate, redirect or timeout). Partial data is preserved",
    )?;
    let status = response.status().as_u16();
    let effective_url = response.get_uri().to_string();
    let headers = response
        .headers()
        .iter()
        .filter(|(key, _)| {
            matches!(
                key.as_str(),
                "content-encoding" | "etag" | "content-length" | "content-range" | "accept-ranges"
            )
        })
        .map(|(key, value)| Ok((key.as_str().to_owned(), value.to_str()?.to_owned())))
        .collect::<Result<BTreeMap<_, _>>>()?;
    Ok(Response {
        status,
        effective_url,
        headers,
        body: Box::new(response.into_body().into_reader()),
    })
}

fn strong_etag(value: Option<String>) -> Option<String> {
    value.filter(|s| {
        s.len() >= 2 && s.starts_with('"') && s.ends_with('"') && !s[1..s.len() - 1].contains('"')
    })
}

pub fn parse_content_range(value: &str) -> Result<(u64, u64, u64)> {
    let rest = value
        .strip_prefix("bytes ")
        .context("Invalid Content-Range unit")?;
    let (range, total) = rest.split_once('/').context("Invalid Content-Range")?;
    let (start, end) = range
        .split_once('-')
        .context("Invalid Content-Range bounds")?;
    let (start, end, total): (u64, u64, u64) = (start.parse()?, end.parse()?, total.parse()?);
    ensure!(start <= end && end < total, "Invalid Content-Range bounds");
    Ok((start, end, total))
}

fn checkpoint(
    file: &File,
    job: &Download,
    meta: &mut PartialMetadata,
    done: u64,
    hash: &Sha256,
    complete: bool,
) -> Result<()> {
    file.sync_all()
        .context("Cannot flush partial file (check free disk space)")?;
    meta.bytes = done;
    meta.sha256 = format!("{:x}", hash.clone().finalize());
    meta.complete = complete;
    storage::atomic_json(&job.meta_path(), meta).context("Cannot save resume checkpoint")
}

pub fn transfer(
    job: &Download,
    control: Arc<AtomicU8>,
    bandwidth: Arc<Bandwidth>,
    progress: impl FnMut(Progress),
) -> Result<Outcome> {
    transfer_with_connections(job, control, bandwidth, 1, progress)
}

pub fn transfer_with_connections(
    job: &Download,
    control: Arc<AtomicU8>,
    bandwidth: Arc<Bandwidth>,
    connections: usize,
    progress: impl FnMut(Progress),
) -> Result<Outcome> {
    transfer_from(job, control, bandwidth, connections, progress, fetch)
}

pub fn transfer_with_source(
    job: &Download,
    control: Arc<AtomicU8>,
    bandwidth: Arc<Bandwidth>,
    progress: impl FnMut(Progress),
    source: impl FnOnce(&str, Option<(u64, &str)>) -> Result<Response>,
) -> Result<Outcome> {
    transfer_from(job, control, bandwidth, 1, progress, source)
}

const SAMPLE_INTERVAL: Duration = Duration::from_millis(250);

struct SpeedMeter {
    sampled_at: Instant,
    sampled_bytes: u64,
    speed: Option<f64>,
}

impl SpeedMeter {
    fn new(bytes: u64) -> Self {
        Self {
            sampled_at: Instant::now(),
            sampled_bytes: bytes,
            speed: None,
        }
    }

    fn sample(&mut self, bytes: u64, total: Option<u64>) -> Option<Progress> {
        let now = Instant::now();
        let elapsed = now.duration_since(self.sampled_at);
        if elapsed < SAMPLE_INTERVAL {
            return None;
        }
        let seconds = elapsed.as_secs_f64();
        let rate = bytes.saturating_sub(self.sampled_bytes) as f64 / seconds;
        let weight = 1.0 - (-seconds / 2.0).exp();
        let speed = self
            .speed
            .map_or(rate, |previous| previous + weight * (rate - previous));
        self.sampled_at = now;
        self.sampled_bytes = bytes;
        self.speed = Some(speed);
        Some(Progress {
            bytes,
            total,
            speed,
        })
    }
}

fn transfer_from(
    job: &Download,
    control: Arc<AtomicU8>,
    bandwidth: Arc<Bandwidth>,
    connections: usize,
    mut progress: impl FnMut(Progress),
    source: impl FnOnce(&str, Option<(u64, &str)>) -> Result<Response>,
) -> Result<Outcome> {
    // A crash can occur between the final rename and durable queue completion.
    // A retained, verified completion receipt makes that window recoverable.
    if job.destination.exists() && !job.part_path().exists() && job.meta_path().exists() {
        let receipt: PartialMetadata = storage::read_json(&job.meta_path(), 64 * 1024)?;
        if receipt.complete && receipt.url == job.url {
            let mut final_file = File::open(&job.destination)?;
            ensure!(
                final_file.metadata()?.len() == receipt.bytes,
                "Completed file length changed; original file preserved"
            );
            let mut hash = Sha256::new();
            let mut buffer = [0u8; 32 * 1024];
            loop {
                if control.load(Ordering::Relaxed) != 0 {
                    return Ok(control_outcome(&control));
                }
                let count = final_file.read(&mut buffer)?;
                if count == 0 {
                    break;
                }
                hash.update(&buffer[..count]);
            }
            ensure!(
                format!("{:x}", hash.finalize()) == receipt.sha256,
                "Completed file checksum changed; original file preserved"
            );
            progress(Progress {
                bytes: receipt.bytes,
                total: Some(receipt.bytes),
                speed: 0.0,
            });
            return Ok(Outcome::Completed);
        }
    }
    ensure!(
        !job.destination.exists(),
        "Destination already exists; choose another filename. It has not been overwritten."
    );
    let part_path = job.part_path();
    let mut file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(&part_path)
        .context("Cannot create partial file (check folder permissions and free space)")?;
    file.try_lock_exclusive()
        .context("This partial file is already in use")?;
    let length = file.metadata()?.len();
    let empty_hash = Sha256::new();
    let mut meta = if job.meta_path().exists() {
        let meta: PartialMetadata = storage::read_json(&job.meta_path(), 64 * 1024)?;
        ensure!(
            meta.url == job.url,
            "Resume metadata belongs to a different URL"
        );
        ensure!(
            length >= meta.bytes,
            "Partial file is shorter than its checkpoint; use Restart from zero"
        );
        meta
    } else {
        ensure!(
            length == 0,
            "Partial file has no resume metadata; use Restart from zero"
        );
        PartialMetadata {
            url: job.url.clone(),
            effective_url: String::new(),
            etag: None,
            total: None,
            bytes: 0,
            sha256: format!("{:x}", empty_hash.clone().finalize()),
            complete: false,
        }
    };
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 32 * 1024];
    let mut remaining = meta.bytes;
    while remaining > 0 {
        if control.load(Ordering::Relaxed) != 0 {
            return Ok(control_outcome(&control));
        }
        let count = remaining.min(buffer.len() as u64) as usize;
        file.read_exact(&mut buffer[..count])
            .context("Cannot verify partial file")?;
        hash.update(&buffer[..count]);
        remaining -= count as u64;
    }
    ensure!(
        format!("{:x}", hash.clone().finalize()) == meta.sha256,
        "Partial file checksum changed; use Restart from zero"
    );
    file.set_len(meta.bytes)?;
    file.seek(SeekFrom::Start(meta.bytes))?;
    if meta.complete {
        drop(file);
        storage::finalize(&part_path, &job.destination)
            .context("Cannot finalize file; check permissions and destination name")?;
        progress(Progress {
            bytes: meta.bytes,
            total: Some(meta.bytes),
            speed: 0.0,
        });
        return Ok(Outcome::Completed);
    }
    if control.load(Ordering::Relaxed) != 0 {
        return Ok(control_outcome(&control));
    }
    let offset = meta.bytes;
    if offset > 0 && meta.etag.is_none() {
        bail!("Server supplied no strong ETag; safe resume is unavailable. Use Restart from zero.");
    }
    let response = source(
        &job.url,
        if offset > 0 {
            Some((offset, meta.etag.as_deref().unwrap()))
        } else {
            None
        },
    )?;
    let effective = response.effective_url.clone();
    ensure!(
        url::Url::parse(&effective)?.scheme() != "http" || !job.url.starts_with("https:"),
        "HTTPS was redirected to insecure HTTP; download stopped"
    );
    ensure!(
        response
            .header("content-encoding")
            .is_none_or(|e| e.eq_ignore_ascii_case("identity")),
        "Server returned an encoded body despite Accept-Encoding: identity"
    );
    let etag = strong_etag(response.header("etag"));
    let content_length = response
        .header("content-length")
        .map(|v| v.parse::<u64>())
        .transpose()
        .context("Invalid Content-Length")?;
    let total;
    if offset > 0 {
        ensure!(
            response.status == 206,
            "Server refused resume or resource changed (HTTP {}). Partial data preserved; use Restart from zero.",
            response.status
        );
        let range = parse_content_range(
            &response
                .header("content-range")
                .context("Missing Content-Range; partial data preserved")?,
        )?;
        ensure!(
            range.0 == offset && range.1 == range.2 - 1,
            "Server returned an unexpected byte range; partial data preserved"
        );
        ensure!(
            content_length.is_none_or(|n| n == range.1 - range.0 + 1),
            "Content-Length disagrees with Content-Range"
        );
        ensure!(
            etag == meta.etag
                && effective == meta.effective_url
                && meta.total.is_none_or(|n| n == range.2),
            "Remote entity or redirect target changed; use Restart from zero"
        );
        total = Some(range.2);
    } else {
        ensure!(
            response.status == 200,
            "Server returned HTTP {}; expected 200 OK",
            response.status
        );
        total = content_length;
        meta.etag = etag;
        meta.effective_url = effective;
    }
    meta.total = total;
    checkpoint(&file, job, &mut meta, offset, &hash, false)?;
    let mut done = offset;
    let mut last_checkpoint = offset;
    progress(Progress {
        bytes: done,
        total,
        speed: 0.0,
    });
    let ranges_allowed = response
        .header("accept-ranges")
        .is_none_or(|value| !value.eq_ignore_ascii_case("none"));
    let mut fallback = Some(response);
    let streaming: Result<Outcome> = (|| {
        if ranges_allowed
            && connections > 1
            && let (Some(total), Some(etag)) = (total, meta.etag.clone())
        {
            let mut meter = SpeedMeter::new(offset);
            let effective_url = meta.effective_url.clone();
            let request = segments::Request {
                url: &job.url,
                effective_url: &effective_url,
                etag: &etag,
                offset,
                total,
                connections,
            };
            if let Some(outcome) = segments::stream(
                request,
                &mut file,
                &control,
                &bandwidth,
                &mut fallback,
                |file, prefix, received| {
                    if prefix > done {
                        file.seek(SeekFrom::Start(done))?;
                        while done < prefix {
                            let count = (prefix - done).min(buffer.len() as u64) as usize;
                            file.read_exact(&mut buffer[..count])?;
                            hash.update(&buffer[..count]);
                            done += count as u64;
                        }
                    }
                    if done - last_checkpoint >= 2 * 1024 * 1024 {
                        checkpoint(file, job, &mut meta, done, &hash, false)?;
                        last_checkpoint = done;
                    }
                    if let Some(sample) = meter.sample(received, Some(total)) {
                        progress(sample);
                    }
                    Ok(())
                },
            )? {
                ensure!(
                    outcome != Outcome::Completed || done == total,
                    "Parallel transfer ended before the declared length"
                );
                return Ok(outcome);
            }
        }
        let mut meter = SpeedMeter::new(offset);
        let mut reader = fallback.take().unwrap().body;
        loop {
            if control.load(Ordering::Relaxed) != 0 {
                return Ok(control_outcome(&control));
            }
            // Keep each bandwidth reservation short, even at a very low configured limit.
            let limit = bandwidth.kib_per_second.load(Ordering::Relaxed);
            let size = if limit == 0 {
                buffer.len()
            } else {
                buffer.len().min((limit * 1024 / 10).max(1) as usize)
            };
            let read = reader.read(&mut buffer[..size]).context(
                "Transfer interrupted or timed out; checkpoint preserved. Retry to resume.",
            )?;
            if read == 0 {
                ensure!(
                    total.is_none_or(|n| done == n),
                    "Transfer ended before the declared length; retry to resume"
                );
                return Ok(Outcome::Completed);
            }
            let next_done = done
                .checked_add(read as u64)
                .context("Download is too large")?;
            ensure!(
                total.is_none_or(|n| next_done <= n),
                "Response exceeded declared length; checkpoint preserved"
            );
            bandwidth.throttle(read, || control.load(Ordering::Relaxed) != 0);
            file.write_all(&buffer[..read]).context("Cannot write partial file; check free disk space and permissions. Checkpoint preserved.")?;
            hash.update(&buffer[..read]);
            done = next_done;
            if done - last_checkpoint >= 2 * 1024 * 1024 {
                checkpoint(&file, job, &mut meta, done, &hash, false)?;
                last_checkpoint = done;
            }
            if let Some(sample) = meter.sample(done, total) {
                progress(sample);
            }
        }
    })();
    if !matches!(streaming, Ok(Outcome::Completed)) {
        // A short write may have placed uncommitted bytes beyond the hashed prefix.
        // The previous checkpoint remains usable if flushing this one fails.
        file.set_len(done)
            .context("Cannot trim uncommitted partial bytes; previous checkpoint preserved")?;
        checkpoint(&file, job, &mut meta, done, &hash, false)?;
        progress(Progress {
            bytes: done,
            total,
            speed: 0.0,
        });
        return streaming;
    }
    checkpoint(&file, job, &mut meta, done, &hash, true)?;
    drop(file);
    storage::finalize(&part_path, &job.destination).context("Cannot finalize file; destination may exist or disk access failed. Retry preserves the complete partial file.")?;
    progress(Progress {
        bytes: done,
        total: Some(done),
        speed: 0.0,
    });
    Ok(Outcome::Completed)
}

fn control_outcome(control: &AtomicU8) -> Outcome {
    if control.load(Ordering::Relaxed) == 2 {
        Outcome::Canceled
    } else {
        Outcome::Paused
    }
}

pub fn restart(job: &Download) -> Result<()> {
    for path in [job.part_path(), job.meta_path()] {
        match fs::remove_file(path) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e).context("Cannot remove managed partial data"),
        }
    }
    Ok(())
}
