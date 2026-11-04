use super::*;

pub(super) struct Request<'a> {
    pub url: &'a str,
    pub effective_url: &'a str,
    pub etag: &'a str,
    pub offset: u64,
    pub total: u64,
    pub connections: usize,
}

struct Segment {
    start: u64,
    end: u64,
    written: AtomicU64,
}

impl Segment {
    fn response(&self, request: &Request<'_>) -> Result<Response> {
        let response = fetch_range(
            request.url,
            Some((self.start, Some(self.end - 1), request.etag)),
        )?;
        ensure!(
            response.status == 206
                && response.effective_url == request.effective_url
                && response
                    .headers
                    .get("etag")
                    .is_some_and(|tag| tag == request.etag)
                && response
                    .headers
                    .get("content-encoding")
                    .is_none_or(|encoding| encoding.eq_ignore_ascii_case("identity")),
            "Server rejected a verified segment"
        );
        let range = response
            .headers
            .get("content-range")
            .context("Missing Content-Range")?;
        ensure!(
            parse_content_range(range)? == (self.start, self.end - 1, request.total)
                && response.headers.get("content-length").is_none_or(|length| {
                    length.parse::<u64>().ok() == Some(self.end - self.start)
                }),
            "Server returned an unexpected segment length or range"
        );
        Ok(response)
    }

    fn write(
        &self,
        mut response: Response,
        file: &Mutex<&mut File>,
        bandwidth: &Bandwidth,
        workers: usize,
        stopped: impl Fn() -> bool,
    ) -> Result<()> {
        let mut buffer = [0u8; 32 * 1024];
        let mut position = self.start;
        while position < self.end && !stopped() {
            let limit = bandwidth.kib_per_second.load(Ordering::Relaxed);
            let quantum = if limit == 0 {
                buffer.len()
            } else {
                (limit * 1024 / (10 * workers as u64)).max(1) as usize
            };
            let size = (self.end - position).min(quantum.min(buffer.len()) as u64) as usize;
            let count = response.body.read(&mut buffer[..size])?;
            ensure!(count != 0, "Segment ended before the declared range length");
            let next = position + count as u64;
            if next == self.end {
                ensure!(
                    response.body.read(&mut [0])? == 0,
                    "Segment exceeded its range"
                );
            }
            bandwidth.throttle(count, &stopped);
            if stopped() {
                break;
            }
            let mut file = file.lock().unwrap();
            file.seek(SeekFrom::Start(position))?;
            file.write_all(&buffer[..count])
                .context("Cannot write partial file")?;
            position = next;
            self.written.store(position - self.start, Ordering::Relaxed);
        }
        Ok(())
    }
}

/// Use the original serial response if any segment is rejected before writing.
/// Once streaming starts, only the verified contiguous prefix can be checkpointed.
pub(super) fn stream(
    request: Request<'_>,
    file: &mut File,
    control: &AtomicU8,
    bandwidth: &Bandwidth,
    fallback: &mut Option<Response>,
    mut sample: impl FnMut(&mut File, u64, u64) -> Result<()>,
) -> Result<Option<Outcome>> {
    use std::sync::{atomic::AtomicBool, mpsc};

    let remaining = request.total - request.offset;
    let workers = (remaining / (256 * 1024))
        .min(request.connections.min(crate::model::MAX_CONNECTIONS) as u64);
    if workers < 2 {
        return Ok(None);
    }
    let mut start = request.offset;
    let segments: Vec<_> = (0..workers)
        .map(|index| {
            let end = start + remaining / workers + u64::from(index < remaining % workers);
            let segment = Segment {
                start,
                end,
                written: AtomicU64::new(0),
            };
            start = end;
            segment
        })
        .collect();

    let file = Mutex::new(file);
    let abort = AtomicBool::new(false);
    let stopped = || abort.load(Ordering::Relaxed) || control.load(Ordering::Relaxed) != 0;
    let (ready_tx, ready_rx) = mpsc::channel();
    let (done_tx, done_rx) = mpsc::channel();

    thread::scope(|scope| {
        let mut gates = Vec::new();
        let mut handles = Vec::new();
        for segment in &segments {
            let (start_tx, start_rx) = mpsc::channel();
            let ready_tx = ready_tx.clone();
            let done_tx = done_tx.clone();
            let request = &request;
            let file = &file;
            let abort = &abort;
            handles.push(thread::Builder::new().spawn_scoped(scope, move || {
                let response = segment.response(request);
                let _ = ready_tx.send(response.is_ok());
                drop(ready_tx);
                let result = if let Ok(response) = response
                    && start_rx.recv().is_ok()
                {
                    segment.write(response, file, bandwidth, workers as usize, stopped)
                } else {
                    Ok(())
                };
                if result.is_err() {
                    abort.store(true, Ordering::Relaxed);
                }
                let _ = done_tx.send(());
                result
            })?);
            gates.push(start_tx);
        }
        drop(ready_tx);
        drop(done_tx);

        let mut report = || -> Result<()> {
            let mut file = file.lock().unwrap();
            let mut prefix = request.offset;
            let mut received = request.offset;
            for segment in &segments {
                let count = segment.written.load(Ordering::Relaxed);
                received += count;
                if prefix == segment.start {
                    prefix += count;
                }
            }
            sample(&mut file, prefix, received)
        };
        if !(0..workers).all(|_| ready_rx.recv() == Ok(true)) || stopped() {
            return Ok(None);
        }
        drop(fallback.take());
        for gate in gates {
            let _ = gate.send(());
        }

        let mut finished = 0;
        let mut next_sample = Instant::now() + SAMPLE_INTERVAL;
        let mut error = None;
        while finished < workers {
            match done_rx.recv_timeout(next_sample.saturating_duration_since(Instant::now())) {
                Ok(()) => finished += 1,
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    if let Err(e) = report() {
                        error = Some(e);
                        abort.store(true, Ordering::Relaxed);
                        break;
                    }
                    next_sample = Instant::now() + SAMPLE_INTERVAL;
                }
            }
        }
        for handle in handles {
            let result = handle
                .join()
                .unwrap_or_else(|_| Err(anyhow::anyhow!("Segment worker failed")));
            if let Err(e) = result {
                error.get_or_insert(e);
            }
        }
        let last_sample = report();
        if let Some(error) = error {
            return Err(error)
                .context("Parallel transfer interrupted; retry to resume from the checkpoint");
        }
        last_sample?;
        if control.load(Ordering::Relaxed) != 0 {
            return Ok(Some(control_outcome(control)));
        }
        Ok(Some(Outcome::Completed))
    })
}
