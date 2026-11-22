"""Tiny stdlib-only voxel scene builder that prints a `rox apply` op list."""

import math
import random
import sys


class Scene:
    def __init__(self, seed=1):
        self.v = {}
        self.rng = random.Random(seed)

    # -- primitives ---------------------------------------------------------
    def put(self, x, y, z, c):
        if y < 0 or c is None:
            return
        self.v[(int(x), int(y), int(z))] = c

    def get(self, x, y, z):
        return self.v.get((int(x), int(y), int(z)))

    def clear(self, x, y, z):
        self.v.pop((int(x), int(y), int(z)), None)

    def box(self, x0, x1, y0, y1, z0, z1, c):
        for x in range(x0, x1 + 1):
            for y in range(y0, y1 + 1):
                for z in range(z0, z1 + 1):
                    self.put(x, y, z, c() if callable(c) else c)

    def clear_box(self, x0, x1, y0, y1, z0, z1):
        for x in range(x0, x1 + 1):
            for y in range(y0, y1 + 1):
                for z in range(z0, z1 + 1):
                    self.clear(x, y, z)

    def cyl(self, cx, cz, r, y0, y1, c, hollow=False):
        ri = int(math.ceil(r))
        for x in range(-ri, ri + 1):
            for z in range(-ri, ri + 1):
                d = math.hypot(x, z)
                if d <= r and (not hollow or d > r - 1.2):
                    for y in range(y0, y1 + 1):
                        self.put(cx + x, y, cz + z, c(x, y, z) if callable(c) else c)

    def ellipsoid(self, cx, cy, cz, rx, ry, rz, c):
        for x in range(-int(rx) - 1, int(rx) + 2):
            for y in range(-int(ry) - 1, int(ry) + 2):
                for z in range(-int(rz) - 1, int(rz) + 2):
                    if (x / rx) ** 2 + (y / ry) ** 2 + (z / rz) ** 2 <= 1.0:
                        self.put(cx + x, cy + y, cz + z,
                                 c(x, y, z) if callable(c) else c)

    def bounds(self):
        xs = [k[0] for k in self.v]
        ys = [k[1] for k in self.v]
        zs = [k[2] for k in self.v]
        return (min(xs), max(xs), min(ys), max(ys), min(zs), max(zs))

    def recenter(self):
        """Shift so the footprint is centered on the origin and min y = 0."""
        x0, x1, y0, _, z0, z1 = self.bounds()
        dx, dz = -((x0 + x1) // 2), -((z0 + z1) // 2)
        self.v = {(x + dx, y - y0, z + dz): c for (x, y, z), c in self.v.items()}

    # -- output -------------------------------------------------------------
    def emit(self, palette, out=sys.stdout):
        """Write the scene as a `rox apply` op list: `palette` maps names to "#rrggbb"."""
        names = {}
        for name, c in palette.items():
            names.setdefault(c, name)
        cells = {}
        for p, c in sorted(self.v.items()):
            cells.setdefault(c, []).append(p)
        order = [c for c in names if c in cells] + [c for c in cells if c not in names]

        out.write("(\n    colors: {\n")
        for name, c in palette.items():
            out.write(f'        "{name}": "{c}",\n')
        out.write("    },\n    ops: [\n")
        for c in order:
            pts = ", ".join(f"({x}, {y}, {z})" for x, y, z in cells[c])
            out.write(f'        Voxels(color: "{names.get(c, c)}", cells: [{pts}]),\n')
        out.write("    ],\n)\n")
