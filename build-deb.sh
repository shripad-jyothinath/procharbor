#!/bin/sh
set -eu
umask 022

HERE=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
APP_ID=io.github.shripad-jyothinath.procharbor
PKG=procharbor
VERSION=1.2.0
MAINTAINER="Shripad <shripad-jyothinath@users.noreply.github.com>"
HOMEPAGE="https://github.com/shripad-jyothinath/procharbor"
OUT="$HERE/dist"

rm -rf "$OUT"
mkdir -p "$OUT"

for ARCH in amd64 arm64; do
    ROOT=$(mktemp -d)
    chmod 755 "$ROOT"
    trap 'rm -rf "$ROOT"' EXIT

    mkdir -p "$ROOT/DEBIAN" \
             "$ROOT/usr/bin" \
             "$ROOT/usr/share/$PKG/assets" \
             "$ROOT/usr/share/applications" \
             "$ROOT/usr/share/icons/hicolor/scalable/apps" \
             "$ROOT/usr/share/doc/$PKG"

    install -m 644 "$HERE/procharbor.py" "$ROOT/usr/share/$PKG/procharbor.py"
    install -m 644 "$HERE/assets/logo.svg" "$ROOT/usr/share/$PKG/assets/logo.svg"
    if [ -f "$HERE/assets/screenshot.png" ]; then
        install -m 644 "$HERE/assets/screenshot.png" "$ROOT/usr/share/$PKG/assets/screenshot.png"
    fi

    cat > "$ROOT/usr/bin/$PKG" <<EOF
#!/bin/sh
exec python3 /usr/share/$PKG/procharbor.py "\$@"
EOF
    chmod 755 "$ROOT/usr/bin/$PKG"

    cat > "$ROOT/usr/share/applications/$APP_ID.desktop" <<EOF
[Desktop Entry]
Type=Application
Version=1.0
Name=ProcHarbor
GenericName=Task Manager
Comment=Native GTK4 task manager and system monitor for Linux
Exec=/usr/bin/$PKG
Icon=$APP_ID
Terminal=false
Categories=System;Monitor;
Keywords=task;manager;process;cpu;memory;performance;monitor;
StartupNotify=true
StartupWMClass=$APP_ID
EOF

    install -m 644 "$HERE/assets/logo.svg" "$ROOT/usr/share/icons/hicolor/scalable/apps/$APP_ID.svg"
    python3 - "$HERE/assets/logo.svg" "$ROOT/usr/share/icons/hicolor" "$APP_ID" <<'PY'
import os
import sys

import gi
gi.require_version("GdkPixbuf", "2.0")
from gi.repository import GdkPixbuf

src, icon_root, app_id = sys.argv[1:4]
for size in (32, 48, 64, 128, 256, 512):
    dest = os.path.join(icon_root, f"{size}x{size}", "apps")
    os.makedirs(dest, exist_ok=True)
    pb = GdkPixbuf.Pixbuf.new_from_file_at_scale(src, size, size, True)
    pb.savev(os.path.join(dest, f"{app_id}.png"), "png", [], [])
PY

    cat > "$ROOT/DEBIAN/control" <<EOF
Package: $PKG
Version: $VERSION
Section: utils
Priority: optional
Architecture: $ARCH
Maintainer: $MAINTAINER
Depends: python3 (>= 3.8), python3-gi, python3-gi-cairo, python3-cairo, gir1.2-gtk-4.0, librsvg2-common, hicolor-icon-theme, desktop-file-utils
Recommends: iproute2, wireless-tools, pciutils
Homepage: $HOMEPAGE
Description: Native GTK4 task manager and system monitor
 ProcHarbor is a Linux task manager and system monitor built with GTK4.
 It shows processes grouped by app with CPU, memory, disk, network and GPU
 usage, live performance charts, per-process GPU and network accounting,
 user autostart entries, users, and systemd services.
EOF

    cat > "$ROOT/DEBIAN/postinst" <<'EOF'
#!/bin/sh
set -e
if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database -q /usr/share/applications >/dev/null 2>&1 || true
fi
if command -v gtk-update-icon-cache >/dev/null 2>&1; then
    gtk-update-icon-cache -f -t /usr/share/icons/hicolor >/dev/null 2>&1 || true
fi
exit 0
EOF
    chmod 755 "$ROOT/DEBIAN/postinst"

    cat > "$ROOT/usr/share/doc/$PKG/copyright" <<EOF
Format: https://www.debian.org/doc/packaging-manuals/copyright-format/1.0/
Upstream-Name: $PKG
Source: $HOMEPAGE

Files: *
Copyright: 2026 Shripad
License: MIT
 Permission is hereby granted, free of charge, to any person obtaining a copy
 of this software and associated documentation files (the "Software"), to deal
 in the Software without restriction, including without limitation the rights
 to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
 copies of the Software, and to permit persons to whom the Software is
 furnished to do so, subject to the following conditions:
 .
 The above copyright notice and this permission notice shall be included in all
 copies or substantial portions of the Software.
 .
 THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
 IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
 FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
 AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
 LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
 OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
 SOFTWARE.
EOF

    dpkg-deb --root-owner-group --build "$ROOT" "$OUT/${PKG}_${VERSION}_${ARCH}.deb" >/dev/null
    rm -rf "$ROOT"
    trap - EXIT
    echo "built dist/${PKG}_${VERSION}_${ARCH}.deb"
done

( cd "$OUT" && sha256sum *.deb > SHA256SUMS )
cat "$OUT/SHA256SUMS"
