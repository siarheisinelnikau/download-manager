# Architecture

## Execution model

The native egui UI builds controls and sends commands through a bounded channel (64 events). It reads a short-lived snapshot lock; file I/O, state loading/saving, network requests, checksum work and updater staging run outside normal UI callbacks. Add requests have distinct acknowledgements so repeated errors cannot close the form or strand its pending state.

One coordinator owns the queue, persistent state and the data-directory instance lock. It blocks on channel receive while idle. At most four transfer threads and one update worker are active. Transfer progress is published at approximately 250 ms intervals; redundant progress events can be dropped when the channel is full, while terminal results include the latest progress. Concurrency changes govern new work; already running transfers are allowed to finish or be paused explicitly.

The window uses egui CPU tessellation/rasterization through egui_software_backend 0.0.2 and softbuffer 0.4.8's Win32 GDI BitBlt presenter. OpenGL, glutin and wgpu are absent from the shipping dependency graph. Windows desktop composition remains an OS responsibility.

The host waits for native events and actual data changes. Backend notifications have their own event-loop wake, independent of egui's deduplicated repaint callback; this keeps close/save completion working even when no paint is pending. Native viewport focus, size and minimized state are synchronized before each input pass. Cached pixels service unchanged OS repaint requests without reconstructing the scene. Hidden/minimized windows retain dirty state but skip layout, rasterization and presentation. Restore schedules a fresh frame. There is no permanent paint timer, FPS throttle or Poll loop.

Developer-only `--capture-ui main|completed|settings|popup|new OUTPUT.png [WIDTH HEIGHT]` and `--diagnose-ui main|settings|popup|new|navigation|tray OUTPUT.json` require an explicit isolated data directory. Capture renders two ready frames, saves the bitmap atomically and exits. Diagnostics measure settled visible, hidden and restored states over finite phases; the disposable diagnostic window is not activated and disables its own cursor hit-testing to prevent unrelated pointer motion contaminating idle samples. Native-activity intervals are labeled separately; one bounded retry can obtain a clean idle sample. Both initial and restored quiet intervals are required. Normal input behavior is unchanged. These modes do not verify keyboard/mouse interaction or universal absence of perceived stutter.

## Tray and window lifecycle

A Windows notification icon and native menu dispatch commands through the UI event-loop proxy, including while the scene is hidden. Single-click toggles the compact unfinished-download popup; double-click and Show downloads restore the full queue. The popup and full interface reuse the same native window and coordinator. Queue geometry is retained across popup and compact-settings use; popup placement is clamped to the monitor work area and recalculated after DPI/size changes. Focus loss and Escape dismiss the popup. Tooltip/menu state changes follow displayed state, not byte progress; there is no tray polling timer.

Close hides by default, controlled by persisted `close_to_tray`. Quit sends a stop command, pauses waiting tasks and signals all transfer workers. The coordinator persists their results before allowing exit, including when hidden. A disk-write failure restores the full window and defers exit. Callback guards are disabled before destroying the owned tray host. If initial Shell registration fails, the native builder returns an error; the regular window displays the failure and close exits. The vendored tray dependency patch and its provenance are described in `vendor/README.md`.

A per-data-directory Windows named mutex selects one UI instance. A second launch grants foreground permission to its recorded PID and signals a named activation event. One blocking wait thread forwards activation through the same UI dispatcher; it is signaled and joined during shutdown. No localhost server or activation polling is used. The coordinator's file lock remains the persistence boundary. The tray diagnostic exercises the real window/controller, a separate activation process and hidden Quit; Shell registration/cleanup are checked separately, and an unavailable tray produces an explicit diagnostic failure.

The native tray bitmap is rasterized at the Win32 small-icon metric, with no fixed 32px downscale. One code-native icon produces the RGBA window/tray texture and the multiresolution ICO embedded by the Windows resource compiler. Packaging asks Windows to decode all seven sizes from the final EXE as data, without executing it.

OS termination/power loss is handled by durable checkpoints rather than a promise of orderly cleanup.

## Appearance

The 0.2.0 interface shares semantic colors, a restrained blue accent, spacing, rounded controls, file-type icons, status badges and focus outlines. Installed Segoe UI regular/semibold fonts are loaded once at startup with bundled egui fallbacks; system fonts are not redistributed. Custom switches publish checkbox state and support keyboard activation. File rows keep Pause/Resume/Retry beside the filename; recoverable errors retain details and explicit restart confirmation. Completed rows open only the containing folder after a user action.

`Settings.theme` persists the explicit Dark/Light choice and defaults to Dark for new and older state files. One semantic palette in `src/theme.rs` supplies surfaces, text, borders, selection, status colors, progress tracks and primary actions for every application view. A settings draft previews immediately and is persisted through the existing Save changes action; leaving settings restores the saved choice. The native window receives the matching theme request.

Settings use three compact groups and one Dark/Light dropdown, with related controls aligned in short rows. All normal options fit at the preferred size; long errors and update progress scroll in the content area. The Save changes button has its own reserved footer bounds so error text cannot displace it. The host changes the shared window to 620×500 (minimum 560×440), retaining queue position, size and maximization for the return path. A tray-originated Settings command restores normal geometry and positions this view near the icon, first moving onto the target monitor and then accounting for its DPI, native frame and work area. Opening Settings from the queue keeps the queue's local position. Fixed-screen captures preserve an explicitly requested size. Theme and geometry changes each schedule bounded work only on a real transition; there is no theme polling or animation timer. The theme-aware review script uses isolated fixtures for both palettes and can explicitly skip the separately blocked Shell tray test.

