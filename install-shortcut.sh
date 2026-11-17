#!/bin/sh
set -eu

HERE=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
APP_ID=io.github.shripad-jyothinath.procharbor
ICON_DIR="$HOME/.local/share/icons/hicolor"
APPS_DIR="$HOME/.local/share/applications"
DESKTOP_DIR=$(xdg-user-dir DESKTOP 2>/dev/null || true)

mkdir -p "$ICON_DIR/scalable/apps" "$APPS_DIR"
cp "$HERE/assets/logo.svg" "$ICON_DIR/scalable/apps/$APP_ID.svg"

python3 - "$HERE/assets/logo.svg" "$ICON_DIR" "$APP_ID" <<'PY'
import os
import sys

import gi
gi.require_version("GdkPixbuf", "2.0")
from gi.repository import GdkPixbuf

src, icon_dir, app_id = sys.argv[1:4]
for size in (32, 48, 64, 128, 256, 512):
    dest_dir = os.path.join(icon_dir, f"{size}x{size}", "apps")
    os.makedirs(dest_dir, exist_ok=True)
    pb = GdkPixbuf.Pixbuf.new_from_file_at_scale(src, size, size, True)
    pb.savev(os.path.join(dest_dir, f"{app_id}.png"), "png", [], [])
PY

ENTRY="[Desktop Entry]
Type=Application
Version=1.0
Name=ProcHarbor
GenericName=Task Manager
Comment=Native GTK4 task manager and system monitor for Linux
Exec=$HERE/run.sh
Path=$HERE
Icon=$APP_ID
Terminal=false
Categories=System;Monitor;
Keywords=task;manager;process;cpu;memory;performance;monitor;
StartupNotify=true
StartupWMClass=$APP_ID"

printf '%s\n' "$ENTRY" > "$APPS_DIR/$APP_ID.desktop"
chmod 644 "$APPS_DIR/$APP_ID.desktop"

if [ -n "${DESKTOP_DIR:-}" ] && [ -d "$DESKTOP_DIR" ]; then
    printf '%s\n' "$ENTRY" > "$DESKTOP_DIR/ProcHarbor.desktop"
    chmod +x "$DESKTOP_DIR/ProcHarbor.desktop"
    gio set "$DESKTOP_DIR/ProcHarbor.desktop" metadata::trusted true 2>/dev/null || true
fi

update-desktop-database "$APPS_DIR" >/dev/null 2>&1 || true
if command -v gtk-update-icon-cache >/dev/null 2>&1; then
    gtk-update-icon-cache -f -t "$ICON_DIR" >/dev/null 2>&1 || true
fi

echo "Installed ProcHarbor icon and shortcuts."
