# DownloadManager v0.3.0

This release adds six interface languages while preserving the download queue, resumable transfers and saved preferences from v0.2.0.

## Added

- English, Russian, French, German, Spanish and Italian interfaces. Choose a language by its native name in Settings → Appearance & behavior → Language, preview it immediately and select Save changes to keep it across restarts.
- Localized queue/history, tray popup and context menu, download forms, folder-dialog title/actions, recovery confirmations and update controls. Technical diagnostics remain available verbatim under error details.
- Embedded catalogs with coverage, placeholder-safety and older-settings migration checks. Existing settings without a language use English and retain their saved theme and download limits.

## Improved

- Keep the direct Dark/Light selector beside Language. Both palettes apply to the complete interface, including dialogs and the compact tray popup. Appearance changes preview immediately, and the back arrow discards unsaved changes.
- Use clear numeric simultaneous-download choices in every language, avoiding incorrect plural suffixes.

## Fixed

- Make Windows loopback test fixtures explicitly restore blocking mode on accepted sockets. The bounded nonblocking accept remains intact; synchronous request reads no longer fail intermittently with WSAEWOULDBLOCK.

## Downloads and verification

Windows 10/11 x64 portable asset: `DownloadManager-v0.3.0.exe`, with static CRT and the existing CPU rasterizer / Win32 GDI presentation path. Settings, queue, completed files and partial checkpoints stay in their existing locations.

Formatting, all-target Clippy with warnings denied, and all 38 synthetic/loopback tests passed during preparation. Fresh package and bounded client-area capture results are recorded in `docs/verification/localization-v0.3.0.md`. Actual tray menu gestures, native folder-dialog input, update installation and subjective interaction smoothness remain separate manual checks; no real update was installed during development.

Previous local build notes are preserved in `docs/verification/release-notes-v0.2.0.md`. Publication status is determined by GitHub Releases, not by this source document.
