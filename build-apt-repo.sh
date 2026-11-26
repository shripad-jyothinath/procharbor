#!/usr/bin/env bash
set -euo pipefail

HERE=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
cd "$HERE"

KEY_ID="${APT_KEY_ID:-ProcHarbor Archive Automatic Signing Key}"
DOCS_DIR="$HERE/docs"
DEBIAN_DIR="$DOCS_DIR/debian"
DIST_DIR="$HERE/dist"

# Ensure debian package is built
if [ ! -d "$DIST_DIR" ] || ! compgen -G "$DIST_DIR/*.deb" >/dev/null; then
    echo "No .deb files found in dist/. Running ./build-deb.sh..."
    "$HERE/build-deb.sh"
fi

echo "Setting up APT repository in docs/debian/..."
mkdir -p "$DEBIAN_DIR/pool/main/p/procharbor"
mkdir -p "$DEBIAN_DIR/dists/stable/main/binary-amd64"

# Copy deb files to pool
cp "$DIST_DIR"/*.deb "$DEBIAN_DIR/pool/main/p/procharbor/"

# If arm64 deb exists, create binary-arm64 dir as well
ARCHES="amd64"
if compgen -G "$DEBIAN_DIR/pool/main/p/procharbor/*arm64.deb" >/dev/null; then
    mkdir -p "$DEBIAN_DIR/dists/stable/main/binary-arm64"
    ARCHES="amd64 arm64"
fi

cd "$DEBIAN_DIR"

# Generate Packages & Packages.gz for amd64
echo "Generating Packages index..."
apt-ftparchive --arch amd64 packages pool/main > dists/stable/main/binary-amd64/Packages
gzip -9cn dists/stable/main/binary-amd64/Packages > dists/stable/main/binary-amd64/Packages.gz

if [ -d "dists/stable/main/binary-arm64" ]; then
    apt-ftparchive --arch arm64 packages pool/main > dists/stable/main/binary-arm64/Packages
    gzip -9cn dists/stable/main/binary-arm64/Packages > dists/stable/main/binary-arm64/Packages.gz
fi

# Generate Release
echo "Generating Release manifest..."
CONF=$(mktemp)
cat << EOF > "$CONF"
APT::FTPArchive::Release::Origin "ProcHarbor";
APT::FTPArchive::Release::Label "ProcHarbor";
APT::FTPArchive::Release::Suite "stable";
APT::FTPArchive::Release::Codename "stable";
APT::FTPArchive::Release::Architectures "$ARCHES";
APT::FTPArchive::Release::Components "main";
APT::FTPArchive::Release::Description "ProcHarbor Official APT Repository";
EOF

cd "$DEBIAN_DIR/dists/stable"
apt-ftparchive -c "$CONF" release . > Release
rm -f "$CONF"

# Sign Release
echo "Signing Release with GPG ($KEY_ID)..."
gpg --batch --yes --clearsign -u "$KEY_ID" -o InRelease Release
gpg --batch --yes -abs -u "$KEY_ID" -o Release.gpg Release

# Export public keyring
echo "Exporting public keys to docs/..."
gpg --armor --export "$KEY_ID" > "$DOCS_DIR/procharbor-archive-keyring.asc"
gpg --export "$KEY_ID" > "$DOCS_DIR/procharbor-archive-keyring.gpg"

echo "APT repository successfully generated in $DEBIAN_DIR!"
