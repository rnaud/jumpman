#!/bin/sh
# Build target/Jumpman.app. Bundles the UI art/fonts (assets/ui), the models every course in the
# menu needs, and the shared actors. JUMPMAN_BUNDLE_COURSES=castle_grounds,bob limits the course
# set (default: all); JUMPMAN_ICON=path/to/icon.icns adds an app icon.
set -eu
cd "$(dirname "$0")"
cargo build --release --locked
app=target/Jumpman.app/Contents
rm -rf "$app/Resources/assets"
mkdir -p "$app/MacOS" "$app/Resources"
cp target/release/jumpman "$app/MacOS/Jumpman"
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
