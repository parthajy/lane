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

# Built without ever mounting the disc.
#
# `hdiutil create -srcfolder` mounts the new image at /Volumes/Lane and
# copies the files in, and that path is TCC-protected the moment anybody has
# run Lane from a disc of the same name and granted it a permission. From
# then on every build on that Mac dies with "could not access
# /Volumes/Lane/Lane.app - Operation not permitted" — and with -quiet, dies
# saying nothing at all. makehybrid lays the filesystem down directly, so
# there is no mount and nothing for macOS to object to.
rm -f "$OUT" "$OUT.raw.dmg"
hdiutil makehybrid -hfs -hfs-volume-name "Lane" -o "$OUT.raw.dmg" "$STAGE" -quiet
hdiutil convert "$OUT.raw.dmg" -format UDZO -o "$OUT" -quiet
rm -f "$OUT.raw.dmg"

# Sign the disk image with the same identity as the app (ad-hoc until the Developer ID step).
IDENTITY=$(codesign -dvv "$APP" 2>&1 | sed -n 's/^Authority=\(.*\)/\1/p' | head -1)
codesign --force --sign "${IDENTITY:-Developer ID Application: Partha Borthakur (6AKUD88CVN)}" --timestamp "$OUT" 2>/dev/null || true
du -h "$OUT" | cut -f1 | xargs -I{} echo "dmg: $OUT ({})"
