#!/bin/sh
# Exit status 0 while some game still needs the Python pipeline, 1 once the Rust importer covers all four games
# (byor/ported-games.txt). The packaging scripts ask this before bundling the Python runtime and the pipeline scripts.
# BYOR_PYTHON_NEEDED=0|1 overrides the answer (to test the Rust-only layout before the last game is ported).
case "${BYOR_PYTHON_NEEDED:-}" in 0) exit 1 ;; 1) exit 0 ;; esac
dir="$(cd "$(dirname "$0")" && pwd)"
for g in sm64 mk64 oot bk; do
  grep -qx "$g" "$dir/ported-games.txt" || exit 0
done
exit 1
