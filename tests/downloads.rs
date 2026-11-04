use download_manager::{
    engine::{self, Bandwidth, Outcome},
    model::{Download, Status},
};
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::{
        Arc,
        atomic::{AtomicU8, Ordering},
    },
    thread,
    time::Duration,
};
use tempfile::TempDir;
mod support;
use support::fixture_dir;

fn accept(listener: &TcpListener) -> TcpStream {
    listener.set_nonblocking(true).unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        match listener.accept() {
            Ok((stream, _)) => {
                // Windows may inherit the listener's nonblocking mode on accepted sockets.
                // The fixture reads synchronously with a timeout after this bounded accept.
                stream.set_nonblocking(false).unwrap();
                return stream;
            }
            Err(e)
                if e.kind() == std::io::ErrorKind::WouldBlock
                    && std::time::Instant::now() < deadline =>
            {
                thread::sleep(Duration::from_millis(10))
            }
            Err(e) => panic!("Loopback accept failed or timed out: {e}"),
        }
    }
}

fn server(
    handler: impl FnOnce(TcpStream, String) + Send + 'static,
) -> (String, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/fixture", listener.local_addr().unwrap());
    let handle = thread::spawn(move || {
        let mut stream = accept(&listener);
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut request = Vec::new();
        while !request.ends_with(b"\r\n\r\n") {
            let mut b = [0];
            stream.read_exact(&mut b).unwrap();
            request.push(b[0]);
            assert!(request.len() < 16384);
        }
        handler(stream, String::from_utf8(request).unwrap());
    });
    (url, handle)
}

fn job(url: String, directory: &TempDir) -> Download {
    Download {
        id: 1,
        url,
        destination: directory.path().join("fixture.bin"),
        status: Status::Queued,
        bytes: 0,
        total: None,
        error: None,
        speed: 0.0,
    }
}

fn run(job: &Download) -> anyhow::Result<Outcome> {
    engine::transfer(
        job,
        Arc::new(AtomicU8::new(0)),
        Arc::new(Bandwidth::default()),
        |_| {},
    )
}

#[test]
fn downloads_known_and_unknown_length_without_overwriting() {
    for header in ["Content-Length: 11\r\n", ""] {
        let (url, handle) = server(move |mut s, _| {
            write!(
                s,
                "HTTP/1.1 200 OK\r\n{header}Connection: close\r\n\r\nhello world"
            )
            .unwrap();
        });
        let dir = fixture_dir();
        let job = job(url, &dir);
        assert_eq!(run(&job).unwrap(), Outcome::Completed);
        assert_eq!(std::fs::read(&job.destination).unwrap(), b"hello world");
        std::fs::remove_file(job.meta_path()).unwrap();
        assert!(
            run(&job)
                .unwrap_err()
                .to_string()
                .contains("already exists")
        );
        handle.join().unwrap();
    }
}

// A two-request fixture keeps the original URL stable while changing server behavior.
fn interrupted_fixture(
    second: impl FnOnce(TcpStream, String) + Send + 'static,
) -> (String, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/fixture", listener.local_addr().unwrap());
    let handle = thread::spawn(move || {
        for index in 0..2 {
            let mut s = accept(&listener);
            s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
            let mut bytes = Vec::new();
            while !bytes.ends_with(b"\r\n\r\n") {
                let mut b = [0];
                s.read_exact(&mut b).unwrap();
                bytes.push(b[0]);
            }
            let req = String::from_utf8(bytes).unwrap().to_ascii_lowercase();
            if index == 0 {
                write!(s, "HTTP/1.1 200 OK\r\nETag: \"v1\"\r\nContent-Length: 10\r\nConnection: close\r\n\r\nabcde").unwrap();
            } else {
                second(s, req);
                break;
            }
        }
    });
    (url, handle)
}

#[test]
fn interrupted_transfer_resumes_with_range_and_if_range() {
    let (url, handle) = interrupted_fixture(|mut s, req| {
        assert!(req.contains("range: bytes=5-\r\n"));
        assert!(req.contains("if-range: \"v1\"\r\n"));
        write!(s, "HTTP/1.1 206 Partial Content\r\nETag: \"v1\"\r\nContent-Range: bytes 5-9/10\r\nContent-Length: 5\r\nConnection: close\r\n\r\nfghij").unwrap();
    });
    let dir = fixture_dir();
    let job = job(url, &dir);
    assert!(run(&job).is_err());
    assert_eq!(std::fs::read(job.part_path()).unwrap(), b"abcde");
    assert_eq!(run(&job).unwrap(), Outcome::Completed);
    assert_eq!(std::fs::read(&job.destination).unwrap(), b"abcdefghij");
    handle.join().unwrap();
}

