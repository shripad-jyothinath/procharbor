#!/bin/sh
set -eu

HERE=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
MATURIN=$(command -v maturin || echo "$HOME/.local/bin/maturin")

if [ ! -x "$MATURIN" ]; then
    echo "maturin not found. Please install it with 'pip install maturin' or 'cargo install maturin'" >&2
    exit 1
fi

echo "Building Python wheel and sdist with maturin..."
mkdir -p "$HERE/dist"
echo "Building Python wheel with maturin..."
"$MATURIN" build --release --out "$HERE/dist"

echo "Building Python source distribution (sdist) with maturin..."
"$MATURIN" sdist --out "$HERE/dist"

echo "Python packages generated in dist/:"
ls -lh "$HERE/dist"/*.whl "$HERE/dist"/*.tar.gz 2>/dev/null || true
