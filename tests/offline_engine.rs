use download_manager::{
    engine::{self, Bandwidth, Outcome, Response},
    model::{Download, Status},
    storage,
};
use std::{
    collections::BTreeMap,
    io::{self, Cursor, Read, Write},
    sync::{Arc, atomic::AtomicU8},
};
use tempfile::TempDir;

fn job(dir: &TempDir) -> Download {
    Download {
        id: 4,
        url: "https://example.test/file".into(),
        destination: dir.path().join("file.bin"),
        status: Status::Queued,
        bytes: 0,
        total: None,
        error: None,
        speed: 0.0,
    }
}
fn response(status: u16, headers: &[(&str, &str)], bytes: &[u8]) -> Response {
    Response {
        status,
        effective_url: "https://example.test/file".into(),
        headers: headers
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect::<BTreeMap<_, _>>(),
        body: Box::new(Cursor::new(bytes.to_vec())),
    }
}
fn transfer(
    job: &Download,
    response: Response,
    expected_resume: Option<(u64, &str)>,
) -> anyhow::Result<Outcome> {
    engine::transfer_with_source(
        job,
        Arc::new(AtomicU8::new(0)),
        Arc::new(Bandwidth::default()),
        |_| {},
        |url, resume| {
            assert_eq!(url, job.url);
            assert_eq!(resume, expected_resume);
            Ok(response)
        },
    )
}
fn partial(job: &Download) {
    assert!(
        transfer(
            job,
            response(
                200,
                &[("content-length", "10"), ("etag", "\"v1\"")],
                b"abcde"
            ),
            None
        )
        .is_err()
    );
    assert_eq!(std::fs::read(job.part_path()).unwrap(), b"abcde");
}

