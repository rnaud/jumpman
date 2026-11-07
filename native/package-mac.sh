#!/bin/sh
set -eu
cd "$(dirname "$0")"
cargo build --release --locked
app=target/Jumpman.app/Contents
mkdir -p "$app/MacOS" "$app/Resources"
cp target/release/jumpman "$app/MacOS/Jumpman"
python3 - "$app" <<'PY'
import json, pathlib, plistlib, shutil, sys
contents = pathlib.Path(sys.argv[1])
assets = pathlib.Path("../assets")
course = pathlib.Path("courses/castle_grounds.json")
data = json.loads((assets / course).read_text())
for path in {course, data["model"], "actors/mario/mario_geo.glb", *(p["model"] for p in data["objects"])}:
    target = contents / "Resources/assets" / path
    target.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(assets / path, target)
with (contents / "Info.plist").open("wb") as out:
    plistlib.dump(dict(CFBundleName="Jumpman", CFBundleDisplayName="Jumpman",
                       CFBundleExecutable="Jumpman", CFBundleIdentifier="local.jumpman.native",
                       CFBundleVersion="1", CFBundleShortVersionString="0.1",
                       CFBundlePackageType="APPL", NSHighResolutionCapable=True), out)
PY
codesign --force --sign - target/Jumpman.app
echo "Built $(pwd)/target/Jumpman.app"
