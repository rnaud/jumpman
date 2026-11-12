#!/bin/sh
# DEFAULT: builds the DATA-FREE app (byor/package-byor.sh -> target/Jumpman-BYOR.app, audited by byor/verify-byor-app.py). The Mac app
# never carries game data: the user's ROMs are imported on first run into ~/Library/Application Support/Jumpman. The baked-in build
# below is a developer-only convenience that needs the explicit opt-in LOCAL_BAKED=1 and must never be installed or shared.
# Build target/Jumpman.app (LOCAL_BAKED=1 only). The whole assets/ tree is bundled (all courses, actors,
# audio incl. the sequence-player wasm, UI art/fonts, zelda json). JUMPMAN_BUNDLE_COURSES=castle_grounds,bob
# bundles only those courses' models plus the shared actors/audio/ui (smaller app).
# The icon is made from public/icon-512.png; JUMPMAN_ICON=path/to/icon.icns overrides it.
# BYOR=1 ./package-mac.sh builds the distributable app with NO game data instead (byor/package-byor.sh: Jumpman-BYOR.app, audited
# by byor/verify-byor-app.py). The default build below is the LOCAL app: baked-in assets (never leaves this machine) PLUS the
# importer (Contents/Resources/byor-src, the same tree the BYOR app carries), so Settings > OPTIONS shows ROM IMPORT. Imports go to
# ~/Library/Application Support/Jumpman (JUMPMAN_DATA_DIR overrides) and win over the baked-in assets once they exist.
# JUMPMAN_NO_IMPORTER=1 leaves byor-src out (no ROM import in the app). BYOR_REPO_ROOT points at a checkout with the decomp
# submodules (reference/) populated, default `..`.
set -eu
cd "$(dirname "$0")"
if [ -z "${LOCAL_BAKED:-}" ]; then exec byor/package-byor.sh; fi
cargo build --release --locked
app=target/Jumpman.app/Contents
rm -rf "$app/Resources/assets"
mkdir -p "$app/MacOS" "$app/Resources"
cp target/release/jumpman "$app/MacOS/Jumpman"
# Link/Banjo/rig models are read as <assets>/../public/models.
rm -rf "$app/Resources/public"
mkdir -p "$app/Resources/public"
rsync -a --exclude README.md ../public/models "$app/Resources/public/"
# Same ROM export as the browser and iOS runtime. Fail clearly if it is missing.
test -f ../public/mk64/luigi-raceway.json || { echo "Run python3 scripts/export-mk64.py <ROM> before packaging" >&2; exit 1; }
rsync -a ../public/mk64 "$app/Resources/public/"
if [ -z "${JUMPMAN_BUNDLE_COURSES:-}" ]; then
  rsync -a --exclude export-report.json ../assets/ "$app/Resources/assets/"
  echo "bundled full assets tree"
fi
rm -rf "$app/Resources/byor-src"
if [ -z "${JUMPMAN_NO_IMPORTER:-}" ]; then
  byor/stage-byor-src.sh "$app/Resources/byor-src"
  echo "bundled the ROM importer (byor-src): $(du -sk "$app/Resources/byor-src" | awk '{printf "%.1f MB", $1/1024}')"
fi
if [ -z "${JUMPMAN_ICON:-}" ] && [ -f ../public/icon-512.png ] && command -v iconutil >/dev/null; then
  iconset=$(mktemp -d)/Jumpman.iconset
  mkdir -p "$iconset"
  for sz in 16 32 128 256; do
    sips -z $sz $sz ../public/icon-512.png --out "$iconset/icon_${sz}x${sz}.png" >/dev/null
    sips -z $((sz*2)) $((sz*2)) ../public/icon-512.png --out "$iconset/icon_${sz}x${sz}@2x.png" >/dev/null
  done
  cp ../public/icon-512.png "$iconset/icon_512x512.png"
  iconutil -c icns "$iconset" -o target/Jumpman.icns && JUMPMAN_ICON=$(pwd)/target/Jumpman.icns
  export JUMPMAN_ICON
