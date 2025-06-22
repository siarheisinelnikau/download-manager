# DownloadManager v0.1.0 — initial release draft

First implementation of an independent native Windows download manager. No preceding published release exists; this draft describes initial capabilities. Publication is not part of the current work.

## Added

- An English native interface for adding HTTP(S) URLs, choosing a destination and filename, managing a queue and viewing completed files.
- Pause/resume/cancel/retry, explicit restart from zero, queue reordering and persistent settings/history.
- Up to four concurrent files, a shared bandwidth limit, transfer progress, recent speed and ETA for known sizes.
- Flushed SHA-256 checkpoints and conservative Range/If-Range validation that refuses changed resources and unsafe server responses.
- No-overwrite completion and recovery receipts for interrupted finalization.
- Manual and optional nonblocking startup update checks against the canonical repository, with user-initiated SHA-256-verified portable update staging and a replacement/restart helper.
- Windows build/check scripts, a lockfile, offline fault tests, optional local-server tests and CI preparation.
- A focused queue and settings design with consistent contrast, a clear New download action, compact file rows and a save action that stays visible on small windows.
- CPU rasterization with Win32 GDI, retained repaint pixels, explicit backend wakes and native viewport synchronization to eliminate unnecessary idle/hidden redraws and missed close-completion notifications.
- Real application-generated README screenshots and bounded capture/runtime diagnostic commands.

## Platform and assets

Intended for Windows 10/11 x64. The portable MSVC build uses a static CRT and CPU/GDI rendering, without an application OpenGL/DirectX dependency. Planned single-architecture asset: `DownloadManager-v0.1.0.exe`, accompanied by `SHA256SUMS.txt`, MIT and dependency notices.

## Known limitations and validation

This is a local, unpublished implementation. Offline tests, native client-area screenshots and bounded idle/minimized/restore diagnostics are verified. Actual mouse/keyboard/clipboard interaction, real HTTP/TLS transport, broader platform compatibility and update installation/restart remain pending. Sandbox policy blocks outgoing loopback sockets. Screenshot fixtures use synthetic data; they do not demonstrate live Internet downloads.

Resume requires a strong ETag and a valid matching range response. Servers without a strong validator require an explicit restart after interruption. Each file uses a single connection; browser integration, authentication/cookies, scheduled transfers and segmented downloading are not included. Downloads never run automatically. There is no claim of universal acceleration or resume support.
