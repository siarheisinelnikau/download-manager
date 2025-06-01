# Verification record

Development date: 2026-10-31 (Europe/Minsk). Repository-only unattended session. No application GUI, updater, screenshots, real user downloads, remote Git operations or publication were run.

## Completed

- Rust 1.96.1, host `x86_64-pc-windows-msvc`; Cargo uses two jobs and repository-local output.
- `cargo check --offline --all-targets`: passed, including the native UI, Windows file API calls and updater helper code.
- `cargo clippy --offline --all-targets -- -D warnings`: passed. Sandbox incremental-cache hard-link warnings can occur; Cargo falls back to copying.
- `./scripts/check.ps1 -Offline`: passed formatting, Clippy with warnings denied, and all **21** offline tests on the final source.
- `cargo test --offline --locked --features loopback-tests --test downloads --no-run`: passed compilation of the nine optional HTTP transport tests without executing them.
- `cargo clippy --offline --locked --features loopback-tests --all-targets -- -D warnings`: passed including the optional socket-test source.
- `./scripts/package.ps1 -Offline -SkipChecks`: built the optimized Windows x64 portable EXE with a static CRT and generated its SHA-256, release notes and dependency notices.
- `./scripts/inspect-package.ps1`: confirmed an x64 PE32+ Windows GUI executable, with system DLL imports and no direct dynamic MSVC CRT dependency. No GUI process was started. Clean-machine deployment remains a manual check.
- Final local artifact: `dist/DownloadManager-v0.1.0.exe`, 7,574,016 bytes. SHA-256: `25458cee3f19b07e8c0703bc9fc47633a27d77401deab081c5b30811f58f8a0f`.
- Offline engine fixtures cover known/unknown length, interrupted body checkpointing, exact Range/If-Range, changed ETag/redirect/length, missing or weak validators, invalid/missing ranges, HTTP 200/416 on resume, encoding/downgrade rejection, edited prefix, uncommitted tail, pause/cancel, shared limiter delay, destination collision and completion-receipt recovery.
- State/coordinator tests cover atomic state replacement, paused restart, startup-check opt-out, corrupt/future state preservation, filename/URL rules, worker pause signaling, second-instance lock, durable settings and orderly shutdown.
- Updater tests cover canonical URL selection, exact SHA-256 entry matching, malformed/duplicate hashes, semver comparison and stable-channel filtering. No update request/install was executed.

## Environment limits observed

- Online Cargo dependency resolution failed while initializing DNS (`getaddrinfo() thread failed to start`). The existing cache was sufficient for the pinned eframe 0.33.3 build and lockfile. No elevation or alternate authentication was attempted.
- Loopback server tests could bind a local listener, but outgoing requests failed with Windows socket error **10013** (access forbidden). Those tests were not counted as passed. They are behind `loopback-tests`; run `./scripts/check.ps1 -Loopback` on an ordinary developer PC or CI runner.
- Native atomic moves inside the sandbox's virtual system temp directory failed with access denied. Artificial test directories are now explicitly under this repository's ignored `.test-output`, where the same atomic operations pass. No system temp configuration was changed.
- The generated dependency notice file flags **13** resolved crate source archives with declared licenses but no bundled license text. The font crate's available OFL/emoji notices are copied recursively; complete the upstream/Ubuntu font notice audit before any public distribution. This is recorded as pending, not claimed as a completed license audit.

## Required manual review before a public release

1. Run the packaged EXE on Windows 10 and 11 x64. Exercise Ctrl+V, add/download controls, settings persistence, scrolling, DPI scaling, resize/minimize/restore and close while transferring. Capture real application and settings screenshots for README.
2. Run the optional loopback suite. Confirm real HTTP redirects, chunked/unknown bodies, ignored/invalid ranges, changing ETags and abrupt disconnect/retry through the actual ureq adapter. Validate HTTPS, certificate/hostname failures and blocked HTTPS-to-HTTP redirects against isolated test endpoints.
3. Verify multi-file queue ordering/concurrency and bandwidth changes with small controlled files. Measure idle visible/minimized CPU and input/render latency; compilation does not prove responsiveness.
4. Validate permission-denied and disk-full handling on a disposable volume, process termination at checkpoint/finalization boundaries and recovery after restart. Do not fill a real user/system volume.
5. After the canonical repository and a test stable release exist, validate manual/startup check behavior, disabled-startup network silence, rate-limit/404 errors, checksum/size mismatch, successful user-initiated install, non-writable app directory, interrupted staging, backup recovery and relaunch with unchanged data. Do not run replacement against a production install during initial validation.
6. Review dependency notices and complete any missing notices before publication. Inspect the packaged PE imports/static-CRT deployment on a clean machine, consider code signing and verify the published executable/checksum names together.

No public release, universal resume support, speed gains, successful graphical smoke test or successful updater lifecycle is claimed by this record.
