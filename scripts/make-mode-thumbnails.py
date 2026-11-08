#!/usr/bin/env python3
"""Generate the mode-picker thumbnails in assets/ui/modes/<key>.jpg.

Uses the OpenAI Images API (OPENAI_API_KEY from the environment, or parsed from
~/projects/.env without echoing it). Needs Pillow. Existing outputs are skipped
unless --force. Usage: make-mode-thumbnails.py [--force] [--regen] [--model M] [key ...]
Raw generations are cached in $THUMB_RAW_DIR (default: system temp dir) so a
re-crop does not cost another API call; --regen ignores that cache.
"""
import base64, io, json, os, sys, tempfile, urllib.request
from PIL import Image

MODEL = "gpt-image-2"
OUT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "assets", "ui", "modes")
RAW = os.environ.get("THUMB_RAW_DIR") or os.path.join(tempfile.gettempdir(), "jumpman-mode-raw")

STYLE = (
    "Bold, colourful, chunky N64-era-meets-modern video game key art, thick clean shapes, "
    "saturated colours, soft glossy 3D-render look, strong rim light, one clear focal subject, "
    "simple uncluttered composition readable at tiny size, landscape 16:10 framing with the subject centred. "
    "ABSOLUTELY NO text, letters, numbers, logos or UI. Original stylised characters. Scene: "
)
HERO = "a small original cartoon explorer kid seen from behind-three-quarters with a red cap, orange jacket, teal shorts and a tiny backpack, no facial hair"
MODES = {
    "dimensions": f"{HERO} standing on a grassy mountain, split down the middle: the left half is a full 3D scene, the right half flattens into a 2D side-scroller cross-section with pixel-flat platforms, a glowing seam between them. Cyan and magenta palette.",
    "god": f"A giant glowing cartoon hand descending from the clouds placing a wooden plank bridge, a ramp and a spring in front of tiny {HERO} who is mid-jump, divine golden rays, purple sky with gold accents.",
    "runner": f"{HERO} sprinting away from the camera on an endless winding track floating in the sky, hurdles and a gap ahead, a rolling boulder behind, strong speed lines, orange and teal palette.",
    "hundred": f"A huge crowd of dozens of tiny identical {HERO}s swarming across a green hill, packed together, one waving, joyful chaos, bright red and green palette.",
    "diorama": "A miniature toy video-game level inside a clear plastic display case on a wooden table, tilt-shift blur, tiny figures and a tiny castle on grass, a lens glare, warm pastel palette.",
    "race": f"{HERO} and several rival cartoon characters racing up a winding mountain path to a glowing summit flag at the top, dynamic dust, sunrise sky, yellow and orange palette.",
    "shrink": f"A huge looming cartoon giant sneaker next to tiny {HERO} holding a chunky ray gun that fires a pink-lilac beam making a small mushroom scale up, scale contrast, lilac and lime palette.",
    "planet": f"{HERO} running on top of a tiny spherical planet with grass, trees and a tiny castle all curving around the globe, stars in space, small moon, deep blue space and green planet palette.",
    "swarm": "A swarm of hundreds of glowing yellow cartoon stars with tiny smiling faces swirling in a vortex around a hero silhouette in deep space, golden sparkles, midnight blue and gold palette.",
    "kart": "Two chunky cartoon racing go-karts, one with a red-capped driver and one with a green-capped driver, drifting side by side round a banked oval race track in a grand sunny stadium, a banana peel and a spinning shell on the asphalt, motion blur, green and red palette.",
    "laststar": f"Many cartoon heroes in a floating arena shrinking away piece by piece into a void, one {HERO} standing alone on the last platform holding a big gold star overhead, cracking tiles, red and gold on dark purple.",
    "relic": "A brave cartoon adventurer with a lantern and sword descending torch-lit stone dungeon stairs towards a glowing golden relic on an altar, mossy ruins, a treasure chest, warm amber and deep teal palette.",
    "gate": "A sturdy stone castle gate with a raised portcullis defended by cartoon cannon and archer towers, a horde of cute monsters marching up the road toward it, fantasy tower defence view from above, blue and orange palette.",
    "stardeck": "A fan of five glossy playing cards with star emblems and colourful illustrations held in a hand, one card glowing and lifting off, sparkles on a dark felt table, indigo and gold palette.",
    "halls": "A grand castle gallery hall with giant framed paintings on the walls, one painting rippling like a glowing portal showing a snowy mountain world, a hero silhouette in a red cap about to leap in, carpet runner, warm red and violet palette.",
    "brawl": "Two cartoon brawlers fighting in a round arena, one glowing with golden power-form aura, three coloured gems (red, green, blue) on a central pedestal, crates flying, dynamic action, hot pink and cyan palette.",
    "party": "A colourful board-game party scene: a winding board path of coloured tiles on a cheerful island, dice mid-air, confetti, a sun-stone trophy glowing at the centre, party balloons, cartoon pawns in different colours, rainbow festive palette with warm sunset sky.",
    "levels": f"A collage-like castle courtyard with a stone castle and a tall tower, a big framed painting on the wall showing a green hill world, a second painting of snowy mountain, a moat and bridge, {HERO} small in front, sunny blue sky, warm stone and green palette.",
}


