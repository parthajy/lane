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
#
# COPYFILE_DISABLE, and it is the whole update mechanism.
#
# Without it macOS tar writes an AppleDouble companion beside every file that
# carries an extended attribute: ._Lane.app, ._Contents, .__CodeSignature and
# forty-five more. macOS tar then hides them again when it lists the archive,
# so `tar -tzf` shows a clean tree and the archive looks right. Tauri unpacks
# with the Rust tar crate, which has no such courtesy, reaches ._Lane.app
# first and stops: "failed to unpack `._Lane.app`". Every update shipped this
# way fails on every machine, and the artefact looks perfect in every check
# made with macOS tar.
COPYFILE_DISABLE=1 tar --no-mac-metadata -czf "$OUT/Lane.app.tar.gz" -C "$BUNDLE" Lane.app

# Proved, not assumed, with a reader that does not hide them.
python3 - "$OUT/Lane.app.tar.gz" <<'CHECK'
import sys, tarfile
with tarfile.open(sys.argv[1]) as t:
    bad = [n for n in t.getnames() if n.startswith("._") or "/._" in n]
if bad:
    sys.exit(f"  the update archive carries {len(bad)} AppleDouble entries ({bad[0]}); "
             "Tauri cannot unpack it. COPYFILE_DISABLE did not take effect.")
CHECK
npx tauri signer sign --private-key-path "$HOME/.tauri/lane.key" -p "" "$OUT/Lane.app.tar.gz" >/dev/null
SIG=$(cat "$OUT/Lane.app.tar.gz.sig")
NOTES="${NOTES:-Improvements and fixes.}"
python3 - "$VERSION" "$ARCH" "$SIG" "$NOTES" "$OUT" <<'PY'
import json, os, sys, datetime
version, arch, sig, notes, out = sys.argv[1:6]
manifest = {
  "version": version,
  "notes": notes,
  "pub_date": datetime.datetime.now(datetime.timezone.utc).isoformat(),
  # The artefact lives on the releases page; the manifest is served by the
  # website, which is what the app polls.
  "platforms": {f"darwin-{arch}": {"signature": sig, "url": f"https://github.com/parthajy/lane/releases/download/v{version}/Lane.app.tar.gz"}},
}
json.dump(manifest, open(f"{out}/darwin-{arch}.json", "w"), indent=2)
os.makedirs("site/updates", exist_ok=True)
json.dump(manifest, open(f"site/updates/darwin-{arch}.json", "w"), indent=2)
print(f"manifest: site/updates/darwin-{arch}.json (commit it; Netlify serves it)")
PY
echo "next: scripts/release-github.sh, then commit site/updates/darwin-$ARCH.json"
