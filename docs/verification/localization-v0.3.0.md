# DownloadManager v0.3.0 localization verification

Prepared on 2026-11-01. This record covers the six-language update against the preceding local v0.2.0 build; earlier versioned records are retained.

## Implementation and catalog audit

- Embedded 155 complete messages in English, Russian, French, German, Spanish and Italian. Explicit lookup covers the queue, history, settings, add/restart dialogs, statuses, actions, tray popup/native menu, native folder-picker title/accept action and updater UI. File names, paths, URL samples, product names, unit/file-type badges and raw technical diagnostics are intentionally preserved.
- Settings provides adjacent Language (native names) and Theme (Dark/Light) dropdowns. Both preview immediately, Save changes persists the draft, and Back discards it. Missing or unknown locale values use English while retaining the previous theme and limits.
- Consumer-facing errors are translated; OS/network diagnostics remain available verbatim in details/hover surfaces. Template values are inserted once, so file names containing template-like braces are preserved.
- Catalog tests check all six selected-language mappings, nonempty/unique entries, complete columns and matching placeholders. Migration tests retain old configuration values and round-trip every language.
- Localized Browse captions determine their actual button width. The default settings viewport is 620×560 so all basic preferences and Updates fit; 560×440 remains supported through scrolling and a fixed Save footer. Long translated captions and the minimum main-window size were visually reviewed.

## Automated checks

- `cargo fmt --all -- --check`: passed.
- `cargo clippy --locked --offline --all-targets --features loopback-tests -- -D warnings`: passed.
- `cargo test --locked --offline --all-targets --features loopback-tests -- --test-threads=2`: all 38 tests passed, including real loopback HTTP fixtures and rejected unsafe resume responses.
- Repaired a Windows fixture defect: accepted sockets now explicitly leave the listener's nonblocking mode before synchronous request reads. The bounded nonblocking accept and read timeout are retained.
- `cargo build --locked --offline --release --target x86_64-pc-windows-msvc --bin download-manager` with `-C target-feature=+crt-static`: passed.
- `scripts/inspect-package.ps1` verified x64 GUI PE, file/product version 0.3.0, versioned original filename, static CRT, no GPU API imports, and actual Windows decoding of embedded icons at 16/20/24/32/48/64/256 pixels. This is packaged-resource evidence; OS tray interactions were not tested.

Compiler cache hard-link warnings were emitted by existing dependencies on this workspace filesystem; compilation, Clippy and tests succeeded.

## Real client-area captures

`scripts/review-localization.ps1` used disposable synthetic state, disabled startup update checks and rendered through the actual optimized EXE's CPU/GDI backend. Each invocation saved its own presented bitmap and exited successfully under an eight-second external timeout and five-second internal deadline. Every image below was opened and visually inspected. No Computer Use or injected OS input was used.

| Image | Physical dimensions | Finite frames | Median total frame time |
| --- | --- | --- | --- |
| main-window.png | 960 × 680 | 3 | 2.64 ms |
| main-window-light.png | 960 × 680 | 3 | 5.26 ms |
| settings.png | 620 × 560 | 3 | 3.37 ms |
| settings-light.png | 620 × 560 | 2 | 3.96 ms |
| main-window-ru.png | 960 × 680 | 3 | 2.03 ms |
| settings-de-light.png | 560 × 440 | 3 | 6.22 ms |
| new-download-fr-light.png | 960 × 680 | 3 | 6.78 ms |
| tray-popup-es.png | 400 × 372 | 3 | 4.92 ms |
| main-window-it-light.png | 760 × 540 | 2 | 5.10 ms |

Settings in both default themes show the Language, Theme and update controls together. Russian queue/error text, German minimum settings, French form actions, Spanish popup actions and Italian minimum queue remain readable. Minimum-height content intentionally scrolls; primary actions stay visible.

## Bounded rendering/load gate

Source inspection confirmed `egui_software_backend` CPU rasterization and softbuffer's Windows GDI presentation path, event-driven Wait/WaitUntil scheduling, hidden-window paint rejection, retained bitmap blits for OS redraws and repaint requests only on changed selections/data. Fresh versioned reports identify the actual executable and test process IDs.