#[test]
fn unsafe_range_responses_preserve_partial_bytes() {
    for response in [
        "200 OK\r\nETag: \"v2\"\r\nContent-Length: 5",
        "206 Partial Content\r\nETag: \"v2\"\r\nContent-Range: bytes 5-9/10\r\nContent-Length: 5",
        "206 Partial Content\r\nETag: \"v1\"\r\nContent-Range: bytes 4-8/10\r\nContent-Length: 5",
        "206 Partial Content\r\nETag: \"v1\"\r\nContent-Range: bytes 5-9/11\r\nContent-Length: 5",
        "416 Range Not Satisfiable\r\nContent-Length: 5",
        "206 Partial Content\r\nETag: \"v1\"\r\nContent-Length: 5",
    ] {
        let (url, handle) = interrupted_fixture(move |mut s, _| {
            write!(s, "HTTP/1.1 {response}\r\nConnection: close\r\n\r\nfghij").unwrap();
        });
        let dir = fixture_dir();
        let job = job(url, &dir);
        assert!(run(&job).is_err());
        assert!(run(&job).is_err());
        assert_eq!(std::fs::read(job.part_path()).unwrap(), b"abcde");
        assert!(!job.destination.exists());
        handle.join().unwrap();
    }
}

#[test]
fn cancel_and_pause_preserve_data_and_prevent_finalization() {
    for signal in [1, 2] {
        let dir = fixture_dir();
        let job = job("http://127.0.0.1:1/unused".into(), &dir);
        let result = engine::transfer(
            &job,
            Arc::new(AtomicU8::new(signal)),
            Arc::new(Bandwidth::default()),
            |_| {},
        )
        .unwrap();
        assert_eq!(
            result,
            if signal == 1 {
                Outcome::Paused
            } else {
                Outcome::Canceled
            }
        );
        assert!(!job.destination.exists());
    }
}

#[test]
fn edited_partial_is_rejected_before_any_network_request() {
    let (url, handle) = server(|mut s, _| {
        write!(s, "HTTP/1.1 200 OK\r\nETag: \"v1\"\r\nContent-Length: 10\r\nConnection: close\r\n\r\nabcde").unwrap();
    });
    let dir = fixture_dir();
    let job = job(url, &dir);
    assert!(run(&job).is_err());
    handle.join().unwrap();
    std::fs::write(job.part_path(), b"xxxxx").unwrap();
    assert!(
        run(&job)
            .unwrap_err()
            .to_string()
            .contains("checksum changed")
    );
}

#[test]
fn destination_race_does_not_replace_existing_file() {
    let dir = fixture_dir();
    let destination = dir.path().join("fixture.bin");
    let created = destination.clone();
    let (url, handle) = server(move |mut s, _| {
        std::fs::write(created, b"keep me").unwrap();
        write!(
            s,
            "HTTP/1.1 200 OK\r\nContent-Length: 5\r\nConnection: close\r\n\r\nabcde"
        )
        .unwrap();
    });
    let job = job(url, &dir);
    assert!(run(&job).is_err());
    assert_eq!(std::fs::read(destination).unwrap(), b"keep me");
    assert_eq!(std::fs::read(job.part_path()).unwrap(), b"abcde");
    handle.join().unwrap();
}

#[test]
fn checkpoint_ignores_uncommitted_tail() {
    let (url, handle) = interrupted_fixture(|mut s, _| {
        write!(s, "HTTP/1.1 206 Partial Content\r\nETag: \"v1\"\r\nContent-Range: bytes 5-9/10\r\nContent-Length: 5\r\nConnection: close\r\n\r\nfghij").unwrap();
    });
    let dir = fixture_dir();
    let job = job(url, &dir);
    assert!(run(&job).is_err());
    std::fs::OpenOptions::new()
        .append(true)
        .open(job.part_path())
        .unwrap()
        .write_all(b"uncommitted tail")
        .unwrap();
    assert_eq!(run(&job).unwrap(), Outcome::Completed);
    assert_eq!(std::fs::read(job.destination).unwrap(), b"abcdefghij");
    handle.join().unwrap();
}

#[test]
fn controls_are_observed_while_streaming() {
    let (url, handle) = server(|mut s, _| {
        write!(
            s,
            "HTTP/1.1 200 OK\r\nETag: \"v1\"\r\nContent-Length: 65536\r\nConnection: close\r\n\r\n"
        )
        .unwrap();
        let _ = s.write_all(&[7; 65536]);
    });
    let dir = fixture_dir();
    let job = job(url, &dir);
    let control = Arc::new(AtomicU8::new(0));
    let stop = control.clone();
    let result = engine::transfer(&job, control, Arc::new(Bandwidth::default()), |_| {
        stop.store(1, Ordering::Relaxed);
    })
    .unwrap();
    assert_eq!(result, Outcome::Paused);
    assert!(!job.destination.exists());
    handle.join().unwrap();
}

