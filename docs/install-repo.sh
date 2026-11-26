#!/usr/bin/env bash
set -euo pipefail

if [ "$(id -u)" -ne 0 ]; then
    echo "Error: This script must be run as root (e.g. curl ... | sudo bash)." >&2
    exit 1
fi

KEYRING_DIR="/etc/apt/keyrings"
KEYRING_FILE="$KEYRING_DIR/procharbor.gpg"
SOURCES_FILE="/etc/apt/sources.list.d/procharbor.list"
REPO_URL="https://shripad-jyothinath.github.io/procharbor"
KEY_URL="$REPO_URL/procharbor-archive-keyring.gpg"

echo "Configuring ProcHarbor APT repository..."

mkdir -p "$KEYRING_DIR"
chmod 755 "$KEYRING_DIR"

if command -v curl >/dev/null 2>&1; then
    curl -fsSL "$KEY_URL" -o "$KEYRING_FILE"
elif command -v wget >/dev/null 2>&1; then
    wget -qO "$KEYRING_FILE" "$KEY_URL"
else
    echo "Error: neither curl nor wget found. Please install curl or wget." >&2
    exit 1
fi

chmod 644 "$KEYRING_FILE"

ARCH=$(dpkg --print-architecture 2>/dev/null || echo "amd64")

cat << EOF > "$SOURCES_FILE"
deb [arch=$ARCH signed-by=$KEYRING_FILE] $REPO_URL/debian stable main
EOF

chmod 644 "$SOURCES_FILE"

echo "Updating APT package lists..."
apt-get update

echo ""
echo "ProcHarbor APT repository added successfully!"
echo "You can now install ProcHarbor with:"
echo "    sudo apt install procharbor"