#[test]
fn completes_known_and_unknown_lengths_and_refuses_overwrite() {
    for headers in [vec![("content-length", "5")], vec![]] {
        let dir = TempDir::new().unwrap();
        let job = job(&dir);
        assert_eq!(
            transfer(&job, response(200, &headers, b"abcde"), None).unwrap(),
            Outcome::Completed
        );
        assert_eq!(std::fs::read(&job.destination).unwrap(), b"abcde");
        assert!(transfer(&job, response(200, &[], b"bad"), None).is_err());
        assert_eq!(std::fs::read(job.destination).unwrap(), b"abcde");
    }
}
#[test]
fn resumes_only_a_valid_matching_entity() {
    let dir = TempDir::new().unwrap();
    let job = job(&dir);
    partial(&job);
    assert_eq!(
        transfer(
            &job,
            response(
                206,
                &[
                    ("content-range", "bytes 5-9/10"),
                    ("content-length", "5"),
                    ("etag", "\"v1\"")
                ],
                b"fghij"
            ),
            Some((5, "\"v1\""))
        )
        .unwrap(),
        Outcome::Completed
    );
    assert_eq!(std::fs::read(job.destination).unwrap(), b"abcdefghij");
}
#[test]
fn wrong_ranges_changed_etags_and_refused_ranges_preserve_partial() {
    for (status, headers) in [
        (200, vec![("etag", "\"v1\"")]),
        (416, vec![]),
        (
            206,
            vec![("etag", "\"v2\""), ("content-range", "bytes 5-9/10")],
        ),
        (
            206,
            vec![("etag", "\"v1\""), ("content-range", "bytes 4-9/10")],
        ),
        (
            206,
            vec![("etag", "\"v1\""), ("content-range", "bytes 5-9/11")],
        ),
        (
            206,
            vec![
                ("etag", "\"v1\""),
                ("content-range", "bytes 5-9/10"),
                ("content-length", "4"),
            ],
        ),
        (
            206,
            vec![("etag", "W/\"v1\""), ("content-range", "bytes 5-9/10")],
        ),
        (206, vec![("etag", "\"v1\"")]),
    ] {
        let dir = TempDir::new().unwrap();
        let job = job(&dir);
        partial(&job);
        assert!(
            transfer(
                &job,
                response(status, &headers, b"fghij"),
                Some((5, "\"v1\""))
            )
            .is_err()
        );
        assert_eq!(std::fs::read(job.part_path()).unwrap(), b"abcde");
        assert!(!job.destination.exists());
    }
}
#[test]
fn changed_redirect_target_is_rejected() {
    let dir = TempDir::new().unwrap();
    let job = job(&dir);
    partial(&job);
    let mut changed = response(
        206,
        &[("etag", "\"v1\""), ("content-range", "bytes 5-9/10")],
        b"fghij",
    );
    changed.effective_url = "https://example.test/other".into();
    assert!(transfer(&job, changed, Some((5, "\"v1\""))).is_err());
    assert_eq!(std::fs::read(job.part_path()).unwrap(), b"abcde");
}
#[test]
fn unsafe_content_encoding_and_insecure_redirects_are_rejected() {
    for encoding in [true, false] {
        let dir = TempDir::new().unwrap();
        let job = job(&dir);
        let mut source = response(200, &[], b"abcde");
        if encoding {
            source
                .headers
                .insert("content-encoding".into(), "gzip".into());
        } else {
            source.effective_url = "http://example.test/file".into();
        }
        assert!(transfer(&job, source, None).is_err());
        assert!(!job.destination.exists());
    }
}
#[test]
fn no_validator_never_appends_to_a_partial() {
    for etag in [None, Some("W/\"weak\"")] {
        let dir = TempDir::new().unwrap();
        let job = job(&dir);
        let mut headers = vec![("content-length", "10")];
        if let Some(etag) = etag {
            headers.push(("etag", etag));
        }
        assert!(transfer(&job, response(200, &headers, b"abcde"), None).is_err());
        let error = engine::transfer_with_source(
            &job,
            Arc::new(AtomicU8::new(0)),
            Arc::new(Bandwidth::default()),
            |_| {},
            |_, _| panic!("must not request unsafe resume"),
        )
        .unwrap_err();
        assert!(error.to_string().contains("strong ETag"));
    }
}
#[test]
fn truncates_uncommitted_tail_but_rejects_edited_prefix() {
    let dir = TempDir::new().unwrap();
    let job = job(&dir);
    partial(&job);
    std::fs::OpenOptions::new()
        .append(true)
        .open(job.part_path())
        .unwrap()
        .write_all(b"uncommitted")
        .unwrap();
    assert_eq!(
        transfer(
            &job,
            response(
                206,
                &[("etag", "\"v1\""), ("content-range", "bytes 5-9/10")],
                b"fghij"
            ),
            Some((5, "\"v1\""))
        )
        .unwrap(),
        Outcome::Completed
    );
    let dir = TempDir::new().unwrap();
    let job = self::job(&dir);
    partial(&job);
    std::fs::write(job.part_path(), b"xxxxx").unwrap();
    assert!(
        transfer(&job, response(200, &[], b""), None)
            .unwrap_err()
            .to_string()
            .contains("checksum changed")
    );
}
#[test]
fn complete_checkpoint_can_be_finalized_after_a_destination_collision() {
    let dir = TempDir::new().unwrap();
    let job = job(&dir);
    let result = engine::transfer_with_source(
        &job,
        Arc::new(AtomicU8::new(0)),
        Arc::new(Bandwidth::default()),
        |_| {},
        |_, _| {
            std::fs::write(&job.destination, b"existing").unwrap();
            Ok(response(200, &[], b"abcde"))
        },
    );
    assert!(result.is_err());
    assert_eq!(std::fs::read(&job.destination).unwrap(), b"existing");
    std::fs::remove_file(&job.destination).unwrap();
    let result = engine::transfer_with_source(
        &job,
        Arc::new(AtomicU8::new(0)),
        Arc::new(Bandwidth::default()),
        |_| {},
        |_, _| panic!("complete checkpoint needs no network"),
    );
    assert_eq!(result.unwrap(), Outcome::Completed);
    assert_eq!(std::fs::read(&job.destination).unwrap(), b"abcde");
}
struct BrokenReader {
    prefix: Cursor<Vec<u8>>,
}
impl Read for BrokenReader {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if self.prefix.position() == self.prefix.get_ref().len() as u64 {
            Err(io::Error::new(
                io::ErrorKind::ConnectionReset,
                "synthetic disconnect",
            ))
        } else {
            self.prefix.read(buffer)
        }
    }
}
#[test]
fn network_error_checkpoints_received_bytes() {
    let dir = TempDir::new().unwrap();
    let job = job(&dir);
    let mut source = response(200, &[("etag", "\"v1\""), ("content-length", "10")], b"");
    source.body = Box::new(BrokenReader {
        prefix: Cursor::new(b"abcde".to_vec()),
    });
    assert!(transfer(&job, source, None).is_err());
    let meta: serde_json::Value = storage::read_json(&job.meta_path(), 65536).unwrap();
    assert_eq!(meta["bytes"], 5);
}
#[test]
fn pause_and_cancel_are_honored_without_network() {
    for signal in [1, 2] {
        let dir = TempDir::new().unwrap();
        let job = job(&dir);
        let result = engine::transfer_with_source(
            &job,
            Arc::new(AtomicU8::new(signal)),
            Arc::new(Bandwidth::default()),
            |_| {},
            |_, _| panic!("must not start"),
        );
        assert_eq!(
            result.unwrap(),
            if signal == 1 {
                Outcome::Paused
            } else {
                Outcome::Canceled
            }
        );
        assert!(!job.destination.exists());
    }
}
