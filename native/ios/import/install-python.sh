#!/bin/bash
# Xcode run-script phase of the BYOR target: puts CPython 3.13 into the .app.
#   <app>/Frameworks/Python.framework           the interpreter (dynamic framework from Python-Apple-Support)
#   <app>/python/lib/python3.13                  a pruned stdlib (no tests, IDLE, Tk, ensurepip, ...)
#   <app>/python/lib/python3.13/lib-dynload/*.fwork + <app>/Frameworks/<module>.framework
#                                                the extension modules the exporters need, one framework each (App Store rule:
#                                                no loose .so; PEP 730's AppleFrameworkLoader finds them via the .fwork stubs)
# Needs vendor/Python.xcframework (ios/fetch-python.sh).
set -euo pipefail
# Every game imported by the Rust importer (byor/ported-games.txt): no Python runtime in the app.
if ! "$SRCROOT/../byor/python-needed.sh"; then
  echo "note: all games are imported by the Rust importer; not bundling Python"
  rm -rf "$CODESIGNING_FOLDER_PATH/Frameworks/Python.framework" "$CODESIGNING_FOLDER_PATH/python"
  exit 0
fi
PY="$SRCROOT/vendor/Python.xcframework"
[ -d "$PY" ] || { echo "error: $PY missing: run native/ios/fetch-python.sh"; exit 1; }
APP="$CODESIGNING_FOLDER_PATH"
if [ "$PLATFORM_NAME" = "iphonesimulator" ]; then SLICE=ios-arm64_x86_64-simulator; else SLICE=ios-arm64; fi

# Extension modules the pipeline, the exporters and the stdlib pieces they import actually load.
MODULES="_struct zlib math binascii _md5 _sha1 _sha2 _sha3 _blake2 _random _bisect _heapq _json _datetime array select \
_opcode _contextvars unicodedata fcntl mmap _statistics _pickle _queue _posixsubprocess resource _bz2 _lzma _asyncio cmath _csv pyexpat _elementtree"

mkdir -p "$APP/Frameworks" "$APP/python/lib"
rsync -a --delete "$PY/$SLICE/Python.framework/" "$APP/Frameworks/Python.framework/" --exclude Headers
rsync -a --delete "$PY/lib/python3.13/" "$APP/python/lib/python3.13/" \
  --exclude 'test/' --exclude 'idlelib/' --exclude 'tkinter/' --exclude 'turtledemo/' --exclude 'ensurepip/' \
  --exclude 'pydoc_data/' --exclude 'lib2to3/' --exclude 'unittest/' --exclude 'sqlite3/' --exclude 'distutils/' \
  --exclude 'turtle.py' --exclude '__pycache__/' --exclude 'site-packages/'
mkdir -p "$APP/python/lib/python3.13/lib-dynload"
SLICE_LIB="$PY/$SLICE/lib-arm64/python3.13"
# platform config (sysconfig data) next to the stdlib
for f in "$SLICE_LIB"/_sysconfigdata*.py "$SLICE_LIB"/build-details.json; do [ -f "$f" ] && cp "$f" "$APP/python/lib/python3.13/"; done

rm -rf "$APP"/Frameworks/_*.framework "$APP/python/lib/python3.13/lib-dynload"/*
for m in $MODULES; do
  so="$(ls "$SLICE_LIB/lib-dynload/$m".cpython-313-*.so 2>/dev/null | head -1 || true)"
  [ -n "$so" ] || { echo "note: no extension module $m in this slice"; continue; }
  fw="$APP/Frameworks/$m.framework"
  mkdir -p "$fw"
  cp "$so" "$fw/$m"
  cp "$PY/build/iOS-dylib-Info-template.plist" "$fw/Info.plist"
  plutil -replace CFBundleExecutable -string "$m" "$fw/Info.plist"
  plutil -replace CFBundleIdentifier -string "$(echo "$PRODUCT_BUNDLE_IDENTIFIER.$m" | tr '_' '-')" "$fw/Info.plist"
  # the loader reads the framework binary path (relative to the .app) from the .fwork stub left where the .so was
  echo "Frameworks/$m.framework/$m" > "$APP/python/lib/python3.13/lib-dynload/$m.fwork"
  echo "python/lib/python3.13/lib-dynload/$m.fwork" > "$fw/$m.origin"
  # the dylib's install name must be inside the framework
  install_name_tool -id "@rpath/$m.framework/$m" "$fw/$m" 2>/dev/null || true
done

# Sign (ad hoc for the simulator / unsigned builds; Xcode re-signs the app bundle with the real identity at the end, but
# nested code has to be signed first).
IDENTITY="${EXPANDED_CODE_SIGN_IDENTITY:-}"
if [ -z "$IDENTITY" ]; then IDENTITY="-"; fi
for fw in "$APP"/Frameworks/*.framework; do
  /usr/bin/codesign --force --sign "$IDENTITY" --timestamp=none --preserve-metadata=identifier,entitlements,flags "$fw" >/dev/null
done
du -sk "$APP/Frameworks" "$APP/python" | awk '{printf "python runtime: %s %.1f MB\n", $2, $1/1024}'
