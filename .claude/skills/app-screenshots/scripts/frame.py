#!/usr/bin/env python3
"""Measure where the model sits in a Roxel window capture and print the
pan/zoom that moves it onto the composition target.

usage: frame.py <capture.png> [--fill 0.72] [--lift 0] [--mass 0.15] [--nudge-x 0] [--print-bbox]
prints: "<pan_dx> <pan_dy> <scroll_steps> <x> <y> <w> <h>" in window points.
scroll_steps > 0 zooms in (+20 px wheel per step), < 0 zooms out.

The model is found by differencing the canvas against its own background
color; small blobs (floor dots) are removed with a morphological open.
The composition center blends the bounding-box center with the model's
visual-mass centroid (--mass), so a tall thin part like a tower doesn't
drag the heavy part of the model off-center.
"""

import argparse
import math
import subprocess

SCALE = 2  # retina capture
INSPECTOR_W = 245  # left panel, points
RAIL_LEFT = 1170  # tool rail / gizmo column, points
TOP = 24
ZOOM_PER_STEP = 1.037  # model size change per 20 px wheel step, measured
DOWN = 4  # mask downsample for the centroid


def mask(path, win_h):
    x0, x1 = INSPECTOR_W + 6, RAIL_LEFT
    crop = f"{(x1 - x0) * SCALE}x{(win_h - TOP) * SCALE}+{x0 * SCALE}+{TOP * SCALE}"
    bg = subprocess.run(
        ["magick", path, "-crop", crop, "+repage", "-format", "%[pixel:p{8,8}]", "info:"],
        capture_output=True, text=True, check=True).stdout.strip()
    pgm = subprocess.run(
        ["magick", path, "-crop", crop, "+repage",
         "(", "+clone", "-fill", bg, "-colorize", "100", ")",
         "-compose", "difference", "-composite", "-colorspace", "gray",
         "-threshold", "7%", "-morphology", "open", "disk:5",
         "-sample", f"{100 // DOWN}%", "-depth", "8", "pgm:-"],
        capture_output=True, check=True).stdout
    parts = pgm.split(maxsplit=4)
    w, h = int(parts[1]), int(parts[2])
    data = parts[4][-w * h:]
    return data, w, h, x0


def measure(path, win_h):
    data, w, h, x0 = mask(path, win_h)
    xs, ys = [], []
    for i, v in enumerate(data):
        if v > 127:
            xs.append(i % w)
            ys.append(i // w)
    if not xs:
        raise SystemExit("no model found in capture")
    k = DOWN / SCALE  # mask px -> points
    bx, by = min(xs) * k + x0, min(ys) * k + TOP
    bw, bh = (max(xs) - min(xs) + 1) * k, (max(ys) - min(ys) + 1) * k
    mx, my = sum(xs) / len(xs) * k + x0, sum(ys) / len(ys) * k + TOP
    return bx, by, bw, bh, mx, my


def main():
    p = argparse.ArgumentParser()
    p.add_argument("capture")
    p.add_argument("--win-w", type=int, default=1280)
    p.add_argument("--win-h", type=int, default=832)
    p.add_argument("--fill", type=float, default=0.72,
                   help="share of the canvas the model's limiting side fills")
    p.add_argument("--lift", type=float, default=0.0,
                   help="raise the target above center by this share of height")
    p.add_argument("--mass", type=float, default=0.15,
                   help="weight of the visual-mass centroid vs the bbox center")
    p.add_argument("--nudge-x", type=float, default=0.0)
    p.add_argument("--nudge-y", type=float, default=0.0)
    p.add_argument("--print-bbox", action="store_true",
                   help="print only the model's bbox: x y w h")
    a = p.parse_args()

    x, y, w, h, mx, my = measure(a.capture, a.win_h)
    if a.print_bbox:
        print(round(x), round(y), round(w), round(h))
        return
    # center in the visible canvas: between the inspector and the tool
    # rail, eased toward the window edge since the rail floats over it
    target_cx = INSPECTOR_W + (RAIL_LEFT - INSPECTOR_W) / 2 + 25 + a.nudge_x
    target_cy = a.win_h / 2 - a.win_h * a.lift + a.nudge_y
    usable_w = RAIL_LEFT - INSPECTOR_W - 60
    scale = min(usable_w * 0.85 / w, a.win_h * a.fill / h)
    # a clipped model measures small; never zoom in while it touches an edge
    touching = y <= TOP + 2 or y + h >= a.win_h - 2 or x <= INSPECTOR_W + 8 or x + w >= RAIL_LEFT - 2
    if touching:
        scale = min(scale, 0.85)
    steps = round(math.log(scale) / math.log(ZOOM_PER_STEP))
    cx = (1 - a.mass) * (x + w / 2) + a.mass * mx
    cy = (1 - a.mass) * (y + h / 2) + a.mass * my
    print(round(target_cx - cx), round(target_cy - cy), steps,
          round(x), round(y), round(w), round(h))


if __name__ == "__main__":
    main()
