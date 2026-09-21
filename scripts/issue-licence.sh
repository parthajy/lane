#!/usr/bin/env bash
# Issue one Lane licence key.
#
#   scripts/issue-licence.sh partha@example.com lifetime
#
# Prints the key to paste into the buyer's receipt. The key is a signed note:
# Lane checks it offline, so nothing has to be activated against a server.
# Signed with the same key that signs updates ($HOME/.tauri/lane.key).
set -euo pipefail
cd "$(dirname "$0")/.."

email="${1:-}"
plan="${2:-monthly}"
key="${LANE_SIGNING_KEY:-$HOME/.tauri/lane.key}"

if [ -z "$email" ]; then
  echo "usage: scripts/issue-licence.sh <email> [monthly|yearly|lifetime]" >&2
  exit 2
fi
case "$plan" in
  monthly|yearly|lifetime) ;;
  *) echo "plan must be monthly, yearly or lifetime" >&2; exit 2 ;;
esac
[ -f "$key" ] || { echo "no signing key at $key" >&2; exit 1; }

payload="lane1|${email}|${plan}|$(python3 -c 'import time; print(int(time.time()*1000))')"
tmp="$(mktemp -t lane-licence)"
printf '%s' "$payload" > "$tmp"

sig="$(npx --no-install tauri signer sign \
  --private-key-path "$key" -p "${LANE_SIGNING_PASSWORD:-}" "$tmp" \
  | grep -A1 'Public signature' | tail -1 | tr -d '[:space:]')"
rm -f "$tmp" "$tmp.sig"

[ -n "$sig" ] || { echo "signing produced nothing" >&2; exit 1; }
echo "${payload}::${sig}"
