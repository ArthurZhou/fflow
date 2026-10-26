#!/usr/bin/env python3
"""Generate fflow's icon — SVG, PNG set and .ico — with no dependencies.

The mark is the app's own language, shrunk to 512 px: one file port on the
left, splitting into a video wire and an audio wire, each ending in a port
dot. Blue = File, orange = Video, green = Audio, exactly like the canvas
legend, so the icon says what the tool does rather than spelling its name.

Everything is drawn from signed distance fields and rendered *per size*
instead of being downscaled, so 16 px gets one-pixel antialiasing of its own
rather than a blurry reduction of 512 px.

    python tools/make_icon.py

writes assets/icon.svg, assets/icon-<size>.png and assets/icon.ico.
"""

import math
import os
import struct
import zlib

# --- palette (mirrors src/theme.rs) ----------------------------------------
TILE_TOP = (0x1C, 0x1E, 0x26)
TILE_BOTTOM = (0x0A, 0x0B, 0x0E)
HAIRLINE = (0xFF, 0xFF, 0xFF, 0.08)
FILE = (0x6C, 0xAE, 0xFF)
VIDEO = (0xFB, 0x92, 0x3C)
AUDIO = (0x45, 0xD4, 0x9A)

SIZES = [16, 24, 32, 48, 64, 128, 256, 512]
ICO_SIZES = [16, 24, 32, 48, 64, 128, 256]

# --- geometry, in a 512 x 512 box -----------------------------------------
# One trunk on the left, forking into two branches on the right. The branches
# leave the hub steeply rather than hugging y = 256, otherwise all three wires
# sit on top of each other for the first third of their length and the fork
# reads as a single blob at 16 px.
TILE_R = 116.0
STEM = ((112.0, 256.0), (238.0, 256.0))
UPPER = ((238.0, 256.0), (252.0, 212.0), (332.0, 156.0), (396.0, 156.0))
LOWER = ((238.0, 256.0), (252.0, 300.0), (332.0, 356.0), (396.0, 356.0))
WIRE_W = 52.0
DOT_L = ((112.0, 256.0), 52.0)
DOT_U = ((396.0, 156.0), 46.0)
DOT_D = ((396.0, 356.0), 46.0)


def lerp(a, b, t):
    return a + (b - a) * t


def sd_round_box(px, py, cx, cy, hw, hh, r):
    qx = abs(px - cx) - (hw - r)
    qy = abs(py - cy) - (hh - r)
    ax, ay = max(qx, 0.0), max(qy, 0.0)
    return math.hypot(ax, ay) + min(max(qx, qy), 0.0) - r


def sd_segment(px, py, ax, ay, bx, by):
    pax, pay = px - ax, py - ay
    bax, bay = bx - ax, by - ay
    denom = bax * bax + bay * bay
    t = 0.0 if denom == 0.0 else max(0.0, min(1.0, (pax * bax + pay * bay) / denom))
    return math.hypot(pax - bax * t, pay - bay * t)


def bezier(p0, p1, p2, p3, steps=40):
    pts = []
    for i in range(steps + 1):
        t = i / steps
        u = 1.0 - t
        x = u * u * u * p0[0] + 3 * u * u * t * p1[0] + 3 * u * t * t * p2[0] + t * t * t * p3[0]
        y = u * u * u * p0[1] + 3 * u * u * t * p1[1] + 3 * u * t * t * p2[1] + t * t * t * p3[1]
        pts.append((x, y))
    return pts


def sd_polyline(px, py, pts):
    best = 1e9
    for (ax, ay), (bx, by) in zip(pts, pts[1:]):
        d = sd_segment(px, py, ax, ay, bx, by)
        if d < best:
            best = d
    return best


STEM_PTS = [STEM[0], STEM[1]]
UPPER_PTS = bezier(*UPPER)
LOWER_PTS = bezier(*LOWER)


def unit(c):
    """Palette entry (0-255, optional alpha) to the 0-1 floats `over` wants."""
    return (c[0] / 255.0, c[1] / 255.0, c[2] / 255.0, c[3] if len(c) > 3 else 1.0)


