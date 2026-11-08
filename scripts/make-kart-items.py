#!/usr/bin/env python3
"""Draw the kart experiment's item icons (mushroom, banana, green shell) and the item box question mark as 64x64
PNGs under public/mk64/. Procedural pixel art, no ROM or third-party packages needed. The MK64 originals live in
the ROM's common texture set and are not extracted here; these stand in for them in the HUD, in the world and on
the item box.
"""
import argparse
import math
import struct
import zlib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SIZE = 64
SS = 4  # supersampling


def png(path, width, height, pixels):
    def chunk(kind, value):
        return struct.pack('>I', len(value)) + kind + value + struct.pack('>I', zlib.crc32(kind + value))
    rows = b''.join(b'\0' + bytes(pixels[y * width * 4:(y + 1) * width * 4]) for y in range(height))
    path.write_bytes(b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', struct.pack('>IIBBBBB', width, height, 8, 6, 0, 0, 0)) +
                     chunk(b'IDAT', zlib.compress(rows, 9)) + chunk(b'IEND', b''))


def ellipse(x, y, cx, cy, rx, ry):
    return ((x - cx) / rx) ** 2 + ((y - cy) / ry) ** 2 <= 1


def dist_segment(x, y, ax, ay, bx, by):
    dx, dy = bx - ax, by - ay
    t = max(0, min(1, ((x - ax) * dx + (y - ay) * dy) / (dx * dx + dy * dy or 1)))
    return math.hypot(x - (ax + t * dx), y - (ay + t * dy))


def dist_polyline(x, y, pts):
    return min(dist_segment(x, y, *pts[i], *pts[i + 1]) for i in range(len(pts) - 1))


def raster(painter):
    out = bytearray(SIZE * SIZE * 4)
    for py in range(SIZE):
        for px in range(SIZE):
            acc = [0.0, 0.0, 0.0, 0.0]
            for sy in range(SS):
                for sx in range(SS):
                    r, g, b, a = painter(px + (sx + 0.5) / SS, py + (sy + 0.5) / SS)
                    acc[0] += r * a; acc[1] += g * a; acc[2] += b * a; acc[3] += a
            i = (py * SIZE + px) * 4
            if acc[3] > 0:
                out[i:i + 3] = bytes(int(v / acc[3]) for v in acc[:3])
            out[i + 3] = int(255 * acc[3] / (SS * SS))
    return out


CLEAR = (0, 0, 0, 0.0)
OUTLINE = (34, 24, 30, 1.0)


def outlined(inside, grow=2.2):
    """inside(x, y) -> colour or None; adds a dark outline `grow` pixels wide around the union."""
    def painter(x, y):
        c = inside(x, y)
        if c:
            return (*c, 1.0)
        for ang in range(0, 360, 30):
            if inside(x + grow * math.cos(math.radians(ang)), y + grow * math.sin(math.radians(ang))):
                return OUTLINE
        return CLEAR
    return painter


def mushroom(x, y):
    cap = ellipse(x, y, 32, 30, 27, 21) and y < 34
    if cap:
        for sx, sy, r in ((19, 25, 6), (44, 24, 7), (32, 14, 5.5)):
            if ellipse(x, y, sx, sy, r, r):
                return (252, 252, 248)
        return (226, 52, 44) if y > 12 or ellipse(x, y, 32, 30, 22, 17) else (226, 52, 44)
    stem = ellipse(x, y, 32, 44, 15, 13) and y >= 32
    if stem or (21 < x < 43 and 33 <= y < 44):
        if ellipse(x, y, 27.5, 43, 2.3, 4.2) or ellipse(x, y, 36.5, 43, 2.3, 4.2):
            return (40, 30, 40)
        return (250, 232, 196)
    return None


def banana(x, y):
    # A crescent: a big disc minus a shifted one, tipped brown at both ends.
    outer = ellipse(x, y, 34, 34, 26, 26)
    inner = ellipse(x, y, 25, 24, 22, 22)
    if outer and not inner and (x + y > 30 or True):
        # Keep the lower-right sweep only.
        if (x - 34) + (y - 34) > -16:
            tip = math.hypot(x - 50, y - 12) < 6.5 or math.hypot(x - 11, y - 55) < 6.5
            return (118, 82, 38) if tip else ((250, 222, 40) if x + y < 80 else (240, 196, 24))
    return None


def shell(x, y):
    dome = ellipse(x, y, 32, 36, 27, 24) and y < 40
    rim = ellipse(x, y, 32, 41, 28, 8) and y >= 36
    if rim:
        return (250, 244, 224) if not ellipse(x, y, 32, 41, 23, 4.5) else (232, 222, 190)
    if dome:
        # Hexagon plates picked out in lighter green.
        for cx, cy in ((32, 28), (18, 34), (46, 34)):
            if math.hypot(x - cx, y - cy) < 7.5:
                return (96, 200, 90)
        shade = 70 + int(30 * (1 - (y - 14) / 26))
        return (36, shade + 90, 52)
    return None


def question(x, y):
    # A bold white '?' with a dark rim: the arc, the stem and the dot.
    pts = [(21, 20), (24, 14), (30, 11), (36, 11), (42, 14), (45, 20), (43, 27), (37, 32), (32, 36), (32, 42)]
    if dist_polyline(x, y, pts) < 5.6 or math.hypot(x - 32, y - 53) < 6.2:
        return (255, 255, 255)
    return None


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, default=ROOT / 'public/mk64')
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    for name, shape in (('mushroom', mushroom), ('banana', banana), ('shell', shell), ('question', question)):
        painter = outlined(shape, 2.6 if name == 'question' else 2.2)
        png(args.output / f'item-{name}.png', SIZE, SIZE, raster(painter))
    print('Wrote 4 item icons to', args.output)


if __name__ == '__main__':
    main()
