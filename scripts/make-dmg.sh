#!/bin/bash
# Wraps a built (and signed) Lane.app in a drag-to-Applications DMG.
# Usage: scripts/make-dmg.sh [path/to/Lane.app] [out.dmg]
set -euo pipefail
APP="${1:-src-tauri/target/release/bundle/macos/Lane.app}"
VERSION=$(python3 -c "import json;print(json.load(open('src-tauri/tauri.conf.json'))['version'])")
OUT="${2:-src-tauri/target/release/bundle/dmg/Lane-$VERSION.dmg}"
[ -d "$APP" ] || { echo "no app at $APP"; exit 1; }
mkdir -p "$(dirname "$OUT")"
STAGE=$(mktemp -d)
cp -R "$APP" "$STAGE/"
ln -s /Applications "$STAGE/Applications"
rm -f "$OUT"
hdiutil create -volname "Lane" -srcfolder "$STAGE" -ov -format UDZO -quiet "$OUT"
rm -rf "$STAGE"
# Sign the disk image with the same identity as the app (ad-hoc until the Developer ID step).
IDENTITY=$(codesign -dvv "$APP" 2>&1 | sed -n 's/^Authority=\(.*\)/\1/p' | head -1)
codesign --force --sign "${IDENTITY:-Developer ID Application: Partha Borthakur (6AKUD88CVN)}" --timestamp "$OUT" 2>/dev/null || true
du -h "$OUT" | cut -f1 | xargs -I{} echo "dmg: $OUT ({})"
