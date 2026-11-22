import math
import sys

from noise import ValueNoise2D
from props import cottage, find_flat, pier, sailboat
from voxlib import Scene, hexc

P = {
    "deep": hexc("2f6f9f"), "water": hexc("3f8fc0"), "shallow": hexc("6cc3d6"),
    "foam": hexc("e8f6f8"),
    "sand": hexc("f0d9a0"), "sand2": hexc("e2c487"),
    "rock": hexc("8a8f98"), "rock2": hexc("6d727c"), "rock3": hexc("a3a8ae"),
    "grass": hexc("7cc155"), "grass2": hexc("69ae46"), "grass3": hexc("90cf62"),
    "dirt": hexc("8a6142"),
    "white": hexc("f4f1ea"), "red": hexc("d8443c"), "red_dk": hexc("a8302c"),
    "glass": hexc("ffd96a"), "iron": hexc("3a3f4b"),
    "wall": hexc("efe6d2"), "roof": hexc("3f6fa8"), "roof2": hexc("355f92"),
    "door": hexc("8a5232"), "window": hexc("9fd8f0"), "brick": hexc("b0533e"),
    "wood": hexc("a8774a"), "wood2": hexc("8c6038"), "plank": hexc("c79a62"),
    "pine": hexc("2f7d4f"), "pine2": hexc("3b9160"), "trunk": hexc("6b4a2f"),
    "flower_y": hexc("ffd34d"), "flower_p": hexc("f28bb3"),
    "hull": hexc("d8443c"), "hull2": hexc("f4f1ea"),
    "sail2": hexc("f2c14e"), "wave": hexc("5aa8d2"),
}

N = 58          # plot size
SEA = 3         # top of water layer (y = 0..SEA-1 is water)


