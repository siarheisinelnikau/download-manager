# Verification record

Date: 2026-11-01 (Europe/Minsk). All work and synthetic application data stayed inside this repository. Nothing was published. The final design review uses application-generated images and native diagnostics, without Computer Use. Source milestones: `a42319c` and `4f0a0ae`.

## Build and functional checks

- Rust 1.96.1, `x86_64-pc-windows-msvc`, two Cargo jobs and repository-local output.
- `./scripts/check.ps1 -Offline`: formatting, Clippy with warnings denied and 26 offline tests pass. The sandbox's incremental-cache hard-link warning falls back to copying; it is not a source warning or test failure.
- Coverage includes conservative Range/If-Range resume, changed resources, short/error bodies, validator rejection, prefix hashes, checkpoint tails, pause/cancel, rate limiting, protected finalization, completion receipts, persistence (including the close-to-tray preference), instance locking, update metadata, repeated add-error acknowledgements, popup placement, stable tray tooltips and Windows icon decoding.
- The optimized portable build uses a static CRT. `inspect-package.ps1` confirms x64/Windows GUI, system DLL imports, no direct dynamic MSVC CRT dependency and no OpenGL/DirectX/Vulkan graphics import. The source/dependency graph confirms egui CPU rasterization through softbuffer's Win32 GDI BitBlt path.
- Windows `LoadLibraryExW` (resource-data mode) and `LoadImageW` decode icon group 1 in the final packaged EXE at 16, 24, 32, 48, 64 and 256 pixels. This does not execute the EXE or depend on Explorer's icon cache.

## Design and actual screenshots

The consumer flow is now New download → link/name/folder → Download, with preserved input on recoverable errors. Pause/resume stays on each row; less frequent queue/restart/cancel actions are grouped under More. Settings is one page, with download defaults, useful limits, installed version and update controls. Save changes remains visible below the scrollable settings body at the minimum window size.

Reviewed PNGs from the packaged executable:

- `docs/screenshots/main-window.png`: 960×680, synthetic paused/error queue.
- `docs/screenshots/settings.png`: 960×680, isolated settings fixture.
- `docs/screenshots/tray-popup.png`: 400×480, the real compact download interface with synthetic paused/error rows.
- `docs/screenshots/application-icon.png`: 64×64 export from the shared code-native icon source.
- `.local/gui-smoke/design/empty.png`: empty queue.
- `.local/gui-smoke/design/settings-small.png`: minimum 760×540 settings layout.

All images show the real client area, rendered by the same UI and CPU/GDI backend as normal operation. Each invocation waits for application readiness and two actual rendered frames, writes the PNG atomically, requests normal shutdown and exits successfully. Internal readiness deadline: 5 seconds; external timeout: 8 seconds. No AI images, HTML mockups, desktop-wide captures, native-title-bar claims or live Internet transfer claims are involved. Fixtures use `example.test` URLs and disable startup update requests.

Visual iteration fixed low contrast, competing actions, floating/clipped settings, incorrectly unbounded progress bars, long-name overflow and the save action disappearing below small windows. The new popup keeps common per-file actions and the full-queue/settings entry points visible, with the same palette, typography and original icon as the main interface.

## Rendering and lifecycle evidence

The old build used OpenGL. A short warmed sample showed approximately 0% of one core in idle, which did not rule out the user's reported driver/presentation stutter. This is not presented as proof that CPU utilization caused the complaint.

The first CPU/GDI diagnostic caught unnecessary settings redraws: 222 layouts/rasters/presents over a 5-second nominal idle interval, about 10.6% of one core. Native viewport state was not synchronized with egui. Updating native focus/geometry/minimized information before taking input removed that failure in clean samples. Dedicated backend event-loop wakes also repaired a missed close-completion notification after capture. The host retains pixels for unchanged OS invalidations and skips scene construction/rasterization/presentation while hidden or minimized. There is no FPS cap or permanent redraw timer.

