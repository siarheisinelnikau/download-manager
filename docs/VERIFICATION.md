# Verification record

Date: 2026-10-31 (Europe/Minsk). All work and synthetic application data stayed inside this repository. Nothing was published. The final design review uses application-generated images and native diagnostics, without Computer Use.

## Build and functional checks

- Rust 1.96.1, `x86_64-pc-windows-msvc`, two Cargo jobs and repository-local output.
- `./scripts/check.ps1 -Offline`: formatting, Clippy with warnings denied and 22 offline tests pass.
- Coverage includes conservative Range/If-Range resume, changed resources, short/error bodies, validator rejection, prefix hashes, checkpoint tails, pause/cancel, rate limiting, protected finalization, completion receipts, persistence, instance locking, update metadata and repeated add-error acknowledgements.
- The optimized portable build uses a static CRT. `inspect-package.ps1` confirms x64/Windows GUI, system DLL imports, no direct dynamic MSVC CRT dependency and no OpenGL/DirectX/Vulkan graphics import. The source/dependency graph confirms egui CPU rasterization through softbuffer's Win32 GDI BitBlt path.

## Design and actual screenshots

The consumer flow is now New download → link/name/folder → Download, with preserved input on recoverable errors. Pause/resume stays on each row; less frequent queue/restart/cancel actions are grouped under More. Settings is one page, with download defaults, useful limits, installed version and update controls. Save changes remains visible below the scrollable settings body at the minimum window size.

Reviewed PNGs from the packaged executable:

- `docs/screenshots/main-window.png`: 960×680, synthetic paused/error queue.
- `docs/screenshots/settings.png`: 960×680, isolated settings fixture.
- `.local/gui-smoke/design/empty.png`: empty queue.
- `.local/gui-smoke/design/settings-small.png`: minimum 760×540 settings layout.

All images show the real client area, rendered by the same UI and CPU/GDI backend as normal operation. Each invocation waits for application readiness and two actual rendered frames, writes the PNG atomically, requests normal shutdown and exits successfully. Internal readiness deadline: 5 seconds; external timeout: 8 seconds. No AI images, HTML mockups, desktop-wide captures, native-title-bar claims or live Internet transfer claims are involved. Fixtures use `example.test` URLs and disable startup update requests.

Visual iteration fixed low contrast, competing actions, floating/clipped settings, incorrectly unbounded progress bars, long-name overflow and the save action disappearing below small windows.

## Rendering and lifecycle evidence

The old build used OpenGL. A short warmed sample showed approximately 0% of one core in idle, which did not rule out the user's reported driver/presentation stutter. This is not presented as proof that CPU utilization caused the complaint.

The first CPU/GDI diagnostic caught unnecessary settings redraws: 222 layouts/rasters/presents over a 5-second nominal idle interval, about 10.6% of one core. Native viewport state was not synchronized with egui. Updating native focus/geometry/minimized information before taking input removed that failure in clean samples. Dedicated backend event-loop wakes also repaired a missed close-completion notification after capture. The host retains pixels for unchanged OS invalidations and skips scene construction/rasterization/presentation while hidden or minimized. There is no FPS cap or permanent redraw timer.

`./scripts/review-ui.ps1 -Diagnose` records separate layout, raster, present and native-event counts, plus process CPU in one-core units. It uses finite visible-settle, visible-idle, hidden, restore-settle and restored-idle phases. The disposable window is not activated and disables its own cursor hit-testing to avoid unrelated pointer movement contaminating idle samples. Normal application input is unchanged. Intervals containing native focus/resize/input changes are recorded separately from quiet idle intervals. The watchdog stops unexpected sustained scene work; this does not suppress normal application rendering.

See the committed runtime evidence in `docs/verification/` for the final binary's exact counts and CPU samples. Quiet visible intervals must have zero layouts/rasters; the genuinely hidden interval must have zero layouts/rasters/presents. A restore may produce a necessary fresh frame and must then settle again. These measurements do not certify actual pointer/keyboard latency or universal absence of perceived system-wide stutter; no GPU telemetry was available. Earlier automatic minimize tests were also exercised, but final reproducible diagnostics use explicit hide/restore to avoid unrelated native activation contaminating the hidden phase.

Final results for both the populated queue and settings: 5-second visible idle intervals and 5-second restored idle intervals recorded zero layouts/rasters/presents and 0% of one core. The 3-second hidden intervals also recorded zero work and 0% CPU. Restore presented the retained unchanged bitmap once. The diagnostic can label a native-activity interval and retry it once; the final evidence contains clean intervals for both pages.

Local asset: `dist/DownloadManager-v0.1.0.exe`, 7,336,448 bytes, SHA-256 `8debe8b2a980edb6bb4f230e291890aae593df1579a88922f4752a52902b021a`.

## Remaining checks and environment limits

- Outgoing requests to a synthetic loopback server still fail with Windows socket error 10013. The nine optional HTTP adapter tests compile but are not counted as passed. Run `./scripts/check.ps1 -Loopback` on a permitted developer PC or CI runner; actual HTTP/TLS behavior still needs validation there.
- Real keyboard traversal, Ctrl+V, pointer/scroll interaction, OS menus, broader DPI scaling and Windows 10/11 compatibility were not certified by screenshots. Computer Use is intentionally excluded from this phase.
- Validate disk-full/permission failures and abrupt termination on disposable data/volumes, plus actual update checking, download, replacement, recovery and restart against a test published release. No updater installation was executed.
- Online Cargo/DNS access and atomic moves inside the sandbox's virtual system temp were restricted. Dependencies resolved from cache; fixtures use the repository's ignored test folders, where atomic replacement passes. No system configuration was changed.
- Eight resolved crate archives lack bundled license texts; the generated dependency notices flag them. Complete upstream and Ubuntu-font notice review before public distribution.

The public-release gate remains incomplete. The local code, native screenshots and documented bounded diagnostics are independently reviewable.
