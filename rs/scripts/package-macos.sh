#!/usr/bin/env bash
# Builds the cross-platform Grimoire for macOS and packages it as rs/dist/Grimoire-<version>.dmg
# (open it, drag Grimoire onto Applications).
#
# Release builds are universal (Apple Silicon + Intel); set NATIVE_ONLY=1 for a faster host-only build.
# Without a Developer ID the app is ad-hoc signed: it runs on this Mac, and other Macs ask the user to approve
# it once (System Settings → Privacy & Security → Open Anyway). With one, set SIGN_IDENTITY (and NOTARY_PROFILE)
# as in ../scripts/make-dmg.sh.
set -euo pipefail
cd "$(dirname "$0")/.."

VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)"
TARGETS=(aarch64-apple-darwin x86_64-apple-darwin)
[[ -n "${NATIVE_ONLY:-}" ]] && TARGETS=("$(rustc -vV | sed -n 's/^host: //p')")

BINARIES=()
for target in "${TARGETS[@]}"; do
    rustup target add "$target" >/dev/null 2>&1 || true
    MACOSX_DEPLOYMENT_TARGET=12.0 cargo build --release -p grimoire-app --target "$target"
    BINARIES+=("target/$target/release/grimoire")
done

APP="dist/Grimoire.app"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
lipo -create "${BINARIES[@]}" -output "$APP/Contents/MacOS/Grimoire"
cp ../Resources/AppIcon.icns "$APP/Contents/Resources/AppIcon.icns"
mkdir -p "$APP/Contents/Resources/licenses"
cp crates/grimoire-app/assets/fonts/*-OFL.txt "$APP/Contents/Resources/licenses/"
# No CFBundleDocumentTypes: winit can't receive Finder's "Open With" events yet, so files are opened by dropping
# them on the window or with the + button instead.
cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleDisplayName</key><string>Grimoire</string>
    <key>CFBundleExecutable</key><string>Grimoire</string>
    <key>CFBundleIconFile</key><string>AppIcon</string>
    <key>CFBundleIdentifier</key><string>com.neonspellcraft.grimoire</string>
    <key>CFBundleName</key><string>Grimoire</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleShortVersionString</key><string>$VERSION</string>
    <key>CFBundleVersion</key><string>1</string>
    <key>LSApplicationCategoryType</key><string>public.app-category.music</string>
    <key>LSMinimumSystemVersion</key><string>12.0</string>
    <key>NSHighResolutionCapable</key><true/>
    <key>NSHumanReadableCopyright</key><string>Grimoire v$VERSION</string>
</dict>
</plist>
PLIST

if [[ -n "${SIGN_IDENTITY:-}" ]]; then
    codesign --force --options runtime --timestamp --sign "$SIGN_IDENTITY" "$APP"
else
    codesign --force --sign - "$APP" >/dev/null 2>&1
fi

DMG="dist/Grimoire-$VERSION.dmg"
STAGING="$(mktemp -d)"
trap 'rm -rf "$STAGING"' EXIT
cp -R "$APP" "$STAGING/"
ln -s /Applications "$STAGING/Applications"
rm -f "$DMG"
hdiutil create -quiet -volname "Grimoire $VERSION" -srcfolder "$STAGING" -fs HFS+ -format UDZO -ov "$DMG"

if [[ -n "${SIGN_IDENTITY:-}" ]]; then
    codesign --force --timestamp --sign "$SIGN_IDENTITY" "$DMG"
    if [[ -n "${NOTARY_PROFILE:-}" ]]; then
        xcrun notarytool submit "$DMG" --keychain-profile "$NOTARY_PROFILE" --wait
        xcrun stapler staple "$DMG"
    fi
fi

echo "Created $DMG ($(du -h "$DMG" | cut -f1), $(lipo -archs "$APP/Contents/MacOS/Grimoire"))"
