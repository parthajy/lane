#!/bin/bash
# Notarises the built DMG with Apple and staples the ticket, so Gatekeeper
# opens Lane on any Mac. One-time setup (interactive, asks for your Apple ID,
# an app-specific password from appleid.apple.com, and team 6AKUD88CVN):
#   xcrun notarytool store-credentials lane
# Then: npm run release   (builds, signs, notarises, writes the update manifest)
set -euo pipefail
cd "$(dirname "$0")/.."
VERSION=$(python3 -c "import json;print(json.load(open('src-tauri/tauri.conf.json'))['version'])")
DMG="src-tauri/target/release/bundle/dmg/Lane-$VERSION.dmg"
APP="src-tauri/target/release/bundle/macos/Lane.app"
[ -f "$DMG" ] || { echo "no dmg at $DMG (run npm run build:dmg)"; exit 1; }
echo "submitting $DMG to Apple (this takes a few minutes)…"
xcrun notarytool submit "$DMG" --keychain-profile lane --wait
xcrun stapler staple "$DMG"
xcrun stapler staple "$APP" || true
spctl --assess --type open --context context:primary-signature -v "$DMG" && echo "notarised: $DMG"
