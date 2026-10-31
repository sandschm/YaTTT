#!/usr/bin/env bash
# Regenerates docs/screenshots/*.png from the browser mock (macOS only, needs Xcode CLT).
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
  "$TMP/snap" "$BASE/$query" "$w" "$h" "$TMP/raw-$name.png" 2.0 >/dev/null
  sips --resampleWidth $((w * 2)) "$TMP/raw-$name.png" --out "$TMP/$name.png" >/dev/null
}
# crop <src> <dest> <x> <y> <w> <h>   (CSS px, applied to the 2x image)
crop() {
  local src=$1 dest=$2 x=$3 y=$4 w=$5 h=$6
  sips --cropToHeightWidth $((h * 2)) $((w * 2)) --cropOffset $((y * 2)) $((x * 2)) "$src" --out "$dest" >/dev/null
  echo "$dest"
}

shoot tickets "?view=tickets"
shoot times "?view=times"
shoot stats "?view=stats"
shoot connection "?view=settings&tab=connection"
shoot selection "?view=settings&tab=selection" 1180 1500

crop "$TMP/tickets.png" "$OUT/tickets.png" 0 0 1180 560
crop "$TMP/times.png" "$OUT/times.png" 0 0 1180 560
crop "$TMP/stats.png" "$OUT/stats.png" 0 0 1180 560
crop "$TMP/connection.png" "$OUT/step-1-verbinden.png" 100 40 500 300
crop "$TMP/selection.png" "$OUT/step-2-projekte.png" 100 55 500 145
crop "$TMP/selection.png" "$OUT/step-3-filter.png" 200 205 390 240
crop "$TMP/tickets.png" "$OUT/step-4-timer.png" 100 40 1080 215
crop "$TMP/times.png" "$OUT/step-5-zeiten.png" 100 40 1080 200
crop "$TMP/stats.png" "$OUT/step-6-statistik.png" 100 40 1080 200
