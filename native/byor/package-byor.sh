#!/bin/sh
# Build target/Jumpman-BYOR.app: a distributable app with NO Nintendo-derived data. It carries the
# engine, original assets (UI art, fonts, Quaternius models, synthesised voices, item sprites) and
# the extraction scripts + decomp source subsets; the user's ROMs are turned into assets on first run.
# OPEN LICENSING QUESTION: byor-src/reference holds n64decomp/zeldaret source (Nintendo-derived code).
# See native/BYOR.md before distributing.
set -eu
cd "$(dirname "$0")/.."
export PATH="$HOME/.rustup/toolchains/stable-aarch64-apple-darwin/bin:$PATH"
cargo build --release --locked
app=target/Jumpman-BYOR.app/Contents
rm -rf "$app"; mkdir -p "$app/MacOS" "$app/Resources/byor-src/byor" "$app/Resources/byor-src/reference"
cp target/release/jumpman "$app/MacOS/Jumpman"
src="$app/Resources/byor-src"
cp byor/pipeline.py "$src/byor/"
rsync -a --exclude '*.mjs' --exclude __pycache__ ../scripts "$src/"
cp ../reference/c2obj.py "$src/reference/"
# Original assets, copied into the data dir by the pipeline (see ORIGINAL in pipeline.py).
mkdir -p "$src/assets/audio" "$src/public/models" "$src/public/mk64"
rsync -a ../assets/ui "$src/assets/"
cp ../assets/audio/kart-voices.json ../assets/audio/character-sounds.json "$src/assets/audio/"
rsync -a ../public/models/quaternius "$src/public/models/"
cp ../public/mk64/item-*.png "$src/public/mk64/"
cp ../public/icon-* ../public/favicon.* ../public/manifest.webmanifest "$src/public/" 2>/dev/null || true
# Decomp source subsets the exporters read (no ROM data).
rsync -a --exclude .git --exclude 'build' ../reference/sm64/ "$src/reference/sm64/"
for g in mk64 oot banjo-kazooie; do
  test -n "$(ls -A ../reference/$g 2>/dev/null)" || { echo "reference/$g is empty: git submodule update --init reference/$g" >&2; exit 1; }
done
rsync -a --exclude .git ../reference/mk64/ "$src/reference/mk64/"
rsync -a --exclude .git ../reference/oot/ "$src/reference/oot/"
rsync -a --exclude .git ../reference/banjo-kazooie/ "$src/reference/banjo-kazooie/"
cat > "$app/Info.plist" <<'PL'
<?xml version="1.0" encoding="UTF-8"?><!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict><key>CFBundleName</key><string>Jumpman</string><key>CFBundleIdentifier</key><string>app.jumpman.byor</string><key>CFBundleExecutable</key><string>Jumpman</string><key>CFBundlePackageType</key><string>APPL</string><key>NSHighResolutionCapable</key><true/></dict></plist>
PL
echo "built target/Jumpman-BYOR.app ($(du -sh target/Jumpman-BYOR.app | cut -f1))"