def build():
    s = Scene(seed=11)
    rng = s.rng
    noise = ValueNoise2D(5)
    cx, cz = 29, 29

    # island: blobby mask, terraced into beach / meadow / cliff plateau
    height = {}
    for x in range(N):
        for z in range(N):
            d = math.hypot((x - cx) / 23.0, (z - cz) / 21.0)
            n = noise.fbm(x / 8.0, z / 8.0)
            edge = 1.0 - d + (n - 0.5) * 0.45
            if edge < 0:
                height[(x, z)] = SEA - 1 - (1 if edge < -0.12 else 0)
                continue
            if edge < 0.10:
                h = SEA          # beach
            elif edge < 0.16:
                h = SEA + 1
            else:
                h = SEA + 3 + int(n * 2.2)   # meadow
            # plateau toward the back-left (-x, -z) with a ragged cliff edge
            plateau = (cx - x) * 0.55 + (cz - z) * 0.75 + (noise.fbm(x / 5.0 + 40, z / 5.0) - 0.5) * 9
            if plateau > 2 and edge > 0.10:
                h = SEA + 10 + int(n * 2)
            height[(x, z)] = h

    for x in range(N):
        for z in range(N):
            h = height[(x, z)]
            if h < SEA:
                for y in range(SEA):
                    if y < SEA - 1:
                        c = P["deep"]
                    else:
                        c = P["shallow"] if h == SEA - 1 else P["water"]
                    s.put(x, y, z, c)
                continue
            for y in range(h + 1):
                if h <= SEA + 1:
                    c = P["sand"] if rng.random() < 0.8 else P["sand2"]
                elif y == h:
                    r = rng.random()
                    c = P["grass"] if r < 0.75 else (P["grass2"] if r < 0.9 else P["grass3"])
                elif y >= h - 1:
                    c = P["dirt"]
                elif y <= SEA + 1:
                    c = P["sand2"] if y == SEA + 1 else P["rock2"]
                else:
                    band = (y + (x * 3 + z) // 9) % 5
                    c = (P["rock"], P["rock2"], P["rock"], P["rock3"], P["rock"])[band]
                s.put(x, y, z, c)

    # foam ring at the shoreline
    for x in range(N):
        for z in range(N):
            if height[(x, z)] < SEA:
                land = any(height.get((x + dx, z + dz), -9) >= SEA
                           for dx, dz in ((1, 0), (-1, 0), (0, 1), (0, -1)))
                if land and rng.random() < 0.75:
                    s.put(x, SEA - 1, z, P["foam"])

    def ground(x, z):
        y = SEA
        while s.get(x, y, z) is not None:
            y += 1
        return y

    # -- lighthouse on the high north-west shoulder ------------------------
    lx, lz = 22, 22
    gy = max(ground(lx + dx, lz + dz) for dx in range(-4, 5) for dz in range(-4, 5))
    s.cyl(lx, lz, 5.2, gy - 3, gy - 1, P["rock2"])                # plinth
    s.cyl(lx, lz, 5.2, gy, gy, P["rock3"])
    tower_h = 24
    for y in range(gy + 1, gy + 1 + tower_h):
        frac = (y - gy) / tower_h
        r = 4.2 - frac * 1.3
        band = ((y - gy - 1) // 4) % 2
        s.cyl(lx, lz, r, y, y, P["red"] if band else P["white"])
    top = gy + 1 + tower_h
    # door + small windows
    s.box(lx, lx, gy + 1, gy + 3, lz + 4, lz + 4, P["door"])
    for wy in (gy + 9, gy + 17):
        s.put(lx, wy, lz + 4 - (wy - gy) // 12, P["iron"])
        s.put(lx + 4 - (wy - gy) // 12, wy, lz, P["iron"])
    # gallery
    s.cyl(lx, lz, 4.6, top, top, P["iron"])
    s.cyl(lx, lz, 4.6, top + 1, top + 1, P["iron"], hollow=True)
    # lamp room
    s.cyl(lx, lz, 2.4, top + 1, top + 4, lambda x, y, z:
          P["iron"] if (abs(x) == abs(z) and x != 0) else P["glass"])
    s.cyl(lx, lz, 3.0, top + 5, top + 5, P["red_dk"])
    s.cyl(lx, lz, 2.2, top + 6, top + 6, P["red_dk"])
    s.cyl(lx, lz, 1.2, top + 7, top + 7, P["red_dk"])
    s.put(lx, top + 8, lz, P["iron"])
    s.put(lx, top + 9, lz, P["iron"])

    # -- keeper's cottage on the meadow, path up toward the lighthouse -----
    spot = find_flat(height, 9, 7, SEA + 3, SEA + 5, (34, 34), N)
    hx, hz, hy = spot
    for x in range(hx - 1, hx + 10):
        for z in range(hz - 1, hz + 8):
            for y in range(height[(x, z)] + 1, hy + 1):
                s.put(x, y, z, P["dirt"] if y < hy else P["grass"])
    step = cottage(s, P, hx, hz, hy)
    px, pz = step
    while pz < N - 1:
        g = ground(px, pz) - 1
        if s.get(px, g, pz) in (P["sand"], P["sand2"]) or height[(px, pz)] < SEA:
            break
        s.put(px, g, pz, P["sand2"])
        s.put(px + 1, g, pz, P["sand2"])
        pz += 1

    # -- pier off the +x shore with a moored sailboat ----------------------
    pz_row = cz + 6
    shore = max(x for x in range(N) if height[(x, pz_row)] >= SEA)
    pier(s, P, shore - 2, min(N - 2, shore + 9), pz_row, SEA + 1)
    open_water = [
        (x, z) for x in range(6, N - 6) for z in range(3, N - 3)
        if all(height[(x + dx, z + dz)] < SEA - 1 for dx in range(-6, 7) for dz in range(-3, 4))
    ]
    bx, bz = min(open_water, key=lambda p: (p[0] - shore - 5) ** 2 + (p[1] - pz_row - 4) ** 2)
    sailboat(s, P, bx, bz, SEA)

    # -- surface detail on open water ---------------------------------------
    for _ in range(40):
        x, z = rng.randint(0, N - 4), rng.randint(0, N - 1)
        if all(height[(x + k, z)] < SEA - 1 and s.get(x + k, SEA - 1, z) == P["water"]
               for k in range(3)):
            for k in range(rng.randint(2, 3)):
                s.put(x + k, SEA - 1, z, P["wave"])

    # -- pines & flowers ---------------------------------------------------
    def pine(x, z, h):
        g = ground(x, z)
        s.box(x, x, g, g + 2, z, z, P["trunk"])
        for i in range(h):
            r = (h - i) * 0.5 + 0.5
            y = g + 2 + i
            for dx in range(-3, 4):
                for dz in range(-3, 4):
                    if math.hypot(dx, dz) <= r and (i % 3 != 2 or math.hypot(dx, dz) <= r - 1):
                        s.put(x + dx, y, z + dz, P["pine"] if (dx + dz + i) % 3 else P["pine2"])
        s.put(x, g + 2 + h, z, P["pine"])

    for (x, z, h) in ((14, 33, 8), (18, 38, 6), (40, 14, 7), (12, 26, 5), (26, 40, 6)):
        if height[(x, z)] > SEA + 1:
            pine(x, z, h)

    for _ in range(70):
        x, z = rng.randint(8, N - 8), rng.randint(8, N - 8)
        g = ground(x, z)
        if s.get(x, g - 1, z) in (P["grass"], P["grass2"], P["grass3"]):
            s.put(x, g, z, P["flower_y"] if rng.random() < 0.6 else P["flower_p"])

    # rocks in the surf
    for (x, z) in ((54, 20), (8, 50), (57, 46)):
        s.ellipsoid(x, SEA - 1, z, 2.2, 2.0, 1.8,
                    lambda a, b, c: P["rock"] if b < 1 else P["rock3"])

    s.recenter()
    return s


if __name__ == "__main__":
    s = build()
    print(len(s.v), s.bounds())
    s.render(sys.argv[1] if len(sys.argv) > 1 else "lighthouse.png")
