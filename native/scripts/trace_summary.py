#!/usr/bin/env python3
"""Summarise a Bevy Chrome trace (`cargo build --release --features profile-trace`, run the game, it writes trace-*.json).

usage: trace_summary.py trace.json [--skip-secs N] [--top N] [--filter text]

Pairs B/E events per thread and prints, per span name: calls per frame, mean ms per call and ms per frame (inclusive,
summed over all threads, so parallel systems add up to more than the wall-clock frame time). Frames are counted from the
"main app" span. Only spans after --skip-secs (default 6, to skip loading) are counted.
"""
import collections
import json
import sys


def main():
    args = sys.argv[1:]
    path = args[0]
    skip = float(args[args.index("--skip-secs") + 1]) if "--skip-secs" in args else 6.0
    top = int(args[args.index("--top") + 1]) if "--top" in args else 45
    filt = args[args.index("--filter") + 1] if "--filter" in args else ""
    text = open(path).read().rstrip().rstrip(",")
    if not text.endswith("]"):
        text += "]"
    events = json.loads(text)
    t0 = min(e["ts"] for e in events if "ts" in e)
    stacks = collections.defaultdict(list)
    total = collections.defaultdict(float)
    count = collections.Counter()
    for e in events:
        ph = e.get("ph")
        if ph not in ("B", "E", "X"):
            continue
        if ph == "X":
            if (e["ts"] - t0) / 1e6 >= skip:
                n = e["name"]
                total[n] += e.get("dur", 0)
                count[n] += 1
            continue
        key = (e.get("pid"), e.get("tid"))
        if ph == "B":
            stacks[key].append((e["name"], e["ts"]))
        elif stacks[key]:
            name, ts = stacks[key].pop()
            if (ts - t0) / 1e6 >= skip:
                total[name] += e["ts"] - ts
                count[name] += 1
    frames = max(sum(c for n, c in count.items() if n.startswith("main app")), 1)
    print(f"frames counted: {frames}")
    rows = sorted(total.items(), key=lambda kv: -kv[1])
    shown = 0
    for name, us in rows:
        if filt and filt not in name:
            continue
        c = count[name]
        print(f"{us / frames / 1000:8.3f} ms/frame  {c / frames:7.2f} calls/frame  {us / c / 1000:8.3f} ms/call  {name[:110]}")
        shown += 1
        if shown >= top:
            break


main()
