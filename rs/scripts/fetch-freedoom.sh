#!/usr/bin/env bash
# Downloads Freedoom (free game data for the Doom engine, BSD-licensed) and verifies it, into
# crates/grimoire-doom/freedoom/: freedoom1.wad plus its license. The packaging scripts and CI run this; the
# files aren't committed because they're large.
set -euo pipefail
cd "$(dirname "$0")/.."

VERSION=0.13.0
SHA256=3f9b264f3e3ce503b4fb7f6bdcb1f419d93c7b546f4df3e874dd878db9688f59
DEST=crates/grimoire-doom/freedoom
[[ -f "$DEST/freedoom1.wad" && -f "$DEST/COPYING.txt" ]] && { echo "Freedoom $VERSION already present"; exit 0; }

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
curl -fsSL -o "$TMP/freedoom.zip" "https://github.com/freedoom/freedoom/releases/download/v$VERSION/freedoom-$VERSION.zip"
if command -v shasum >/dev/null; then ACTUAL="$(shasum -a 256 "$TMP/freedoom.zip" | cut -d' ' -f1)"
else ACTUAL="$(sha256sum "$TMP/freedoom.zip" | cut -d' ' -f1)"; fi
[[ "$ACTUAL" == "$SHA256" ]] || { echo "Freedoom download doesn't match its checksum ($ACTUAL)" >&2; exit 1; }
unzip -q -o "$TMP/freedoom.zip" -d "$TMP"
mkdir -p "$DEST"
cp "$TMP/freedoom-$VERSION/freedoom1.wad" "$TMP/freedoom-$VERSION/COPYING.txt" "$DEST/"
echo "Fetched Freedoom $VERSION into $DEST"
