# DownloadManager v0.1.1 — local build notes

This build replaces the previously shared 0.1.0 development builds. No published release exists; publication is not part of the current work.

## Changed since the shared 0.1.0 build

- The application, Windows file/product properties and executable filename now identify this build as 0.1.1, so successive delivered builds can be distinguished.
- Packaging, inspection and screenshot scripts resolve the executable name from Cargo metadata. They no longer launch a fixed 0.1.0 filename.
- Packaging rejects a Windows resource version that differs from the project version. Future changed executables receive a new version before delivery; ordinary test compilations keep their current version.

## Initial application capabilities

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
- A compact tray download popup with per-file pause/resume/retry, a native context menu, optional keep-running-on-close behavior and duplicate-launch activation of the existing instance.
- A consistent original application icon embedded in the EXE at six sizes and used by the window, interface and tray. Packaging verifies that Windows can decode the final icon resources.
- Explicit handling of rejected tray registration, keeping the regular window accessible instead of hiding behind an unavailable icon.
- Dark and light themes shared by the full queue, settings, dialogs and compact popup. Dark is the default for new and older settings; the selected theme previews immediately and persists after Save changes.

## Platform and assets

Intended for Windows 10/11 x64. The portable MSVC build uses a static CRT and CPU/GDI rendering, without an application OpenGL/DirectX dependency. Single-architecture asset: `DownloadManager-v0.1.1.exe`, accompanied by `SHA256SUMS.txt`, MIT and dependency notices.

## Known limitations and validation

This is a local, unpublished implementation. All 27 offline tests, native client-area screenshots, packaged icon decoding and bounded rendering diagnostics are verified. The earlier native tray test was rejected with Windows error 5; actual Shell placement, tray cleanup and mouse/menu gestures need validation on an unrestricted desktop. Actual keyboard/clipboard interaction, real HTTP/TLS transport, broader platform compatibility and update installation/restart remain pending. Sandbox policy blocks outgoing loopback sockets. Screenshot fixtures use synthetic data; they do not demonstrate live Internet downloads.

Resume requires a strong ETag and a valid matching range response. Servers without a strong validator require an explicit restart after interruption. Each file uses a single connection; browser integration, authentication/cookies, scheduled transfers and segmented downloading are not included. Downloads never run automatically. There is no claim of universal acceleration or resume support.
