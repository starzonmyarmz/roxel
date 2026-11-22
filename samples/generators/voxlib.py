"""Tiny stdlib-only voxel scene builder: writes Roxel .rox and an iso PNG preview."""

import math
import random
import struct
import zlib


def hexc(h):
    h = h.lstrip("#")
    return (int(h[0:2], 16), int(h[2:4], 16), int(h[4:6], 16))


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
    def write_rox(self, path):
        out = ["(", "    voxels: ["]
        for (x, y, z), (r, g, b) in sorted(self.v.items()):
            out.append(f"        (({x}, {y}, {z}), ({r}, {g}, {b}, 255)),")
        out += ["    ],", ")"]
        with open(path, "w") as f:
            f.write("\n".join(out) + "\n")
        return len(self.v)

    def render(self, path, tile=6, bg=(236, 240, 244), view=0):
        """Isometric preview from the (+x, +y, +z) octant, rotated by `view` quarter turns."""
        v = self.v
        if view:
            def rot(p):
                x, y, z = p
                for _ in range(view):
                    x, z = -z, x
                return (x, y, z)
            v = {rot(p): c for p, c in v.items()}
        hw, hh = tile, tile // 2  # half-width, quarter-height of the rhombus
        vh = tile                 # vertical extent of one voxel

        def proj(x, y, z):
            return ((x - z) * hw, (x + z) * hh - y * vh)

        pts = [proj(*p) for p in v]
        minx = min(p[0] for p in pts) - hw - 4
        maxx = max(p[0] for p in pts) + hw + 4
        miny = min(p[1] for p in pts) - hh * 2 - 4
        maxy = max(p[1] for p in pts) + vh + hh * 2 + 4
        W, H = maxx - minx, maxy - miny
        buf = bytearray(bytes(bg) * (W * H))

        # sprite masks relative to the voxel's top-center vertex
        top, left, right = [], [], []
        for py in range(0, 2 * hh + vh):
            for px in range(-hw, hw):
                fx, fy = px + 0.5, py + 0.5
                # top rhombus: centered at (0, hh)
                if abs(fx) / hw + abs(fy - hh) / hh <= 1.0:
                    top.append((px, py))
                    continue
                # side faces: below rhombus edges, above bottom edges
                if fx < 0:
                    upper = hh + (fx / hw) * -hh  # left edge of rhombus lower half
                    upper = hh + (hh * (1 + fx / hw))
                    if fy >= hh - (-fx) * hh / hw + hh and fy < hh - (-fx) * hh / hw + hh + vh:
                        left.append((px, py))
                else:
                    if fy >= hh - fx * hh / hw + hh and fy < hh - fx * hh / hw + hh + vh:
                        right.append((px, py))
        # left face in screen = +x? With proj, +x goes right-down, +z goes left-down.
        # Screen-left visible face is the +z face, screen-right is the +x face.
        for (x, y, z) in sorted(v, key=lambda p: (p[0] + p[1] + p[2], p[1])):
            c = v[(x, y, z)]
            ox, oy = proj(x, y, z)
            ox -= minx
            oy -= miny
            oy -= hh  # top vertex
            faces = []
            if (x, y + 1, z) not in v:
                faces.append((top, 1.0))
            if (x, y, z + 1) not in v:
                faces.append((left, 0.78))
            if (x + 1, y, z) not in v:
                faces.append((right, 0.62))
            for mask, k in faces:
                col = bytes(min(255, int(ch * k)) for ch in c)
                for px, py in mask:
                    X, Y = ox + px, oy + py
                    if 0 <= X < W and 0 <= Y < H:
                        i = (Y * W + X) * 3
                        buf[i:i + 3] = col
        raw = b"".join(b"\x00" + bytes(buf[r * W * 3:(r + 1) * W * 3]) for r in range(H))

        def chunk(t, d):
            return struct.pack(">I", len(d)) + t + d + struct.pack(">I", zlib.crc32(t + d) & 0xFFFFFFFF)

        png = (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", W, H, 8, 2, 0, 0, 0))
               + chunk(b"IDAT", zlib.compress(raw, 6)) + chunk(b"IEND", b""))
        with open(path, "wb") as f:
            f.write(png)