fi
python3 - "$app" <<'PY'
import json, os, pathlib, plistlib, shutil, sys
contents = pathlib.Path(sys.argv[1])
assets = pathlib.Path("../assets")
only = [c for c in os.environ.get("JUMPMAN_BUNDLE_COURSES", "").split(",") if c]
files = set()
# Shared models the app loads outside any course file (Mario, pickups).
files |= {"actors/mario/mario_geo.glb", "actors/coin/yellow_coin_no_shadow_geo.glb",
          "actors/coin/red_coin_no_shadow_geo.glb", "actors/coin/blue_coin_geo.glb"}
for course in sorted((assets / "courses").glob("*.json")):
    if only and course.stem not in only:
        continue
    data = json.loads(course.read_text())
    files.add(f"courses/{course.name}")
    files.add(data.get("model"))
    files.update(data.get("room_models") or [])
    files.update(o.get("model") for o in data.get("objects", []))
# Menu art and optional fonts (assets/ui/fonts/*.ttf).
files |= {str(p.relative_to(assets)) for p in (assets / "ui").rglob("*") if p.is_file()}
# Experiment textures (src/experiments/textures.rs) and the runner's tree.
files |= {"extracted/textures/generic/bob_textures.05800.rgba16.png", "extracted/textures/grass/wf_textures.05000.rgba16.png", "extracted/textures/grass/wf_textures.04800.rgba16.png", "extracted/textures/generic/bob_textures.05000.rgba16.png", "extracted/textures/grass/wf_textures.08000.rgba16.png", "extracted/textures/grass/wf_textures.07000.rgba16.png", "extracted/textures/grass/wf_textures.09000.rgba16.png", "extracted/textures/generic/bob_textures.06000.rgba16.png", "extracted/textures/generic/bob_textures.0A800.rgba16.png", "extracted/textures/sky/rr_textures.08000.rgba16.png", "extracted/textures/sky/rr_textures.03000.rgba16.png", "extracted/textures/sky/rr_textures.01000.rgba16.png", "extracted/textures/sky/rr_textures.05800.rgba16.png", "actors/tree/bubbly_tree_geo.glb"}
# Shared dirs always needed (audio, dialogue art, zelda json, all actors).
for sub in ("audio", "oot-dialogue", "oot-icons", "actors"):
    files |= {str(p.relative_to(assets)) for p in (assets / sub).rglob("*") if p.is_file()}
files |= {"animations.json", "layouts.json"}
files |= {str(p.relative_to(assets)) for p in (assets / "levels").rglob("*.json")}
if not only:
    files = set()  # full tree already rsynced by the shell
copied = 0
for path in sorted(f for f in files if f):
    src = assets / path
    if not src.is_file():
        print(f"warning: missing asset {path}", file=sys.stderr)
        continue
    target = contents / "Resources/assets" / path
    target.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(src, target)
    copied += 1
print(f"bundled {copied} asset files")
info = dict(CFBundleName="Jumpman", CFBundleDisplayName="Jumpman",
            CFBundleExecutable="Jumpman", CFBundleIdentifier="local.jumpman.native",
            CFBundleVersion="1", CFBundleShortVersionString="0.1",
            CFBundlePackageType="APPL", NSHighResolutionCapable=True,
            LSApplicationCategoryType="public.app-category.games")
icon = os.environ.get("JUMPMAN_ICON")
if icon and pathlib.Path(icon).is_file():
    (contents / "Resources").mkdir(exist_ok=True)
    shutil.copyfile(icon, contents / "Resources/Jumpman.icns")
    info["CFBundleIconFile"] = "Jumpman"
with (contents / "Info.plist").open("wb") as out:
    plistlib.dump(info, out)
PY
codesign --force --sign - target/Jumpman.app
echo "Built $(pwd)/target/Jumpman.app"