The popup reserves a fixed footer rectangle using the actual button text height and padding. Its list is a separately clipped scroll area, so list sizing cannot push the button or frame beyond the native client area. Empty popups use 400×256 logical pixels; the two-file fixture uses 400×372. Self-captures follow the same content-dependent size unless explicitly constrained, record measured footer bounds/bottom inset and reject clipping.

## Folder selection

Both destination editors share an editable path field and Browse action. After painting, the host dispatches at most one picker request to a dedicated STA thread. It uses the real window owner and Windows IFileOpenDialog with folder/filesystem/existing-path options; downloads and the native event loop continue independently. COM objects and Shell strings are released on that thread. Selection, cancellation or error returns through an event-loop proxy, including while the main window is hidden, without a polling timer.

Results carry the target editor and its opening generation. A result for a closed or reopened editor is ignored. Cancellation preserves input; a successful result changes only that editor's path. Settings still require Save changes. Native errors leave manual path entry available. The developer-only `--probe-folder-picker FIXTURE_FOLDER OUTPUT.json` mode requires an isolated data directory and verifies native configuration and initial-folder round trip without showing a picker.

## File protocol

`Download.destination` is an absolute path under an existing canonicalized directory. Reserved Windows filenames, path components, alternate-stream syntax, control characters and managed filenames are rejected. The UI never derives a writable path from Content-Disposition or a redirect. Duplicate destinations in queue/history are rejected case-insensitively.

The per-task part file is exclusively locked while active. Its checkpoint records source and effective URLs, strong ETag, optional total length, committed byte count, prefix SHA-256 and completion state. Each checkpoint follows `write -> sync_all(part) -> write and sync temporary JSON -> atomic replace JSON`. On resume, the engine verifies the committed prefix and truncates only bytes beyond it. A failed checkpoint leaves the previous one available; a shorter/edited prefix requires explicit restart.

The resume response must describe exactly `bytes OFFSET-(TOTAL-1)/TOTAL`, with an unchanged strong ETag, redirect target and any previously known total. An absent Content-Length is allowed when Content-Range provides the total. Fresh responses require HTTP 200; ranged responses require HTTP 206. Multipart/partial segments are not requested. Bodies must use identity content encoding. TLS validation is never disabled.

On completion, the part file and its final checkpoint are flushed before a no-replacement move. A complete checkpoint remains as a receipt until the coordinator durably records completion. After a crash, a matching destination can be verified against this receipt and recovered without downloading again. A name collision preserves both the existing destination and the complete part; after resolving the collision, retry can finalize that part without a request.

Disk-full/permission errors propagate with context and preserve the previous durable checkpoint where the OS permits. No user document is deleted by cancellation. Restart from zero is a separate explicit UI confirmation that removes only the task's managed part/checkpoint. There is no automatic execution of normal downloads.

## Persistence and bounds

`state.json` has a versioned schema, atomic replacement and a 16 MiB load limit. Unknown schemas, corrupt JSON, duplicate/invalid IDs and invalid stored paths fail closed without overwriting the original. Queue/history is limited to 10,000 items. All unfinished queue states reopen paused. Startup update checking is persisted separately from transfer state and defaults to enabled for a new state.

The shared bandwidth limiter reserves delivery time across all active workers and uses interruptible short waits. It limits application delivery/writes, not precise wire traffic: OS/client buffering and headers can cause small bursts. Memory is proportional to bounded stream buffers plus the bounded queue/history. No parallel per-file segments are implemented.

`engine::Response` is the streaming transport boundary. Production uses ureq; offline tests inject deterministic bodies, faults and headers. This validates engine decisions independently of sockets and does not prove production HTTP/TLS adapter behavior.

## Update boundary

The installed app uses public HTTPS GitHub APIs, independent of any Codex plugin. Stable releases are parsed with semver; drafts/prereleases and incompatible assets are rejected. Download URLs must belong to the canonical repository's release path. HTTPS-only redirects, a 256 MiB asset limit, published byte count, SHA-256 and x64 PE-header checks protect staging. The digest is supplied over the same trusted GitHub channel; this is not independent code signing.

Only a user action launches installation. The sibling `rust-auto-updator` core owns release parsing, HTTPS download, checksum/digest agreement, complete PE/architecture verification, staging, Windows atomic replacement/rollback and cleanup. The product adapter supplies repository/version, the 256 MiB limit and staging names. Snapshot clones share one prepared payload; no duplicate updater implementation remains in this repository. Queue persistence and worker shutdown finish before a successful handoff permits exit. The new helper CLI is dispatched before instance/UI side effects; cleanup waits for successful native host initialization.

For migration, older DownloadManager versions pass target/staged/hash/PID to the newly downloaded helper. The adapter validates its product-specific data-directory and helper filename; the shared core verifies the actual helper and payload and imports this request into the modern installer. It never treats old flags as an ordinary GUI launch. The app data directory is not migrated or removed. A real end-to-end updater run, Authenticode signing and an externally published release are outside this session's verified scope.

## Source references

- [HTTP Range and If-Range semantics (RFC 9110)](https://www.rfc-editor.org/rfc/rfc9110.html#section-14)
- [ureq 3.5 streaming and timeout API](https://docs.rs/ureq/3.5.0/ureq/)
- [egui 0.33.3](https://docs.rs/egui/0.33.3/egui/)
- [softbuffer 0.4.8](https://docs.rs/softbuffer/0.4.8/softbuffer/)