#[test]
fn follows_http_redirects_and_decodes_chunked_bodies() {
    let (target, body_handle) = server(|mut s, _| {
        write!(s, "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n5\r\nhello\r\n6\r\n world\r\n0\r\n\r\n").unwrap();
    });
    let (url, redirect_handle) = server(move |mut s, _| {
        write!(s, "HTTP/1.1 302 Found\r\nLocation: {target}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
    });
    let dir = fixture_dir();
    let job = job(url, &dir);
    assert_eq!(run(&job).unwrap(), Outcome::Completed);
    assert_eq!(std::fs::read(job.destination).unwrap(), b"hello world");
    redirect_handle.join().unwrap();
    body_handle.join().unwrap();
}

#[test]
fn parallel_ranges_join_in_order_and_rejected_ranges_fall_back() {
    const HALF: usize = 256 * 1024;
    const SIZE: usize = HALF * 2;

    for reject_segment in [false, true] {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/fixture", listener.local_addr().unwrap());
        let server = thread::spawn(move || {
            thread::scope(|scope| {
                let mut handlers = Vec::new();
                for _ in 0..3 {
                    let mut stream = accept(&listener);
                    handlers.push(scope.spawn(move || {
                        use std::io::{BufRead, BufReader};

                        stream
                            .set_read_timeout(Some(Duration::from_secs(5)))
                            .unwrap();
                        stream
                            .set_write_timeout(Some(Duration::from_secs(5)))
                            .unwrap();
                        let mut reader = BufReader::new(&mut stream);
                        let mut range = None;
                        let mut if_range = None;
                        loop {
                            let mut line = String::new();
                            assert!(reader.read_line(&mut line).unwrap() > 0);
                            if line == "\r\n" {
                                break;
                            }
                            if let Some((name, value)) = line.split_once(':') {
                                match name.to_ascii_lowercase().as_str() {
                                    "range" => range = Some(value.trim().to_owned()),
                                    "if-range" => if_range = Some(value.trim().to_owned()),
                                    _ => {}
                                }
                            }
                        }
                        let Some(range) = range else {
                            write!(
                                stream,
                                "HTTP/1.1 200 OK\r\nETag: \"v1\"\r\nContent-Length: {SIZE}\r\nConnection: close\r\n\r\n"
                            )
                            .unwrap();
                            // The segmented path drops this response after all range headers pass.
                            let _ = stream.write_all(&vec![29; HALF]);
                            let _ = stream.write_all(&vec![71; HALF]);
                            return None;
                        };
                        assert_eq!(if_range.as_deref(), Some("\"v1\""));
                        let first = range == format!("bytes=0-{}", HALF - 1);
                        let (start, end, value) = if first {
                            (0, HALF - 1, 17)
                        } else {
                            assert_eq!(range, format!("bytes={HALF}-{}", SIZE - 1));
                            (HALF, SIZE - 1, 53)
                        };
                        let tag = if reject_segment && !first { "v2" } else { "v1" };
                        write!(
                            stream,
                            "HTTP/1.1 206 Partial Content\r\nETag: \"{tag}\"\r\nContent-Range: bytes {start}-{end}/{SIZE}\r\nContent-Length: {HALF}\r\nConnection: close\r\n\r\n"
                        )
                        .unwrap();
                        let _ = stream.write_all(&vec![value; HALF]);
                        Some(start)
                    }));
                }
                let mut offsets: Vec<_> = handlers
                    .into_iter()
                    .filter_map(|handler| handler.join().unwrap())
                    .collect();
                offsets.sort_unstable();
                assert_eq!(offsets, [0, HALF]);
            });
        });

        let dir = fixture_dir();
        let job = job(url, &dir);
        let result = engine::transfer_with_connections(
            &job,
            Arc::new(AtomicU8::new(0)),
            Arc::new(Bandwidth::default()),
            32,
            |_| {},
        );
        assert_eq!(result.unwrap(), Outcome::Completed);
        let actual = std::fs::read(&job.destination).unwrap();
        assert_eq!(actual.len(), SIZE);
        let (first, second) = if reject_segment { (29, 71) } else { (17, 53) };
        assert!(actual[..HALF].iter().all(|&byte| byte == first));
        assert!(actual[HALF..].iter().all(|&byte| byte == second));
        server.join().unwrap();
    }
}
