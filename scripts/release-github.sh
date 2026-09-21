#!/usr/bin/env bash
# Publish a built, notarised Lane to the repository's releases page, and point
# the updater at it.
#
#   npm run release          # build, sign, notarise, write the manifest
#   scripts/release-github.sh
#
# The dmg is uploaded twice: once under its version, and once as Lane.dmg so
# that /releases/latest/download/Lane.dmg is always the current build. That is
# the link the website's download button follows.
set -euo pipefail
cd "$(dirname "$0")/.."

VERSION=$(python3 -c "import json;print(json.load(open('src-tauri/tauri.conf.json'))['version'])")
ARCH=$(uname -m | sed 's/arm64/aarch64/')
DMG="src-tauri/target/release/bundle/dmg/Lane-$VERSION.dmg"
OUT="dist-updates/$VERSION"
TAG="v$VERSION"

[ -f "$DMG" ] || { echo "no dmg at $DMG — run npm run build:dmg first" >&2; exit 1; }

if ! xcrun stapler validate "$DMG" >/dev/null 2>&1; then
  echo "warning: $DMG is not notarised. Gatekeeper will stop it on other Macs." >&2
  echo "Run scripts/notarize.sh first, or press return to publish it anyway." >&2
  read -r _
fi

tmp="$(mktemp -d)"
cp "$DMG" "$tmp/Lane.dmg"

gh release view "$TAG" >/dev/null 2>&1 || \
  gh release create "$TAG" --title "Lane $VERSION" --notes "${NOTES:-Improvements and fixes.}"

gh release upload "$TAG" "$DMG" "$tmp/Lane.dmg" --clobber
[ -f "$OUT/Lane.app.tar.gz" ] && gh release upload "$TAG" "$OUT/Lane.app.tar.gz" "$OUT/Lane.app.tar.gz.sig" --clobber
rm -rf "$tmp"

echo
echo "published: https://github.com/parthajy/lane/releases/tag/$TAG"
echo "download:  https://github.com/parthajy/lane/releases/latest/download/Lane.dmg"
echo
echo "Now commit site/updates/darwin-$ARCH.json so the updater sees $VERSION."
