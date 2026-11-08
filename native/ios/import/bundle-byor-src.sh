#!/bin/bash
# Xcode run-script phase of the BYOR target: <app>/byor-src (pipeline, exporter scripts, original assets, manifest-listed
# decomp files, exporters.json). BYOR_REPO_ROOT can point at a checkout whose reference/ submodules are populated.
set -euo pipefail
"$SRCROOT/../byor/stage-byor-src.sh" "$CODESIGNING_FOLDER_PATH/byor-src"
du -sk "$CODESIGNING_FOLDER_PATH/byor-src" | awk '{printf "byor-src: %.1f MB\n", $1/1024}'
