#!/bin/sh
set -eu

HERE=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
BIN="$HERE/target/release/procharbor"

if [ ! -x "$BIN" ]; then
    cargo build --release --manifest-path "$HERE/Cargo.toml"
fi

exec "$BIN" "$@"
