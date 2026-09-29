#!/bin/bash
# Notarises Lane with Apple and staples the tickets, so Gatekeeper opens it
# on any Mac — including one that is offline.
#
# Both the app and the disc are notarised, in that order, because the disc is
# built from the app: staple the app afterwards and the copy already sealed
# inside the disc has no ticket. Gatekeeper then asks Apple over the network
# instead, which works until the first person installs Lane on a train.
#
# One-time setup (interactive; asks for your Apple ID, an app-specific
# password from appleid.apple.com, and team 6AKUD88CVN):
#   xcrun notarytool store-credentials lane
set -euo pipefail
cd "$(dirname "$0")/.."
VERSION=$(python3 -c "import json;print(json.load(open('src-tauri/tauri.conf.json'))['version'])")
APP="src-tauri/target/release/bundle/macos/Lane.app"
DMG="src-tauri/target/release/bundle/dmg/Lane-$VERSION.dmg"

case "${1:-all}" in
  app)
    [ -d "$APP" ] || { echo "no app at $APP"; exit 1; }
    ZIP="src-tauri/target/release/bundle/Lane-$VERSION.zip"
    rm -f "$ZIP"
    /usr/bin/ditto -c -k --keepParent "$APP" "$ZIP"
    echo "submitting the app to Apple (a few minutes)…"
    xcrun notarytool submit "$ZIP" --keychain-profile lane --wait
    xcrun stapler staple "$APP"
    rm -f "$ZIP"
    xcrun stapler validate "$APP"
    echo "notarised: $APP"
    ;;
  dmg)
    [ -f "$DMG" ] || { echo "no dmg at $DMG (run scripts/make-dmg.sh)"; exit 1; }
    echo "submitting the disc to Apple (a few minutes)…"
    xcrun notarytool submit "$DMG" --keychain-profile lane --wait
    xcrun stapler staple "$DMG"
    spctl --assess --type open --context context:primary-signature -v "$DMG"
    echo "notarised: $DMG"
    ;;
  all)
    "$0" app
    scripts/make-dmg.sh
    "$0" dmg
    ;;
  *)
    echo "usage: notarize.sh [app|dmg|all]" >&2; exit 2 ;;
esac
