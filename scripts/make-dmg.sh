#!/usr/bin/env bash
# Builds Grimoire and packages it as dist/Grimoire-<version>.dmg: open it, drag Grimoire onto Applications.
#
# Without a Developer ID the app is ad-hoc signed. It runs on this Mac; other Macs will ask the user to
# approve it once (System Settings → Privacy & Security → Open Anyway).
#
# With an Apple Developer account you can sign and notarize instead (this path is untested so far):
#   SIGN_IDENTITY="Developer ID Application: Your Name (TEAMID)" \
#   NOTARY_PROFILE=grimoire-notary \   # created once with: xcrun notarytool store-credentials grimoire-notary
#   scripts/make-dmg.sh
set -euo pipefail
cd "$(dirname "$0")/.."

./bundle.sh release

APP="build/Grimoire.app"
VERSION="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' Resources/Info.plist)"
DMG="dist/Grimoire-$VERSION.dmg"

if [[ -n "${SIGN_IDENTITY:-}" ]]; then
    codesign --force --options runtime --timestamp --sign "$SIGN_IDENTITY" "$APP"
fi

STAGING="$(mktemp -d)"
trap 'rm -rf "$STAGING"' EXIT
cp -R "$APP" "$STAGING/"
ln -s /Applications "$STAGING/Applications"

mkdir -p dist
rm -f "$DMG"
hdiutil create -quiet -volname "Grimoire $VERSION" -srcfolder "$STAGING" -fs HFS+ -format UDZO -ov "$DMG"

if [[ -n "${SIGN_IDENTITY:-}" ]]; then
    codesign --force --timestamp --sign "$SIGN_IDENTITY" "$DMG"
    if [[ -n "${NOTARY_PROFILE:-}" ]]; then
        xcrun notarytool submit "$DMG" --keychain-profile "$NOTARY_PROFILE" --wait
        xcrun stapler staple "$DMG"
    fi
fi

echo "Created $DMG ($(du -h "$DMG" | cut -f1))"