def api_key():
    k = os.environ.get("OPENAI_API_KEY")
    if k:
        return k
    for line in open(os.path.expanduser("~/projects/.env")):
        if line.startswith("OPENAI_API_KEY="):
            return line.split("=", 1)[1].strip().strip("\"'")
    sys.exit("OPENAI_API_KEY missing")


def generate(prompt, model, key):
    body = json.dumps({"model": model, "prompt": STYLE + prompt, "size": "1536x1024", "n": 1, "quality": "medium"}).encode()
    req = urllib.request.Request("https://api.openai.com/v1/images/generations", body,
                                 {"Authorization": "Bearer " + key, "Content-Type": "application/json"})
    try:
        d = json.load(urllib.request.urlopen(req, timeout=300))
    except urllib.error.HTTPError as e:
        raise RuntimeError("API error %s: %s" % (e.code, " ".join(e.read().decode().split())[:300]))
    return base64.b64decode(d["data"][0]["b64_json"])


def finish(raw, path):
    im = Image.open(io.BytesIO(raw)).convert("RGB")
    w, h = im.size
    th = round(w * 10 / 16)
    top = max(0, (h - th) // 2)
    im = im.crop((0, top, w, min(h, top + th))).resize((512, 320), Image.LANCZOS)
    q = 82
    while True:
        im.save(path, "JPEG", quality=q, optimize=True, progressive=True)
        if os.path.getsize(path) < 60000 or q <= 60:
            break
        q -= 4


def main():
    args = sys.argv[1:]
    force = "--force" in args
    model = args[args.index("--model") + 1] if "--model" in args else MODEL
    keys = [a for a in args if a in MODES] or list(MODES)
    os.makedirs(OUT, exist_ok=True)
    os.makedirs(RAW, exist_ok=True)
    key = api_key()
    for k in keys:
        path = os.path.join(OUT, k + ".jpg")
        if os.path.exists(path) and not force:
            print("skip", k)
            continue
        rawp = os.path.join(RAW, k + ".png")
        if os.path.exists(rawp) and "--regen" not in args:
            raw = open(rawp, "rb").read()
        else:
            raw = None
            for attempt in range(3):  # output moderation is flaky; retry
                try:
                    raw = generate(MODES[k], model, key)
                    break
                except RuntimeError as e:
                    print("retry", k, attempt, str(e)[:120])
            if raw is None:
                print("FAILED", k)
                continue
            open(rawp, "wb").write(raw)
        finish(raw, path)
        print("wrote", k, os.path.getsize(path))


if __name__ == "__main__":
    main()
