# DownloadManager v0.2.0 — local build notes

This UI redesign replaces the previously shared 0.1.3 build. It is local and unpublished.

## Changed since the shared 0.1.3 build

- Rebuild the queue with compact file rows, file-type icons, status badges, aligned progress details and Pause/Resume/Retry actions beside each filename. Error reasons and recovery options stay with the affected file.
- Introduce shared charcoal and light-gray palettes with one blue accent, consistent controls and spacing, native Segoe UI regular/semibold typography and visible keyboard focus. Dark remains the default and existing theme preferences are preserved.
- Group download settings, appearance/behavior and updates into one compact page. Basic options fit at 620×500; smaller windows scroll while Save changes stays visible independently of error text.
- Redesign the new-download form with clear fields, Browse, a close cross and Enter submission from its text fields. Opening it returns the underlying list to Queue.
- Give the empty queue a direct Add a download action and completed files an explicit Open folder action. Downloaded files are never executed.
- Keep the tray popup focused on quick actions; reduce its empty size to 400×256 and its two-file size to 400×372 while preserving the measured footer inset.
- Refresh README screenshots from the real packaged application in both themes, including empty and completed states, and add bounded completed-view capture support.

## Current build and validation

Intended Windows 10/11 x64 portable asset: `DownloadManager-v0.2.0.exe`, with a static CRT and CPU/GDI rendering. Source builds currently require the sibling shared updater checkout documented in README. No release was published and no update installation was executed.

All 26 current-checkout offline tests, formatting and all-target Clippy with warnings denied passed. Native self-captures cover both themes and constrained sizes. Bounded visible/hidden/restore reports are described in VERIFICATION.md; real tray clicks, Windows input/dialog navigation and updater installation remain separate interactive checks. Screenshots contain synthetic fixtures.

Previous shared build: 0.1.3. Its notes are preserved in `docs/verification/release-notes-v0.1.3.md`.