def over(dst, src, cov):
    """src over dst, both RGBA floats, cov in 0..1."""
    a = src[3] * cov
    if a <= 0.0:
        return dst
    out_a = a + dst[3] * (1.0 - a)
    if out_a <= 0.0:
        return (0.0, 0.0, 0.0, 0.0)
    rgb = [
        (src[i] * a + dst[i] * dst[3] * (1.0 - a)) / out_a for i in range(3)
    ]
    return (rgb[0], rgb[1], rgb[2], out_a)


def grow(pts, s):
    """Scale a polyline about the tile centre."""
    return [((x - 256.0) * s + 256.0, (y - 256.0) * s + 256.0) for x, y in pts]


def render(size):
    """Return `size`x`size` RGBA bytes."""
    scale = size / 512.0
    aa = 1.0 / scale  # one target pixel, in 512-space
    # Small sizes get a little more weight: a 52-unit wire is 1.6 px at 16,
    # and under 2 px is where strokes start to look grey rather than solid.
    boost = 1.0 + max(0.0, (48.0 - size) / 48.0) * 0.30
    half = WIRE_W * 0.5 * boost
    # ...and a little more room, because the rounded tile corners already eat
    # the margin a 16 px icon cannot spare.
    s = 1.0 + max(0.0, (48.0 - size) / 48.0) * 0.10
    hair = size >= 64
    # Below 32 px the port dots turn the ends into 3 px blobs that merge with
    # the wires they cap, so the round line caps do the job on their own.
    dots = size >= 32
    stem_pts, upper_pts, lower_pts = grow(STEM_PTS, s), grow(UPPER_PTS, s), grow(LOWER_PTS, s)
    file_ink, video_ink, audio_ink = unit(FILE), unit(VIDEO), unit(AUDIO)
    hairline_ink = unit(HAIRLINE)

    px = bytearray(size * size * 4)
    for y in range(size):
        for x in range(size):
            cx = (x + 0.5) / scale
            cy = (y + 0.5) / scale
            out = (0.0, 0.0, 0.0, 0.0)

            # Tile.
            d = sd_round_box(cx, cy, 256.0, 256.0, 256.0, 256.0, TILE_R)
            cov = min(max(0.5 - d / aa, 0.0), 1.0)
            if cov > 0.0:
                t = min(max(cy / 512.0, 0.0), 1.0)
                bg = tuple(lerp(TILE_TOP[i], TILE_BOTTOM[i], t) / 255.0 for i in range(3))
                out = (bg[0], bg[1], bg[2], cov)
            if cov <= 0.0:
                i = (y * size + x) * 4
                continue

            # Inner hairline: an inset stroke, skipped where it would be the
            # only thing left of the tile.
            if hair:
                d = abs(sd_round_box(cx, cy, 256.0, 256.0, 254.0, 254.0, TILE_R - 2.0))
                out = over(out, hairline_ink, min(max(0.5 - (d - 1.0) / aa, 0.0), 1.0))

            # Wires, branches first and the trunk last, so the fork point
            # belongs to the file wire — that is where the Demux sits, and it
            # keeps the junction from turning into whichever colour was
            # painted last.
            for pts, ink in (
                (upper_pts, video_ink),
                (lower_pts, audio_ink),
                (stem_pts, file_ink),
            ):
                d = sd_polyline(cx, cy, pts) - half
                out = over(out, ink, min(max(0.5 - d / aa, 0.0), 1.0))

            # Port dots last, so every end is a clean circle of its own colour.
            if dots:
                for (centre, r), ink in (
                    (DOT_L, file_ink),
                    (DOT_U, video_ink),
                    (DOT_D, audio_ink),
                ):
                    cxx = (centre[0] - 256.0) * s + 256.0
                    cyy = (centre[1] - 256.0) * s + 256.0
                    d = math.hypot(cx - cxx, cy - cyy) - r * s * boost
                    out = over(out, ink, min(max(0.5 - d / aa, 0.0), 1.0))

            i = (y * size + x) * 4
            px[i] = min(255, int(round(out[0] * 255)))
            px[i + 1] = min(255, int(round(out[1] * 255)))
            px[i + 2] = min(255, int(round(out[2] * 255)))
            px[i + 3] = min(255, int(round(out[3] * 255)))
    return bytes(px)


