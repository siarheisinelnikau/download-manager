# DownloadManager v0.4.0

- Active downloads show a 30-second speed graph under the file and a smoothed speed and ETA.
- Parallel downloading is on by default with up to 32 HTTP connections per file. Settings can change the connection count or turn parallel downloading off. Small files use fewer connections. The downloader uses one connection when the server cannot supply validated byte ranges.
- Ubuntu amd64 has an installable package with an application launcher, folder picker and AppIndicator tray menu. Its interface runs on X11 or XWayland.
- The updater uses the shared rust-auto-updator 0.4.1 core for both the Windows executable and Ubuntu package. Ubuntu installation runs through apt and polkit while the window remains responsive.
- Release filenames follow the Space Sniffer convention. Use `DownloadManager-v0.4.0-windows-amd64.exe` and `DownloadManager-v0.4.0-ubuntu-amd64.deb` with tag `v0.4.0`. Windows v0.3.0 users need one manual executable replacement to pick up the new naming scheme.

## Verification

The workflow includes Windows and Ubuntu builds, format/lint checks and local HTTP fixtures. The Ubuntu job also captures a settings frame under Xvfb. On Ubuntu 26.04.1, 38 Rust tests passed. A live test verified 32 concurrent range requests, the speed graph, matching file SHA-256, continued transfer while hidden, and tray restoration and quit.
