#!/bin/sh
# Writes the `byor-src` tree both BYOR apps carry (macOS: Contents/Resources/byor-src, iOS: <app>/byor-src):
# the original (non-Nintendo) assets, the decomp source files the importer reads (byor/decomp-manifest.txt) and the music
# template + sound-player sequence. The import itself is the pure-Rust byor-rs crate linked into the app: no Python, no
# scripts, no hashes file. No ROM data; byor/verify-byor-app.py audits the result.
#   stage-byor-src.sh DEST
set -eu
cd "$(dirname "$0")/.."
src="$1"
repo="${BYOR_REPO_ROOT:-..}"  # a repo checkout (override to point at one whose reference/ submodules are populated)
rm -rf "$src"; mkdir -p "$src/byor" "$src/reference" "$src/scripts/music" "$src/scripts/data"
# The Rust importer (byor-rs) reads: the sound-player sequence (assembled once, iOS has no assembler: byor/gen-precomputed.py) and
# the data-less music engine template (mostly zeros: shipped gzipped; the importer reads either form).
cp byor/sm64-us-sound-player.bin byor/sm64-us-sound-player.json "$src/byor/"
cp $repo/scripts/music/sm64-us-template.json "$src/scripts/music/"
gzip -9 -n -c $repo/scripts/music/sm64-us-template.wasm > "$src/scripts/music/sm64-us-template.wasm.gz"
# MK64's engine template (same idea) and the cached Banjo-Kazooie map catalogue.
cp $repo/scripts/music/mk64-template.json "$src/scripts/music/"
gzip -9 -n -c $repo/scripts/music/mk64-template.wasm > "$src/scripts/music/mk64-template.wasm.gz"
cp $repo/scripts/data/bk-catalogue.json "$src/scripts/data/"
# Original assets, copied into the data dir by the importer (see copy_original in byor-rs/src/sm64_import.rs).
mkdir -p "$src/assets/audio" "$src/assets/ui" "$src/public/models" "$src/public/mk64"
rsync -a --exclude '*.md' $repo/assets/ui/ "$src/assets/ui/"
cp $repo/assets/audio/kart-voices.json $repo/assets/audio/character-sounds.json "$src/assets/audio/"
rsync -a $repo/public/models/quaternius "$src/public/models/"
cp $repo/public/mk64/item-*.png "$src/public/mk64/" 2>/dev/null || true
cp $repo/public/icon-* $repo/public/favicon.* $repo/public/manifest.webmanifest "$src/public/" 2>/dev/null || true
# Decomp sources: exactly the files the exporters read (byor/decomp-manifest.txt, traced from a real run of the
# pipeline with byor/trace_reads.py; byor/check-manifest.py keeps it honest). The rest of the n64decomp / zeldaret
# trees is never opened. No ROM data: verify-byor-app.py fails on binary assets inside the trees.
for g in mk64 oot banjo-kazooie; do
  test -n "$(ls -A $repo/reference/$g 2>/dev/null)" || test -n "${BYOR_ALLOW_MISSING_DECOMP:-}" || { echo "reference/$g is empty: git submodule update --init reference/$g" >&2; exit 1; }
done
# BYOR_ALLOW_MISSING_DECOMP=1 (testing only): a checkout without every submodule bundles the trees it has; the games whose
# sources are missing then fail to import.
rsync -a --files-from=byor/decomp-manifest.txt $repo/reference/ "$src/reference/" || { test -n "${BYOR_ALLOW_MISSING_DECOMP:-}" && echo "note: some decomp files are missing" >&2; } || exit 1
