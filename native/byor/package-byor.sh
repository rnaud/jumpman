#!/bin/sh
# Build target/Jumpman-BYOR.app: a distributable app with NO Nintendo-derived data. It carries the
# engine, original assets (UI art, fonts, Quaternius models, synthesised voices, item sprites) and
# the extraction scripts + decomp source subsets; the user's ROMs are turned into assets on first run.
# byor-src/reference bundles the n64decomp/zeldaret decomp sources (owner decision 2026-11-08: acceptable;
# they contain code and text only). Afterwards byor/verify-byor-app.py audits the bundle for ROM data.
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
# Decomp source subsets the exporters read (no ROM data). Binary blobs (RSP microcode, toolchain objects,
# doc images) and the vendored compilers are not needed to extract, so they are pruned (verify-byor-app.py
# fails on binary assets inside these trees).
prune_copy() {
  rsync -a --exclude .git --exclude build --exclude '*.bin' --exclude '*.png' --exclude '*.jpg' --exclude '*.o' --exclude '*.a' \
    --exclude docs --exclude doxygen --exclude 'tools/ido*' --exclude 'tools/ique*' --exclude __pycache__ "../reference/$1/" "$src/reference/$1/"
}
for g in mk64 oot banjo-kazooie; do
  test -n "$(ls -A ../reference/$g 2>/dev/null)" || { echo "reference/$g is empty: git submodule update --init reference/$g" >&2; exit 1; }
done
for g in sm64 mk64 oot banjo-kazooie; do prune_copy $g; done
cat > "$app/Info.plist" <<'PL'
<?xml version="1.0" encoding="UTF-8"?><!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict><key>CFBundleName</key><string>Jumpman</string><key>CFBundleIdentifier</key><string>app.jumpman.byor</string><key>CFBundleExecutable</key><string>Jumpman</string><key>CFBundlePackageType</key><string>APPL</string><key>CFBundleShortVersionString</key><string>1.0</string><key>LSMinimumSystemVersion</key><string>12.0</string><key>NSHighResolutionCapable</key><true/></dict></plist>
PL
# Ad-hoc signature (the app is not notarised; on first launch use right-click > Open).
codesign --force --deep --sign - target/Jumpman-BYOR.app
python3 byor/verify-byor-app.py target/Jumpman-BYOR.app
echo "built target/Jumpman-BYOR.app ($(du -sh target/Jumpman-BYOR.app | cut -f1))"
