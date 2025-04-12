# DownloadManager

Repository: `download-manager`
Priority: 2 of 15 (user-requested launch order)
Product: DownloadManager
Purpose: Reliable queued and resumable downloads
Inspiration: https://www.internetdownloadmanager.com/

Independent open-source implementation; no affiliation, competitor code or copied assets.

## Requested implementation

Implement a consumer-friendly native download manager: add/paste HTTP(S) URLs, choose destination, queue, pause/resume/cancel/retry, progress/speed/ETA, bandwidth and concurrency controls, and persistence across restarts. Reliable resume is central: validate Range responses, entity validators/If-Range and changing remote content, preserve partial files safely, finalize atomically, and handle missing length, redirects, TLS failures, interrupted transfers and disk-full errors. Only use parallel segments when supported safely by the server.

Use local loopback test servers and synthetic files for functional tests rather than large internet downloads. Never auto-execute downloaded programs. Browser integration is a later optional component, not a prerequisite for a usable app. Do not claim universal speed increases or universal resume support. Include clear per-download errors and recovery, a completed-download list, settings and a release-ready build script.

## Packaging and publication target

Windows standalone asset: `DownloadManager-vVERSION.exe` (for the initial release, `DownloadManager-v0.1.0.exe`). Append architecture only when distributing multiple executable architectures in the same release.
Planned canonical repository: https://github.com/siarheisinelnikau/download-manager (not yet created or published; keep availability/error reporting truthful).

## Working agreement

Follow AGENTS.md. Complete as much real implementation and safe noninteractive verification as possible in this unattended code-only session. Manual UI, device and hardware tests will happen later. Record required external components and their verification status. Keep changes committed locally in meaningful increments.

