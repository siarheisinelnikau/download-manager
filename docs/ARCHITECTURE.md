# Architecture

## Execution model

The native egui UI builds controls and sends commands through a bounded channel (64 events). It reads a short-lived snapshot lock; file I/O, state loading/saving, network requests, checksum work and updater staging run outside normal UI callbacks. Add requests have distinct acknowledgements so repeated errors cannot close the form or strand its pending state.

One coordinator owns the queue, persistent state and the data-directory instance lock. It blocks on channel receive while idle. At most four transfer threads and one update worker are active. Transfer progress is published at approximately 250 ms intervals; redundant progress events can be dropped when the channel is full, while terminal results include the latest progress. Concurrency changes govern new work; already running transfers are allowed to finish or be paused explicitly.

The window uses egui CPU tessellation/rasterization through egui_software_backend 0.0.2 and softbuffer 0.4.8's Win32 GDI BitBlt presenter. OpenGL, glutin and wgpu are absent from the shipping dependency graph. Windows desktop composition remains an OS responsibility.

The host waits for native events and actual data changes. Backend notifications have their own event-loop wake, independent of egui's deduplicated repaint callback; this keeps close/save completion working even when no paint is pending. Native viewport focus, size and minimized state are synchronized before each input pass. Cached pixels service unchanged OS repaint requests without reconstructing the scene. Hidden/minimized windows retain dirty state but skip layout, rasterization and presentation. Restore schedules a fresh frame. There is no permanent paint timer, FPS throttle or Poll loop.

Developer-only `--capture-ui main|settings OUTPUT.png [WIDTH HEIGHT]` and `--diagnose-ui main|settings OUTPUT.json` require an explicit isolated data directory. Capture renders two ready frames, saves the bitmap atomically and exits. Diagnostics measure settled visible, hidden and restored states over finite phases; the disposable diagnostic window is not activated and disables its own cursor hit-testing to prevent unrelated pointer motion contaminating idle samples. Native-activity intervals are labeled separately; one bounded retry can obtain a clean idle sample. Normal input behavior is unchanged. These modes do not verify keyboard/mouse interaction or universal absence of perceived stutter.

Closing sends a stop command, pauses waiting tasks and signals all transfer workers. The coordinator persists their results before allowing the window to close. A disk-write failure is visible and defers exit. OS termination/power loss is handled by durable checkpoints rather than a promise of orderly cleanup.

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

Only a user action launches installation. A verified copy of the new executable runs in helper mode, waits for the parent process, copies the payload to a sibling staging file, verifies it again, backs up the existing EXE and replaces it atomically. Replacement/relaunch failures are logged for the next launch. The app data directory is not migrated or removed. A real end-to-end updater run, Authenticode signing and an externally published release are outside the verified scope of this session.

## Source references

- [HTTP Range and If-Range semantics (RFC 9110)](https://www.rfc-editor.org/rfc/rfc9110.html#section-14)
- [ureq 3.5 streaming and timeout API](https://docs.rs/ureq/3.5.0/ureq/)
- [egui 0.33.3](https://docs.rs/egui/0.33.3/egui/)
- [softbuffer 0.4.8](https://docs.rs/softbuffer/0.4.8/softbuffer/)

