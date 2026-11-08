#!/bin/sh
# Writes the `byor-src` tree both BYOR apps carry (macOS: Contents/Resources/byor-src, iOS: <app>/byor-src):
# the pipeline + in-process layer, the exporter scripts, the original (non-Nintendo) assets, the decomp source files
# the exporters read and the per-game exporter hashes. No ROM data; byor/verify-byor-app.py audits the result.
#   stage-byor-src.sh DEST
set -eu
cd "$(dirname "$0")/.."
src="$1"
repo="${BYOR_REPO_ROOT:-..}"  # a repo checkout (override to point at one whose reference/ submodules are populated)
rm -rf "$src"; mkdir -p "$src/byor" "$src/reference"
cp byor/pipeline.py byor/inproc.py "$src/byor/"
# The decomp's sound-player sequence, assembled once (iOS has no assembler): see byor/gen-precomputed.py.
cp byor/sm64-us-sound-player.bin byor/sm64-us-sound-player.json "$src/byor/"
# Only the scripts the pipeline runs (byor/scripts-manifest.txt); the maintainers' tools stay out.
mkdir -p "$src/scripts/music"
rsync -a --files-from=byor/scripts-manifest.txt $repo/scripts/ "$src/scripts/"
# The audio engine template is mostly zeros (the blanked ROM blobs): ship it gzipped
# (render-music-template.py reads either form).
gzip -9 -n -c $repo/scripts/music/sm64-us-template.wasm > "$src/scripts/music/sm64-us-template.wasm.gz"
cp $repo/reference/c2obj.py "$src/reference/"
# Original assets, copied into the data dir by the pipeline (see ORIGINAL in pipeline.py).
mkdir -p "$src/assets/audio" "$src/assets/ui" "$src/public/models" "$src/public/mk64"
rsync -a --exclude '*.md' $repo/assets/ui/ "$src/assets/ui/"
cp $repo/assets/audio/kart-voices.json $repo/assets/audio/character-sounds.json "$src/assets/audio/"
rsync -a $repo/public/models/quaternius "$src/public/models/"
cp $repo/public/mk64/item-*.png "$src/public/mk64/"
cp $repo/public/icon-* $repo/public/favicon.* $repo/public/manifest.webmanifest "$src/public/" 2>/dev/null || true
# Decomp sources: exactly the files the exporters read (byor/decomp-manifest.txt, traced from a real run of the
# pipeline with byor/trace_reads.py; byor/check-manifest.py keeps it honest). The rest of the n64decomp / zeldaret
# trees is never opened. No ROM data: verify-byor-app.py fails on binary assets inside the trees.
for g in mk64 oot banjo-kazooie; do
  test -n "$(ls -A $repo/reference/$g 2>/dev/null)" || { echo "reference/$g is empty: git submodule update --init reference/$g" >&2; exit 1; }
done
rsync -a --files-from=byor/decomp-manifest.txt $repo/reference/ "$src/reference/"
# Per-game hash of exporter scripts + decomp inputs + step code; the app compares it with the one recorded in a
# game's stamp to mark only the games whose exporters changed as 'update available' (see BYOR.md, Updates).
python3 byor/pipeline.py --source-root "$repo" --data-dir "$src" --print-hashes > "$src/exporters.json"
