# Verification record

Date: 2026-11-01 (Europe/Minsk). Current local build: **0.1.3**. Footer and tray-side placement fixes are in `a52598f` and `9b176c4`. Folder browsing was implemented in `3ff9957`; compact settings and popup changes are in `081ae31`. Work, fixtures and output stayed inside this repository. Nothing was published. Computer Use was not used.

## Current artifact and checks

`dist/DownloadManager-v0.1.3.exe`, 8,096,768 bytes. SHA-256: `2466ce8da6998b30519d7bd5e4376cba221746843a52a9075ba53c6a8bad8b7b`.

- Rust 1.96.1, Windows x64 MSVC, two Cargo jobs and repository-local target output.
- All 27 offline tests pass, including persistence, conservative resume/fault cases, protected finalization, updater metadata, native icon decoding and older-settings migration to Dark. Formatting and Clippy with warnings denied pass. The sandbox's incremental-cache hard-link warning falls back to copying.
- Package inspection checks FileVersion, ProductVersion and OriginalFilename against Cargo metadata. All identify 0.1.3; an intentionally mismatched expected version was rejected during the preceding packaging work.
- The portable build has a static CRT. Imports are system DLLs without an application OpenGL/DirectX/Vulkan renderer. The implementation uses egui_software_backend CPU rasterization and softbuffer Win32 GDI BitBlt presentation; Windows desktop composition remains an OS responsibility.
- Windows loads the final EXE as resource data and decodes icon group 1 at 16, 24, 32, 48, 64 and 256 pixels.

## Interface and real screenshots

The popup keeps New download, per-file actions, Open downloads and a close cross with an Escape tooltip. The footer is reserved before laying out the clipped scroll body, using its actual styled button height. Settings and More remain outside this popup. The native tray context menu retains Settings, Pause all, Resume paused and Quit.

Settings now use a 620×500 client area, a single theme dropdown, compact related rows and a fixed Save action. At the 560×440 minimum, a visible scrollbar exposes the remaining settings. Returning to Downloads restores the queue's previous size, position and maximization. Both folder fields retain manual entry and add Browse. A settings folder selection is a draft until Save changes.

Reviewed native client-area images from the current EXE:

| View | Dark image | Light image | Logical size |
| --- | --- | --- | --- |
| Queue | `main-window.png` | `main-window-light.png` | 960×680 |
| Settings | `settings.png` | `settings-light.png` | 620×500 |
| Compact popup | `tray-popup.png` | `tray-popup-light.png` | 400×432 |
| Empty popup | `tray-popup-empty.png` | `tray-popup-empty-light.png` | 400×272 |
| New download | `new-download.png` | `new-download-light.png` | 960×680, with the actual modal |

Images live in `docs/screenshots/`. Empty and populated popup captures, plus constrained 360×220 captures in both themes, measure the real footer response inside the content bounds with a 17-point bottom inset. The capture script rejects clipped footers; minimum-size popups scroll their body instead of displacing the button. Empty queues and minimum-size settings were also inspected under `.local/gui-smoke/design/dark/` and `light/`. The screenshot fixtures contain synthetic paused/error jobs and disable startup update requests; they do not demonstrate Internet transfers. New installations enable startup checks by default.

Capture waits for manager readiness and opening the requested view before counting two complete frames, writes a PNG atomically and exits. Internal deadline: 5 seconds; external timeout: 8 seconds. This fixed an observed readiness race that could capture an empty queue before the new-download modal's sizing pass. Captures prove the rendered client area, not OS menus, mouse navigation or native title bars.

