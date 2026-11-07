#!/usr/bin/env bash
# Builds Grimoire and wraps the binary in build/Grimoire.app.
# Usage: ./bundle.sh [release|debug]
#   Release builds are universal (Apple Silicon + Intel); set NATIVE_ONLY=1 for a faster host-only build.
set -euo pipefail
cd "$(dirname "$0")"

CONFIG="${1:-release}"
ARCH_FLAGS=()
if [[ "$CONFIG" == "release" && -z "${NATIVE_ONLY:-}" ]]; then
    ARCH_FLAGS=(--arch arm64 --arch x86_64)
fi
swift build -c "$CONFIG" ${ARCH_FLAGS[@]+"${ARCH_FLAGS[@]}"}
BIN_DIR="$(swift build -c "$CONFIG" ${ARCH_FLAGS[@]+"${ARCH_FLAGS[@]}"} --show-bin-path)"

APP="build/Grimoire.app"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp "$BIN_DIR/Grimoire" "$APP/Contents/MacOS/Grimoire"
cp Resources/Info.plist "$APP/Contents/Info.plist"
cp Resources/AppIcon.icns "$APP/Contents/Resources/AppIcon.icns"
codesign --force --sign - "$APP" >/dev/null 2>&1

echo "Built $APP ($(du -sh "$APP" | cut -f1), $(lipo -archs "$APP/Contents/MacOS/Grimoire"))"
