#!/usr/bin/env bash
# One command from source to shipped.
#
#   npm run ship
#
# Builds, signs, notarises, staples, uploads the build to the repository's
# releases page, writes the update manifest and pushes it. Netlify redeploys
# the site on that push, and the download button follows the new release by
# itself, so there is nothing to click anywhere.
#
# NOTES="What changed" npm run ship   puts release notes on all of it.
set -euo pipefail
cd "$(dirname "$0")/.."

VERSION=$(python3 -c "import json;print(json.load(open('src-tauri/tauri.conf.json'))['version'])")
ARCH=$(uname -m | sed 's/arm64/aarch64/')
echo "▸ Lane $VERSION ($ARCH)"

# Three files carry the version and only one of them is the bundle's. The
# crate's goes into the log, the diagnostics people send us and what the
# MCP server calls itself, so a build that disagrees with itself is a
# support problem months later rather than a failure now.
CRATE=$(sed -n 's/^version = "\(.*\)"/\1/p' src-tauri/Cargo.toml | head -1)
PKG=$(python3 -c "import json;print(json.load(open('package.json'))['version'])")
if [ "$CRATE" != "$VERSION" ] || [ "$PKG" != "$VERSION" ]; then
  echo "  versions disagree: tauri.conf.json $VERSION, Cargo.toml $CRATE, package.json $PKG" >&2
  echo "  set all three to the same thing and run again." >&2
  exit 1
fi

if [ -n "$(git status --porcelain --untracked-files=no)" ]; then
  echo "  there are uncommitted changes. Commit them first, so the build matches the code." >&2
  exit 1
fi

echo "▸ building"
npm run build:app

if xcrun notarytool history --keychain-profile lane >/dev/null 2>&1; then
  echo "▸ notarising the app, then building and notarising the disc"
  scripts/notarize.sh all
else
  cat >&2 <<'MSG'
  This Mac has no notarisation credentials, so the build would be stopped by
  Gatekeeper on anyone else's machine. Set them up once with:

      xcrun notarytool store-credentials lane

  It asks for your Apple ID, an app-specific password from appleid.apple.com,
  and the team 6AKUD88CVN.
MSG
  exit 1
fi

echo "▸ update manifest"
scripts/publish-update.sh

echo "▸ publishing the release"
scripts/release-github.sh

if [ -n "$(git status --porcelain site/updates)" ]; then
  echo "▸ pushing the manifest"
  git add site/updates
  git commit -q -m "Lane $VERSION"
  git push -q
fi

echo
echo "▸ done. Lane $VERSION is live:"
echo "   https://github.com/parthajy/lane/releases/tag/v$VERSION"
echo "   https://lane.so/download/mac"
