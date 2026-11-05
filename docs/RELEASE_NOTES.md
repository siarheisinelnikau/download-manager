# DownloadManager v0.4.5

The Linux XEmbed tray menu uses the system GTK theme with its default spacing and shape. Left-click opens the compact download view, and right-click opens the menu beside the icon.

# DownloadManager v0.4.4

On GNOME with a compatible legacy XEmbed tray, the tray icon's background is clipped to the round mark. Left-click opens the compact download view and right-click opens the menu.

# DownloadManager v0.4.3

On Ubuntu with an XEmbed tray, right-click opens a menu using the system GTK theme. It aligns with the icon and opens inward from either panel edge. Left-click still toggles the compact download view.

# DownloadManager v0.4.2

The compact Linux tray window opens near the click or pointer and stays inside the corresponding monitor. The package also includes the separate left- and right-click actions from v0.4.1.

# DownloadManager v0.4.1

On Ubuntu desktops with X11 tray support, left-click the tray icon to open the compact download view and right-click to open the menu. On AppIndicator-only desktops, the menu keeps **Quick download view**.

# DownloadManager v0.4.0

- Active downloads show a 30-second speed graph, smoothed speed and ETA. The main queue and compact tray window animate progress between reports at roughly 60 frames per second.
- Parallel downloading is on by default with up to 32 HTTP connections per file. Settings can change the connection count or turn parallel downloading off. Small files use fewer connections. The downloader uses one connection when the server cannot supply validated byte ranges.
- Ubuntu amd64 has an installable package with an application launcher, folder picker, AppIndicator tray menu and compact download window. Its interface runs on X11 or XWayland.
- The updater uses the shared rust-auto-updator 0.4.1 core for both the Windows executable and Ubuntu package. Ubuntu installation runs through apt and polkit while the window remains responsive.
- Release filenames follow the Space Sniffer convention. Use `DownloadManager-v0.4.0-windows-amd64.exe` and `DownloadManager-v0.4.0-ubuntu-amd64.deb` with tag `v0.4.0`. Windows v0.3.0 users need one manual executable replacement to pick up the new naming scheme.

## Verification

The workflow includes Windows and Ubuntu builds, format/lint checks and local HTTP fixtures. The Ubuntu job also captures a settings frame under Xvfb. On Ubuntu 26.04.1, 39 Rust tests passed. A live test verified 32 concurrent range requests, the speed graph, matching file SHA-256, continued transfer while hidden, and tray restoration and quit.
