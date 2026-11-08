#!/bin/sh
# Builds _jtools.so for the Mac's own python3 (Xcode's Python 3.9) so the in-process extraction path
# (JUMPMAN_INPROC=1) can be tested and byte-compared on a Mac without an iOS device.  Usage:
#   build-mac-jtools.sh OUTDIR     then   PYTHONPATH=OUTDIR JUMPMAN_INPROC=1 python3 ../../byor/pipeline.py ...
set -eu
here="$(cd "$(dirname "$0")" && pwd)"
tools="$here/../../../reference/sm64/tools"
py="$(xcrun --show-sdk-platform-path 2>/dev/null || true)"
inc="$(/usr/bin/python3 -c 'import sysconfig,os;print(sysconfig.get_paths()["include"])')"
[ -f "$inc/Python.h" ] || inc="$(xcode-select -p)/Library/Frameworks/Python3.framework/Versions/3.9/include/python3.9"
mkdir -p "$1"
cd "$here"
cc -O2 -shared -undefined dynamic_lookup -w -include jt_compat.h -I . -I "$tools" -I "$tools/sm64tools" -I "$inc" \
  jtools.c jt_mio0.c jt_n64graphics.c jt_skyconv.c jt_aifc_decode.c jt_codebook.c jt_vadpcm.c -lm -o "$1/_jtools.so"
echo "built $1/_jtools.so"
