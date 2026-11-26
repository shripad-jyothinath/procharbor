#!/bin/sh
set -eu

HERE=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
APP_ID=io.github.shripad-jyothinath.procharbor
ICON_DIR="$HOME/.local/share/icons/hicolor"
APPS_DIR="$HOME/.local/share/applications"
DESKTOP_DIR=$(xdg-user-dir DESKTOP 2>/dev/null || true)
BIN="$HERE/target/release/procharbor"

cargo build --release --manifest-path "$HERE/Cargo.toml"

mkdir -p "$ICON_DIR/scalable/apps" "$APPS_DIR"
cp "$HERE/assets/logo.svg" "$ICON_DIR/scalable/apps/$APP_ID.svg"
"$BIN" --export-icons "$ICON_DIR"

ENTRY="[Desktop Entry]
Type=Application
Version=1.0
Name=ProcHarbor
GenericName=Task Manager
Comment=Native Linux task manager and system monitor
Exec=$BIN
Path=$HERE
Icon=$APP_ID
Terminal=false
Categories=System;Monitor;
Keywords=task;manager;process;cpu;memory;performance;monitor;
StartupNotify=true
StartupWMClass=$APP_ID"

printf '%s\n' "$ENTRY" > "$APPS_DIR/$APP_ID.desktop"
chmod 644 "$APPS_DIR/$APP_ID.desktop"

if [ -n "${DESKTOP_DIR:-}" ] && [ -d "$DESKTOP_DIR" ] && [ "$DESKTOP_DIR" != "$HOME" ]; then
    printf '%s\n' "$ENTRY" > "$DESKTOP_DIR/ProcHarbor.desktop"
    chmod +x "$DESKTOP_DIR/ProcHarbor.desktop"
    gio set "$DESKTOP_DIR/ProcHarbor.desktop" metadata::trusted true 2>/dev/null || true
fi

update-desktop-database "$APPS_DIR" >/dev/null 2>&1 || true
if command -v gtk-update-icon-cache >/dev/null 2>&1; then
    gtk-update-icon-cache -f -t "$ICON_DIR" >/dev/null 2>&1 || true
fi

echo "Installed ProcHarbor icon and shortcuts."