`./scripts/review-ui.ps1 -Diagnose` records separate layout, raster, present and native-event counts, plus process CPU in one-core units. It uses finite visible-settle, visible-idle, hidden, restore-settle and restored-idle phases. The disposable window is not activated and disables its own cursor hit-testing to avoid unrelated pointer movement contaminating idle samples. Normal application input is unchanged. Intervals containing native focus/resize/input changes are recorded separately from quiet idle intervals. The watchdog stops unexpected sustained scene work; this does not suppress normal application rendering.

See the committed runtime evidence in `docs/verification/` for the final binary's exact counts and CPU samples. Quiet visible intervals must have zero layouts/rasters; the genuinely hidden interval must have zero layouts/rasters/presents. A restore may produce a necessary fresh frame and must then settle again. These measurements do not certify actual pointer/keyboard latency or universal absence of perceived system-wide stutter; no GPU telemetry was available. Earlier automatic minimize tests were also exercised, but final reproducible diagnostics use explicit hide/restore to avoid unrelated native activation contaminating the hidden phase.

The final evidence includes queue, settings and popup measurements. Their five-second visible and restored idle intervals record zero layouts/rasters/presents and 0% of one core; three-second hidden intervals also record zero work and CPU. Restore presents the retained bitmap once. Quiet visible, hidden and restored intervals must pass independently; native-activity intervals remain in the record and are not relabeled as idle. The gate now requires both initial and restored quiet samples, rather than accepting only one of them.

Local asset: `dist/DownloadManager-v0.1.0.exe`, 8,032,256 bytes, SHA-256 `76c30a7abcbaef5255a2463a800c0ed22f612334b74ca65302fb1e55a3f75805`.

## Native tray boundary

Source review found that the pinned tray library omitted the required `NOTIFYICONDATAW.cbSize` in registration/modification/removal. A repository-local patch fills it, returns registration errors instead of claiming success, and includes the native error text in removal diagnostics. No shared registry sources were edited.

Even with valid parameters, this sandbox rejects `Shell_NotifyIconW(NIM_ADD)` with **Access is denied (Windows error 5)**. The final application displays that error in the regular window, keeps the tray absent and makes close exit normally. The bounded tray diagnostic still exercises the common native popup/hide/restore dispatcher, a separate duplicate-activation process and hidden Quit, while recording the unavailable Shell boundary. During this test, the diagnostic does not show the hidden window before activation: the second process must cause restoration through the named-event listener. The initial/final window IDs must match. Its process exits with a diagnostic error; the review script does not count it as a passed tray test. See `docs/verification/runtime-tray.json` for the exact partial evidence. Actual Shell placement, icon removal after successful registration and real single/double/right-click menu interactions remain unverified.

The popup PNG verifies the application's rendered popup contents only. It does not demonstrate an icon registered with Explorer or a real tray mouse gesture. No elevation, policy change, Computer Use or authentication workaround was attempted.

## Remaining checks and environment limits

- Outgoing requests to a synthetic loopback server still fail with Windows socket error 10013. The nine optional HTTP adapter tests compile but are not counted as passed. Run `./scripts/check.ps1 -Loopback` on a permitted developer PC or CI runner; actual HTTP/TLS behavior still needs validation there.
- Real keyboard traversal, Ctrl+V, pointer/scroll interaction, OS menus, broader DPI scaling and Windows 10/11 compatibility were not certified by screenshots. Computer Use is intentionally excluded from this phase.
- Validate disk-full/permission failures and abrupt termination on disposable data/volumes, plus actual update checking, download, replacement, recovery and restart against a test published release. No updater installation was executed.
- Online Cargo/DNS access and atomic moves inside the sandbox's virtual system temp were restricted. Dependencies resolved from cache; fixtures use the repository's ignored test folders, where atomic replacement passes. No system configuration was changed.
- Eight resolved crate archives lack bundled license texts; the generated dependency notices flag them. Complete upstream and Ubuntu-font notice review before public distribution.

The public-release gate remains incomplete. The local code, native screenshots and documented bounded diagnostics are independently reviewable.
