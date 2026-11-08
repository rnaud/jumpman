#!/bin/bash
# Downloads BeeWare's Python-Apple-Support iOS build (CPython 3.13, PEP 730 tier 3) into native/ios/vendor/ for the
# BYOR target. ~35 MB download, ~120 MB unpacked (only the arm64 slices and a pruned stdlib end up in the app).
set -euo pipefail
cd "$(dirname "$0")"
VER=3.13-b18
FILE=Python-3.13-iOS-support.b18.tar.gz
SHA256="${PYTHON_SUPPORT_SHA256:-d06acf2a9c7964a8689566353a0e707402eff822037de5e29ba0e16040f7047d}"
if [ -d vendor/Python.xcframework ]; then echo "vendor/Python.xcframework already present"; exit 0; fi
mkdir -p vendor
curl -fL -o "vendor/$FILE" "https://github.com/beeware/Python-Apple-Support/releases/download/$VER/$FILE"
if [ -n "$SHA256" ]; then echo "$SHA256  vendor/$FILE" | shasum -a 256 -c -; else shasum -a 256 "vendor/$FILE"; fi
tar -xzf "vendor/$FILE" -C vendor Python.xcframework
rm "vendor/$FILE"
echo "installed vendor/Python.xcframework"
