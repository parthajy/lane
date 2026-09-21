#!/bin/bash
# Packs the signed update artefact and writes the manifest the app polls.
# Output: dist-updates/<version>/Lane.app.tar.gz, .sig, and darwin-aarch64.json
# Upload the folder to https://lane.so/updates/ (keep the version folders).
set -euo pipefail
cd "$(dirname "$0")/.."
VERSION=$(python3 -c "import json;print(json.load(open('src-tauri/tauri.conf.json'))['version'])")
ARCH=$(uname -m | sed 's/arm64/aarch64/')
BUNDLE=src-tauri/target/release/bundle/macos
APP="$BUNDLE/Lane.app"
[ -d "$APP" ] || { echo "build first: npm run build:dmg"; exit 1; }
OUT="dist-updates/$VERSION"
mkdir -p "$OUT"
# Tauri's own updater artefact is signed at build time when the key is set;
# re-pack from the (runtime-signed) app so the shipped bytes are what was signed.
tar -czf "$OUT/Lane.app.tar.gz" -C "$BUNDLE" Lane.app
npx tauri signer sign --private-key-path "$HOME/.tauri/lane.key" -p "" "$OUT/Lane.app.tar.gz" >/dev/null
SIG=$(cat "$OUT/Lane.app.tar.gz.sig")
NOTES="${NOTES:-Improvements and fixes.}"
python3 - "$VERSION" "$ARCH" "$SIG" "$NOTES" "$OUT" <<'PY'
import json, sys, datetime
version, arch, sig, notes, out = sys.argv[1:6]
manifest = {
  "version": version,
  "notes": notes,
  "pub_date": datetime.datetime.now(datetime.timezone.utc).isoformat(),
  "platforms": {f"darwin-{arch}": {"signature": sig, "url": f"https://lane.so/updates/{version}/Lane.app.tar.gz"}},
}
json.dump(manifest, open(f"{out}/darwin-{arch}.json", "w"), indent=2)
print(f"manifest: {out}/darwin-{arch}.json")
PY
echo "upload dist-updates/$VERSION/* to https://lane.so/updates/$VERSION/ and copy darwin-$ARCH.json to https://lane.so/updates/darwin-$ARCH.json"