For the theme control and grouped rows, the design review consulted the [Windows App mode dropdown](https://learn.microsoft.com/en-us/windows-app/display-settings?tabs=windows) and [Microsoft settings guidance](https://learn.microsoft.com/en-us/windows/apps/design/app-settings/guidelines-for-app-settings). The requested close-cross behavior was inspected in the local Multi-Monitor Brightness popup. No sibling project was modified.

## Bounded runtime measurements

The event loop waits for native events and changed data. Unchanged OS invalidations use retained pixels; hidden/minimized windows skip layout, rasterization and presentation. Backend and folder-picker completion dispatch independently of painting. There is no permanent repaint, FPS or folder-selection polling timer.

The 0.1.3 runtime reports cover empty and populated popups in both themes, plus native navigation. The preceding 0.1.2 settings/form reports retain their original revision and hash. Diagnostics use finite settle, five-second visible idle, three-second hidden, restore-settle and five-second restored-idle phases. Native input/focus/resize intervals are labeled separately; at most one retry per idle phase is allowed. A disposable diagnostic window is not activated and disables only its own cursor hit-testing. Normal application input is unchanged.

The 0.1.3 navigation diagnostic starts with the real queue, minimizes it, restores Settings beside a synthetic tray-side anchor on the actual monitor work area, hides it and returns to the original queue. It records actual outer bounds including the native frame, verifies the right/bottom work-area insets and restores the queue position and size on the same window ID. This does not certify actual Shell menu clicks or mixed-monitor DPI transitions. Runtime JSON files include actual logical window sizes, visibility, minimized state and separate layout/raster/present/native-event counters.

One Light popup run was not a valid restored-idle sample: Windows additionally minimized the window to 160×28 and restored it during both candidate intervals. Its failed gate is retained as `runtime-popup-v0.1.2-light-native-activity.json`; it is not counted as a pass. Only the interrupted Light popup and remaining Light form were repeated. A separate capture attempt conflicted with an active diagnostic's data-directory lock; it exited at its bounded deadline and was repeated sequentially after the diagnostic exited.

Current records are `runtime-popup-empty/populated-v0.1.3[-light].json` and `runtime-navigation-v0.1.3.json` under `docs/verification/`. They are separate from the successful 0.1.2 settings/popup/form records. Earlier runtime records retain their original version, revision and hash. These samples do not certify real input latency or universal absence of perceived system-wide stutter; GPU telemetry was unavailable.

All four current popup combinations pass with zero layouts, rasters and presentations and measured 0% one-core CPU in their quiet visible, hidden and restored samples. The navigation run also passes from a measured minimized starting state, with the entire settings frame inside the work area and the original queue bounds restored. `popup-footer-v0.1.3.json` records the six measured footer layouts independently of the runtime samples.

## Native folder-picker boundary

In the preceding 0.1.2 verification, two bounded probes created/configured the production Windows IFileOpenDialog and round-tripped the initial filesystem folder, including Cyrillic letters and spaces. They did not call Show. `folder-picker-probe.json` records this component proof separately from screenshots.

Source uses one STA worker, the real owner HWND, folder/filesystem/existing-path flags and unchanged-current-directory behavior. COM objects and Shell strings are released on the worker. Completion returns through the event-loop proxy. Cancellation preserves the path; a stale result cannot overwrite a closed/reopened editor. Errors keep manual path entry available.

Actual Browse clicks, confirmation, cancellation, owner-window modality and native-dialog navigation remain unverified interactive paths. No real user folder was selected or changed during testing.

## Remaining limits

- The preceding Shell tray test (`4f0a0ae`, retained in `runtime-tray.json`) was rejected with Windows error 5. Actual tray placement, cleanup after successful registration and real single/double/right-click interactions remain unverified. The UI and navigation diagnostics do not count as a passed Shell tray test.
- Outgoing synthetic loopback requests fail with Windows socket error 10013. The nine optional HTTP adapter tests compile but are not counted as passed; run `check.ps1 -Loopback` on a permitted developer PC/CI runner. Actual HTTP/TLS behavior still needs validation there.
- Keyboard/clipboard interaction, broader DPI/platform compatibility, disk-full/permission failures, abrupt termination and a real update download/replacement/recovery/restart remain separate checks. No updater installation was executed.
- Eight resolved crate archives lack bundled license texts and are flagged by generated notices. Complete upstream/font notice review before public distribution.

The public-release gate remains incomplete. No elevation, driver/system-setting changes, authentication workaround or publication was attempted.
