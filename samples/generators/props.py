"""Reusable props: cottage, pier, sailboat."""


def find_flat(height, w, d, lo, hi, near, N):
    """Top-left corner of a w×d footprint whose heights all lie in [lo, hi], closest to `near`."""
    best = None
    for x in range(1, N - w - 1):
        for z in range(1, N - d - 1):
            hs = [height[(x + i, z + j)] for i in range(w) for j in range(d)]
            if min(hs) >= lo and max(hs) <= hi:
                dist = (x - near[0]) ** 2 + (z - near[1]) ** 2
                if best is None or dist < best[0]:
                    best = (dist, x, z, max(hs))
    return best[1:] if best else None


def cottage(s, P, x0, z0, gy, w=9, d=7):
    """Gabled cottage; door + window on +z, window on +x (camera-facing)."""
    s.box(x0, x0 + w - 1, gy - 2, gy, z0, z0 + d - 1, P["rock2"])
    y0 = gy + 1
    s.box(x0, x0 + w - 1, y0, y0 + 5, z0, z0 + d - 1, P["wall"])
    s.clear_box(x0 + 1, x0 + w - 2, y0, y0 + 5, z0 + 1, z0 + d - 2)
    for i in range(d // 2 + 2):
        y = y0 + 6 + i
        za, zb = z0 - 1 + i, z0 + d - i
        if za > zb:
            break
        for x in range(x0 - 1, x0 + w + 1):
            c = P["roof"] if i % 2 == 0 else P["roof2"]
            s.put(x, y, za, c)
            s.put(x, y, zb, c)
        for z in range(za + 1, zb):
            s.put(x0, y, z, P["wall"])
            s.put(x0 + w - 1, y, z, P["wall"])
    s.box(x0 + w - 3, x0 + w - 2, y0 + 6, y0 + 12, z0 + 1, z0 + 2, P["brick"])
    s.box(x0 + 2, x0 + 3, y0, y0 + 3, z0 + d - 1, z0 + d - 1, P["door"])
    s.box(x0 + 5, x0 + 6, y0 + 2, y0 + 3, z0 + d - 1, z0 + d - 1, P["window"])
    s.box(x0 + w - 1, x0 + w - 1, y0 + 2, y0 + 3, z0 + 2, z0 + 4, P["window"])
    s.box(x0 + 2, x0 + 3, gy, gy, z0 + d, z0 + d, P["plank"])
    return (x0 + 2, z0 + d + 1)  # front step


def pier(s, P, x0, x1, z, deck_y):
    """Pier running along +x from x0 to x1, 3 wide, posts every 4."""
    for x in range(x0, x1 + 1):
        for dz in range(3):
            s.put(x, deck_y, z + dz, P["plank"] if x % 3 else P["wood"])
        if (x - x0) % 4 == 3:
            for zz in (z - 1, z + 3):
                s.box(x, x, 0, deck_y + 1, zz, zz, P["wood2"])


def sailboat(s, P, bx, bz, sea):
    """Small boat, hull long axis along x, sail in the x-y plane (faces +z camera side)."""
    for i, (w, y) in enumerate(((1, sea), (2, sea + 1), (2, sea + 2))):
        for x in range(bx - 5 + i, bx + 6 - i):
            for z in range(bz - w, bz + w + 1):
                s.put(x, y, z, P["hull"] if y < sea + 2 else P["hull2"])
    s.clear_box(bx - 3, bx + 3, sea + 2, sea + 2, bz - 1, bz + 1)
    s.box(bx - 3, bx + 3, sea + 1, sea + 1, bz - 1, bz + 1, P["plank"])
    s.box(bx, bx, sea + 2, sea + 13, bz, bz, P["wood2"])
    for i in range(10):
        for x in range(bx + 1, bx + 1 + max(1, 6 - (i * 6) // 10)):
            s.put(x, sea + 3 + i, bz, P["white"])
    for i in range(6):
        for x in range(bx - 1 - max(0, 3 - i // 2), bx):
            s.put(x, sea + 3 + i, bz, P["sail2"])
