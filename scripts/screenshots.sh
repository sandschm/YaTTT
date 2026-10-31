#!/usr/bin/env bash
# Regenerates docs/screenshots/*.png from the browser mock in English (macOS only, needs Xcode CLT).
# Start the mock first:  npm run dev   (http://localhost:1420)
set -euo pipefail
cd "$(dirname "$0")/.."
BASE="${1:-http://localhost:1420}"
OUT=docs/screenshots
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

swiftc -O -o "$TMP/snap" scripts/snap.swift
mkdir -p "$OUT"

# shoot <name> <query> [cssWidth] [cssHeight] → $TMP/<name>.png at 2x (CSS px × 2)
shoot() {
  local name=$1 query=$2 w=${3:-1180} h=${4:-780}
  "$TMP/snap" "$BASE/$query&lang=en" "$w" "$h" "$TMP/raw-$name.png" 2.0 >/dev/null
  sips --resampleWidth $((w * 2)) "$TMP/raw-$name.png" --out "$TMP/$name.png" >/dev/null
}
# crop <src> <dest> <x> <y> <w> <h>   (CSS px, applied to the 2x image)
crop() {
  local src=$1 dest=$2 x=$3 y=$4 w=$5 h=$6
  sips --cropToHeightWidth $((h * 2)) $((w * 2)) --cropOffset $((y * 2)) $((x * 2)) "$src" --out "$dest" >/dev/null
  echo "$dest"
}

# Menu bar recreation (the native NSMenu cannot be captured without screen-recording rights).
ICON=$(base64 < src-tauri/icons/32x32.png | tr -d '\n')
sed "s|__ICON__|data:image/png;base64,$ICON|" scripts/tray-mockup.html > "$TMP/tray.html"
"$TMP/snap" "file://$TMP/tray.html" 460 440 "$TMP/raw-tray.png" 1.0 >/dev/null
sips --resampleWidth 920 "$TMP/raw-tray.png" --out "$OUT/tray.png" >/dev/null
echo "$OUT/tray.png"

shoot tickets "?view=tickets"
shoot times "?view=times"
shoot stats "?view=stats"
shoot connection "?view=settings&tab=connection"
shoot selection "?view=settings&tab=selection" 1180 1500

# Full windows (1180×780 CSS px) for the header images.
cp "$TMP/tickets.png" "$OUT/tickets.png"
cp "$TMP/times.png" "$OUT/times.png"
cp "$TMP/stats.png" "$OUT/stats.png"
# Content-area crops (x ≥ 200 skips the sidebar, y ≥ 80 the timer bar) for the quick guide.
crop "$TMP/connection.png" "$OUT/step-1-connect.png" 200 80 980 600
crop "$TMP/selection.png" "$OUT/step-2-projects.png" 200 80 980 320
crop "$TMP/selection.png" "$OUT/step-3-filter.png" 390 405 780 470
crop "$TMP/tickets.png" "$OUT/step-4-timer.png" 200 80 980 455
crop "$TMP/times.png" "$OUT/step-5-times.png" 200 80 980 400
crop "$TMP/stats.png" "$OUT/step-6-stats.png" 200 80 980 500
