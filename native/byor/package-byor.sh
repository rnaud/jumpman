#!/bin/sh
# Build target/Jumpman-BYOR.app: a distributable app with NO Nintendo-derived data. It carries the
# engine, original assets (UI art, fonts, Quaternius models, synthesised voices, item sprites) and
# the extraction scripts + the decomp source files the exporters read (byor/decomp-manifest.txt); the user's
# ROMs are turned into assets on first run, with no network access.
# byor-src/reference bundles those n64decomp/zeldaret decomp sources (owner decision 2026-11-08: acceptable;
# they contain code and text only). Afterwards byor/verify-byor-app.py audits the bundle for ROM data.
# BYOR_BIN=/path/to/jumpman reuses an already built release binary instead of running cargo.
set -eu
cd "$(dirname "$0")/.."
export PATH="$HOME/.rustup/toolchains/stable-aarch64-apple-darwin/bin:$PATH"
if [ -n "${BYOR_BIN:-}" ]; then bin="$BYOR_BIN"; else cargo build --release --locked; bin=target/release/jumpman; fi
app=target/Jumpman-BYOR.app/Contents
rm -rf "$app"; mkdir -p "$app/MacOS" "$app/Resources/byor-src/byor" "$app/Resources/byor-src/reference"
cp "$bin" "$app/MacOS/Jumpman"
src="$app/Resources/byor-src"
cp byor/pipeline.py "$src/byor/"
# Only the scripts the pipeline runs (byor/scripts-manifest.txt); the maintainers' tools stay out.
mkdir -p "$src/scripts/music"
rsync -a --files-from=byor/scripts-manifest.txt ../scripts/ "$src/scripts/"
# The audio engine template is mostly zeros (the blanked ROM blobs): ship it gzipped
# (render-music-template.py reads either form).
gzip -9 -n -c ../scripts/music/sm64-us-template.wasm > "$src/scripts/music/sm64-us-template.wasm.gz"
cp ../reference/c2obj.py "$src/reference/"
# Original assets, copied into the data dir by the pipeline (see ORIGINAL in pipeline.py).
mkdir -p "$src/assets/audio" "$src/assets/ui" "$src/public/models" "$src/public/mk64"
rsync -a --exclude '*.md' ../assets/ui/ "$src/assets/ui/"
cp ../assets/audio/kart-voices.json ../assets/audio/character-sounds.json "$src/assets/audio/"
rsync -a ../public/models/quaternius "$src/public/models/"
cp ../public/mk64/item-*.png "$src/public/mk64/"
cp ../public/icon-* ../public/favicon.* ../public/manifest.webmanifest "$src/public/" 2>/dev/null || true
# Decomp sources: exactly the files the exporters read (byor/decomp-manifest.txt, traced from a real run of the
# pipeline with byor/trace_reads.py; byor/check-manifest.py keeps it honest). The rest of the n64decomp / zeldaret
# trees is never opened. No ROM data: verify-byor-app.py fails on binary assets inside the trees.
for g in mk64 oot banjo-kazooie; do
  test -n "$(ls -A ../reference/$g 2>/dev/null)" || { echo "reference/$g is empty: git submodule update --init reference/$g" >&2; exit 1; }
done
rsync -a --files-from=byor/decomp-manifest.txt ../reference/ "$src/reference/"
# Per-game hash of exporter scripts + decomp inputs + step code; the app compares it with the one recorded in a
# game's stamp to mark only the games whose exporters changed as 'update available' (see BYOR.md, Updates).
python3 byor/pipeline.py --source-root .. --data-dir "$app/.." --print-hashes > "$src/exporters.json"
cat > "$app/Info.plist" <<'PL'
<?xml version="1.0" encoding="UTF-8"?><!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict><key>CFBundleName</key><string>Jumpman</string><key>CFBundleIdentifier</key><string>app.jumpman.byor</string><key>CFBundleExecutable</key><string>Jumpman</string><key>CFBundlePackageType</key><string>APPL</string><key>CFBundleShortVersionString</key><string>1.0</string><key>LSMinimumSystemVersion</key><string>12.0</string><key>NSHighResolutionCapable</key><true/></dict></plist>
PL
# Ad-hoc signature (the app is not notarised; on first launch use right-click > Open).
codesign --force --deep --sign - target/Jumpman-BYOR.app
python3 byor/verify-byor-app.py target/Jumpman-BYOR.app
# Size report (du, MB on disk).
kb() { du -sk "$@" | awk '{t+=$1} END {printf "%7.1f MB", t/1024}'; }
echo "size report for target/Jumpman-BYOR.app:"
echo "  binary           $(kb "$app/MacOS")"
for g in sm64 mk64 oot banjo-kazooie; do printf '  decomp %-16s %s\n' "$g" "$(kb "$src/reference/$g")"; done
echo "  scripts          $(kb "$src/scripts" "$src/byor")"
echo "  original assets  $(kb "$src/assets" "$src/public")"
echo "  total            $(kb target/Jumpman-BYOR.app)"
