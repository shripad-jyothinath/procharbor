#!/bin/sh
set -eu

HERE=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
MANIFEST="$HERE/io.github.shripad_jyothinath.procharbor.yml"
BUILD_DIR="$HERE/.flatpak-build"
REPO_DIR="$HERE/.flatpak-repo"
OUT_DIR="$HERE/dist"

if ! command -v flatpak-builder >/dev/null 2>&1; then
    echo "flatpak-builder not found. Please install it with 'sudo apt install flatpak-builder'" >&2
    exit 1
fi

mkdir -p "$OUT_DIR"
echo "Building Flatpak application..."
flatpak-builder --force-clean --repo="$REPO_DIR" "$BUILD_DIR" "$MANIFEST"

echo "Creating single-file Flatpak bundle..."
flatpak build-bundle "$REPO_DIR" "$OUT_DIR/procharbor.flatpak" io.github.shripad_jyothinath.procharbor

echo "Flatpak bundle successfully generated at $OUT_DIR/procharbor.flatpak"
