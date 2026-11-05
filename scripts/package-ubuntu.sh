#!/usr/bin/env bash
set -euo pipefail
umask 022
cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.."

if [[ $(dpkg --print-architecture) != amd64 ]]; then
    echo 'Packaging requires an amd64 Debian/Ubuntu build host.' >&2
    exit 1
fi

target=x86_64-unknown-linux-gnu
export CARGO_TARGET_DIR="$PWD/target"
cargo build --locked --release --target "$target" --bin download-manager -j 2
binary="$CARGO_TARGET_DIR/$target/release/download-manager"
stage=$(mktemp -d "$CARGO_TARGET_DIR/ubuntu-package.XXXXXX")
trap 'rm -rf -- "$stage"' EXIT
package="$stage/root"

# Resolve release metadata and license notices from the same locked build.
cargo metadata --locked --format-version 1 --filter-platform "$target" > "$stage/metadata.json"
version=$(python3 - "$stage/metadata.json" "$stage/notices.txt" <<'PY'
import json
import re
import sys
from pathlib import Path

packages = json.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))["packages"]
project = next(package for package in packages if package["name"] == "download-manager")
assert re.fullmatch(r"\d+\.\d+\.\d+", project["version"]), "Expected a stable package version"
print(project["version"])
with Path(sys.argv[2]).open("w", encoding="utf-8") as output:
    print("DownloadManager third-party notices", file=output)
    for package in sorted(packages, key=lambda p: (p["name"], p["version"])):
        if package == project:
            continue
        print(f'\n===== {package["name"]} {package["version"]} =====', file=output)
        print(f'License: {package["license"]} · Source: {package["repository"]}', file=output)
        root = Path(package["manifest_path"]).parent
        licenses = {
            path for path in root.rglob("*")
            if path.is_file() and re.match(
                r"^(LICENSE|LICENCE|COPYING|NOTICE|UNLICENSE|OFL|UFL)([._-]|$)",
                path.name, re.IGNORECASE,
            )
        }
        if package["license_file"]:
            licenses.add(root / package["license_file"])
        if package["name"] == "rust-auto-updator":
            licenses.add(root.parents[1] / "LICENSE")
        for license_path in sorted(licenses):
            print(f"--- {license_path.name} ---", file=output)
            print(license_path.read_text(encoding="utf-8", errors="replace"), file=output)
PY
)

install -Dm755 "$binary" "$package/usr/bin/download-manager"
doc="$package/usr/share/doc/download-manager"
install -Dm644 LICENSE "$doc/copyright"
install -Dm644 "$stage/notices.txt" "$doc/THIRD_PARTY_NOTICES.txt"
install -Dm644 docs/RELEASE_NOTES.md "$doc/RELEASE_NOTES.md"

extension=download-manager@siarheisinelnikau
extension_dir="$package/usr/share/gnome-shell/extensions/$extension"
install -Dm644 "gnome-extension/$extension/metadata.json" "$extension_dir/metadata.json"
install -Dm644 "gnome-extension/$extension/extension.js" "$extension_dir/extension.js"

icon_dir="$package/usr/share/icons/hicolor/64x64/apps"
install -d -m755 "$package/DEBIAN" "$package/usr/share/applications" "$icon_dir" dist
"$binary" --export-icon "$icon_dir/download-manager.png"
cat > "$package/usr/share/applications/download-manager.desktop" <<'DESKTOP'
[Desktop Entry]
Type=Application
Name=DownloadManager
Comment=Queue and resume HTTP downloads
Exec=download-manager
Icon=download-manager
Terminal=false
Categories=Network;FileTransfer;
DESKTOP
chmod 644 "$package/usr/share/applications/download-manager.desktop" "$icon_dir/download-manager.png"
cat > "$package/DEBIAN/control" <<CONTROL
Package: download-manager
Version: $version
Section: net
Priority: optional
Architecture: amd64
Maintainer: DownloadManager contributors <9218241+siarheisinelnikau@users.noreply.github.com>
Depends: libc6 (>= 2.35), libgtk-3-0t64 | libgtk-3-0, libssl3t64 | libssl3, ca-certificates, libx11-6, libxext6, libxcb1, libxcb-shm0, libxkbcommon0, libxkbcommon-x11-0, libayatana-appindicator3-1, libglib2.0-bin, xdg-utils, zenity, pkexec | policykit-1, apt, dpkg
Description: Native HTTP download manager
 Queue and resume downloads with verified byte ranges, live speed graphs
 and desktop tray controls.
CONTROL
chmod 644 "$package/DEBIAN/control"
asset="DownloadManager-v$version-ubuntu-amd64.deb"
dpkg-deb --root-owner-group --build "$package" "$stage/$asset"
[[ $(dpkg-deb -f "$stage/$asset" Package) == download-manager ]]
[[ $(dpkg-deb -f "$stage/$asset" Version) == "$version" ]]
[[ $(dpkg-deb -f "$stage/$asset" Architecture) == amd64 ]]
mv -- "$stage/$asset" "dist/$asset"
(cd dist && sha256sum -- "$asset" > "$asset.sha256")
printf 'Prepared dist/%s\n' "$asset"