def write_png(path, size, rgba):
    raw = bytearray()
    stride = size * 4
    for y in range(size):
        raw.append(0)  # filter: none
        raw += rgba[y * stride:(y + 1) * stride]

    def chunk(tag, data):
        return (
            struct.pack(">I", len(data))
            + tag
            + data
            + struct.pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF)
        )

    png = b"\x89PNG\r\n\x1a\n"
    png += chunk(b"IHDR", struct.pack(">IIBBBBB", size, size, 8, 6, 0, 0, 0))
    png += chunk(b"IDAT", zlib.compress(bytes(raw), 9))
    png += chunk(b"IEND", b"")
    with open(path, "wb") as f:
        f.write(png)


def write_ico(path, images):
    """Multi-size .ico with PNG payloads (Vista and later)."""
    count = len(images)
    header = struct.pack("<HHH", 0, 1, count)
    entries, blobs, offset = b"", b"", 6 + 16 * count
    for size, png in images:
        entries += struct.pack(
            "<BBBBHHII", size % 256, size % 256, 0, 0, 1, 32, len(png), offset
        )
        blobs += png
        offset += len(png)
    with open(path, "wb") as f:
        f.write(header + entries + blobs)


def svg():
    """The same shapes as markup, so the mark stays editable."""
    def rgb(c):
        return "#%02X%02X%02X" % c

    def curve(p):
        (x0, y0), (x1, y1), (x2, y2), (x3, y3) = p
        return f"M {x0:.1f} {y0:.1f} C {x1:.1f} {y1:.1f} {x2:.1f} {y2:.1f} {x3:.1f} {y3:.1f}"

    def line(p):
        (x0, y0), (x1, y1) = p
        return f"M {x0:.1f} {y0:.1f} L {x1:.1f} {y1:.1f}"

    return f"""<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 512 512" width="512" height="512">
  <defs>
    <linearGradient id="tile" x1="0" y1="0" x2="0" y2="1">
      <stop offset="0" stop-color="{rgb(TILE_TOP)}"/>
      <stop offset="1" stop-color="{rgb(TILE_BOTTOM)}"/>
    </linearGradient>
  </defs>
  <rect x="0" y="0" width="512" height="512" rx="{TILE_R}" fill="url(#tile)"/>
  <rect x="2" y="2" width="508" height="508" rx="{TILE_R - 2}" fill="none"
        stroke="#FFFFFF" stroke-opacity="0.08" stroke-width="2"/>
  <g fill="none" stroke-width="{WIRE_W}" stroke-linecap="round">
    <path d="{curve(UPPER)}" stroke="{rgb(VIDEO)}"/>
    <path d="{curve(LOWER)}" stroke="{rgb(AUDIO)}"/>
    <path d="{line(STEM)}" stroke="{rgb(FILE)}"/>
  </g>
  <circle cx="{DOT_L[0][0]}" cy="{DOT_L[0][1]}" r="{DOT_L[1]}" fill="{rgb(FILE)}"/>
  <circle cx="{DOT_U[0][0]}" cy="{DOT_U[0][1]}" r="{DOT_U[1]}" fill="{rgb(VIDEO)}"/>
  <circle cx="{DOT_D[0][0]}" cy="{DOT_D[0][1]}" r="{DOT_D[1]}" fill="{rgb(AUDIO)}"/>
</svg>
"""


def main():
    here = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    out = os.path.join(here, "assets")
    os.makedirs(out, exist_ok=True)

    rendered = {}
    for size in SIZES:
        rgba = render(size)
        rendered[size] = rgba
        write_png(os.path.join(out, f"icon-{size}.png"), size, rgba)
        print(f"assets/icon-{size}.png")

    write_ico(
        os.path.join(out, "icon.ico"),
        [(s, open(os.path.join(out, f"icon-{s}.png"), "rb").read()) for s in ICO_SIZES],
    )
    print("assets/icon.ico")

    with open(os.path.join(out, "icon.svg"), "w", encoding="utf-8") as f:
        f.write(svg())
    print("assets/icon.svg")


if __name__ == "__main__":
    main()
