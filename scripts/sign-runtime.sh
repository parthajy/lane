#!/bin/bash
# Signs the bundled llama.cpp runtime inside a built Lane.app.
# Tauri signs the main binary with the hardened runtime; the runtime is a
# separate process and is signed without it so it can load its own dylibs.
# Usage: scripts/sign-runtime.sh [path/to/Lane.app] [identity]   (identity defaults to ad-hoc "-")
set -euo pipefail
APP="${1:-src-tauri/target/release/bundle/macos/Lane.app}"
IDENTITY="${2:-Developer ID Application: Partha Borthakur (6AKUD88CVN)}"
DIR="$APP/Contents/Resources/llama"
[ -d "$DIR" ] || { echo "no runtime at $DIR"; exit 1; }
for f in $(find "$DIR" -type f \( -name "*.dylib" -o -name "llama-server" \)) "$APP/Contents/Resources/audio/lane-audio" "$APP/Contents/Resources/audio/lane-calendar" "$APP/Contents/Resources/audio/lane-ocr" "$APP/Contents/Resources/audio/lane-contacts" "$APP/Contents/Resources/audio/lane-shot" "$APP/Contents/Resources/whisper/whisper-cli" "$APP/Contents/Resources/mcp/lane-mcp"; do [ -f "$f" ] || continue
  codesign --force --sign "$IDENTITY" --timestamp --options runtime "$f"
done
codesign --force --sign "$IDENTITY" --timestamp --options runtime --entitlements src-tauri/Entitlements.plist "$APP/Contents/MacOS/rat-mac"
codesign --force --sign "$IDENTITY" --timestamp --options runtime --entitlements src-tauri/Entitlements.plist "$APP"
codesign --verify --deep --strict "$APP" && echo "signed: $APP"