`scripts/review-localization.ps1 -Diagnose` completed isolated Russian dark queue (PID 37900) and German light settings (PID 37580) sessions. Both exited successfully with `idle_hidden_render_gate=true`. CPU is measured as utilization of one logical core; settled samples were 0.000% at the process timer's resolution (also 0.000% when machine-normalized). GPU telemetry was unavailable; backend source, packaged imports, reported renderer and real raster/present counters establish the CPU/GDI path.

| View | State | Sample seconds | One-core CPU | Layout / raster / present counts |
| --- | --- | --- | --- | --- |
| Russian dark queue | visible_settle | 2.00 | 0.781% | 3 / 3 / 3 |
| Russian dark queue | visible_idle | 5.00 | 0.000% | 0 / 0 / 0 |
| Russian dark queue | hidden | 3.00 | 0.000% | 0 / 0 / 0 |
| Russian dark queue | restored_settle | 2.00 | 0.000% | 0 / 0 / 1 |
| Russian dark queue | restored_idle | 5.00 | 0.000% | 0 / 0 / 0 |
| German light settings | visible_settle | 2.00 | 0.000% | 3 / 3 / 3 |
| German light settings | visible_idle | 5.00 | 0.000% | 0 / 0 / 0 |
| German light settings | hidden | 3.00 | 0.000% | 0 / 0 / 0 |
| German light settings | restored_settle | 2.00 | 0.000% | 0 / 0 / 1 |
| German light settings | restored_idle | 5.00 | 0.000% | 0 / 0 / 0 |

Each restored window required one OS-requested blit of its retained bitmap during settling, with no new layout or rasterization. Visible and restored idle samples had zero layout/raster/presentation; hidden samples also had zero presentation. Locale/theme synchronization produced bounded startup repaint requests and then settled. Test process exit was confirmed.

## Artifact and remaining scope

Local optimized portable asset: `dist/DownloadManager-v0.3.0.exe` (8,459,264 bytes). SHA-256: `8d2a06edd566b4efca71d1dce280c7a7b262212ac215ff83e092d68848a7c3e4`. `dist/SHA256SUMS.txt`, license notices and release notes accompany it. CI-produced release bytes may differ with the runner/toolchain and must have their own checksum.

The static design review and bounded idle/hidden/restore checks are complete for the recorded scenarios. Actual tray mouse/menu gestures, native folder-dialog keyboard interaction, clipboard behavior, subjective interaction smoothness, disk-full/power-loss recovery and real update installation/restart remain manual checks. No normal user state, actual downloads, update installation, service, driver or system setting was changed. Native folder-dialog cancellation uses Windows' own system-language controls.

## Rust 1.99.1 CI compatibility preflight

The isolated Rust 1.99.1 toolchain was checked before publication without changing the default installed toolchain. All-target Clippy with `--features loopback-tests -- -D warnings` passed after replacing 22 implicit stroke-width float literals with explicit `f32` values in the theme and real UI components. Their previously inferred numeric values and rendering behavior are preserved; no lint suppressions were added. The icon tests also use fixed array chunks and `contains`, retaining the former ignored-remainder behavior.

`cargo +1.99.1 fmt --all -- --check` passed. `cargo +1.99.1 test --locked --offline --lib --bin download-manager -- --test-threads=2` passed all ten focused library/application unit tests, including catalog mappings and migration, template insertion, updater selection, icon decoding/legibility, tray placement and keyboard activation of the switch. The complete 38-test synthetic/loopback suite recorded above was run with the original local Rust 1.96.1 toolchain; GitHub CI runs the complete suite again with its stable toolchain before creating the release.

These compatibility edits do not change layout, redraw scheduling or consumer behavior. Existing visual/runtime evidence is retained; no repeated GUI captures or optimized rebuild were performed for explicit type annotations. The final downloadable executable and its checksum are generated and verified by the release CI job.
