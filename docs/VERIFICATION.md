# Verification record

## v0.4.0 Ubuntu verification

On Ubuntu 26.04.1 amd64 with Rust and Cargo 1.99.1, the locked release build passed formatting, Clippy with warnings denied and all 38 Rust tests. The Ubuntu packaging script produced an amd64 Debian package with version 0.4.0.

A real GNOME/XWayland session ran the executable extracted from that package. The GNOME AppIndicator host registered the application's menu. Resuming a fixture through the menu sent 32 simultaneous HTTP range requests. The window showed smoothed speed and ETA and rendered the [active speed chart](screenshots/ubuntu-speed-chart.png). The transfer continued after the window closed to the tray and the downloaded 32 MiB matched the fixture SHA-256. The tray menu restored the same window and quit the application. The package was also installed through polkit using a local 0.3.99 predecessor build to prepare the live update check.

The release workflow checks and builds on Windows and Ubuntu 22.04. It verifies package metadata and SHA-256 before uploading both packages, the notices and a checksum manifest to a draft. The release becomes public once GitHub reports the expected SHA-256 for every uploaded asset.

## Artifact and checks

`dist/DownloadManager-v0.2.0.exe`, 8,395,264 bytes. SHA-256: `86ef5b52a6c107373139d343cd22d41f37049fffb35e07eb87b19fa2a9b843b6`.

- Optimized Windows x64 MSVC, static CRT, two Cargo jobs, repository-local `.local/ui-target` output to avoid a concurrently used build directory.
- All **26** current-checkout offline tests pass, including the real egui switch's focused keyboard activation and disabled-state behavior. Formatting and all-target Clippy with `-D warnings` pass. The earlier concurrent audit's temporary UI lint failure is resolved.
- Final package inspection checks FileVersion, ProductVersion and OriginalFilename against 0.2.0, static-CRT imports and the absence of application OpenGL/DirectX/Vulkan rendering imports.
- The source, pinned dependency path and native diagnostics establish egui CPU rasterization and softbuffer Win32 GDI BitBlt presentation. Windows composition remains an OS responsibility.
- Windows decodes all seven embedded icon sizes: 16/20/24/32/48/64/256. `application-icon-v0.2.0.png/json` records an offscreen LoadImageW/DrawIconEx resource proof from this exact executable; this is separate from OS tray/taskbar interaction.

## Interface review

Reviewed the actual application's queue, settings, new-download form, compact popup, empty popup, empty queue and completed files in both themes. README embeds all current views. The queue/form are 960×680, settings 620×500, populated popup 400×372 and empty popup 400×256 logical pixels. Constrained queue/form 760×540, settings 560×440 and popup 360×220 were also inspected. Smaller lists/settings scroll inside their content bounds while primary footers remain visible.

The redesign uses shared semantic colors, installed Segoe UI regular/semibold with bundled fallbacks, consistent spacing and controls, compact file rows and adjacent actions. Errors display a reason and recovery options; completed files open their containing folder only after a user action. Both folder fields retain Browse and manual entry. The saved Dark/Light preference remains intact; Dark is the default.

A numerical contrast check covered text/background, secondary text/surface, accent/badge, error/error-surface, success/success-surface and white/primary pairs in both themes. The minimum was **5.21:1**. This is a token-pair check, not a certification of every platform/accessibility state.

Captures use the real packaged UI and renderer, manager readiness and two requested-view frames, atomic PNG completion, a five-second internal deadline and eight-second external timeout. Each owned capture exited. They establish client-area appearance rather than native chrome, actual keyboard traversal, clipboard, menus or dialogs. Synthetic fixtures contain paused/error/completed entries, not live transfers, and disable startup update requests. New installations enable startup checks by default.

All six popup footer cases (populated/empty/constrained, both themes) remain inside content with a **17-point** bottom inset. See `popup-footer-v0.2.0.json` and `ui-review.json` for artifact identity, sizes and evidence scope.

## Runtime and lifecycle

Ten bounded diagnostics cover queue, settings, populated popup, empty popup and new-download form in both themes. Each passed settled visible, hidden and restored idle checks. Every quiet sample recorded **0 layouts, 0 rasters and 0 presentations**; the maximum measured CPU was **0.0% of one core**. Samples use five seconds per visible idle interval and three seconds hidden, with finite settle phases and at most one retry for native activity. Restore can service an exposure with retained pixels before returning to idle.

The additional navigation diagnostic starts with a minimized queue, opens settings near a synthetic tray-side anchor on the actual monitor work area, hides it and returns to the original queue using the same window. The entire settings outer frame fits the work area; the queue's original outer bounds are restored within two pixels. This checks the application's controller and geometry, not actual Shell menu clicks or mixed-monitor DPI transitions.

There is no persistent animation, repaint or polling timer. Hidden/minimized scenes skip layout/raster/presentation. GPU/compositor telemetry and perceived input latency were not measured; quiet counters do not certify universal absence of system-wide stutter.

## Remaining boundaries

- The earlier Shell tray registration diagnostic was rejected with Windows error 5. Real tray clicks, native menu activation, taskbar/Alt-Tab appearance and mixed-monitor DPI remain interactive checks.
- Native Browse selection/cancellation, actual OS keyboard/clipboard navigation and Open folder activation were not exercised. The switch unit test injects input into the real egui component without controlling Windows.
- Earlier optional loopback HTTP adapter requests were blocked with socket error 10013. Those nine optional tests are not counted as passed. Offline engine/fault tests do not prove real HTTP/TLS transport.
- Update download/replacement/recovery/restart, broader Windows compatibility, power loss and real disk-full/permission scenarios remain separate checks. Source builds require the sibling updater checkout documented in README; no updater was run.

Earlier artifact/runtime records and the shared-updater audit are preserved in [the historical verification record](verification/verification-before-v0.2.0.md). Its old screenshot names and interim artifact identities describe those earlier builds; the current README/manifest above identify 0.2.0. Nothing was published.
