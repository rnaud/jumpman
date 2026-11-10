#!/usr/bin/env bash
# Capture a before/after screenshot pair for one commit and register it in changelog/shots.json.
# Usage: scripts/changelog-shot.sh <commit> <name> [VAR=value ...]   (env vars = JUMPMAN_* scenario for the screenshot)
# "before" is the commit's first parent. Builds release in a scratch worktree with a shared target dir, one at a time.
set -euo pipefail
commit=$(git rev-parse --short=8 "$1"); name=$2; shift 2; scenario=("$@")
root=$(git rev-parse --show-toplevel)
scr=${CHANGELOG_SCRATCH:-/private/tmp/jumpman-changelog}
assets=${JUMPMAN_ASSETS:-/Applications/Jumpman.app/Contents/Resources/assets}
mkdir -p "$scr" "$root/changelog/shots"
wt=$scr/wt; [ -d "$wt" ] || git -C "$root" worktree add --detach "$wt" "$commit" >/dev/null
export CARGO_TARGET_DIR=$scr/target
shoot() { # <rev> <label>
  git -C "$wt" checkout -q --detach "$1"
  (cd "$wt/native" && nice ${CARGO_LOCK:-/private/tmp/claude-501/cargolock.sh} env PATH="$HOME/.rustup/toolchains/stable-aarch64-apple-darwin/bin:$PATH" cargo build --release -j 4 --bin jumpman 2>&1 | tail -2)
  local png=$scr/$commit-$name-$2.png
  rm -f "$png"
  env JUMPMAN_ASSETS="$assets" JUMPMAN_OFFSCREEN=1280x720 JUMPMAN_NO_AUDIO=1 JUMPMAN_EXIT_AFTER=12 \
      JUMPMAN_SCREENSHOT="$png" "${scenario[@]}" timeout 120 "$CARGO_TARGET_DIR/release/jumpman" >/dev/null 2>&1 || true
  [ -s "$png" ] || { echo "no screenshot for $2 ($1)"; return 1; }
  sips -s format jpeg -s formatOptions 80 "$png" --out "$root/changelog/shots/$commit-$name-$2.jpg" >/dev/null
}
shoot "$commit~1" before
shoot "$commit" after
python3 - "$root" "$commit" "$name" <<'PY'
import json,sys,os
root,c,n=sys.argv[1:4]; p=f"{root}/changelog/shots.json"
m=json.load(open(p)) if os.path.exists(p) else {}
m.setdefault(c,[]); m[c]=[e for e in m[c] if e["name"]!=n]+[{"name":n,"before":f"changelog/shots/{c}-{n}-before.jpg","after":f"changelog/shots/{c}-{n}-after.jpg"}]
json.dump(m,open(p,"w"),indent=1)
PY
echo "done: $commit $name"
