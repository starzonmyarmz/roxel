import math
import sys

from voxlib import Scene, hexc

P = {
    "grass": hexc("8ccf5e"), "grass2": hexc("78bd4e"), "grass3": hexc("a3dc72"),
    "soil": hexc("9a6a44"), "soil2": hexc("7d5436"), "root": hexc("5f3f28"),
    "stem": hexc("f3e6c8"), "stem2": hexc("e6d4ae"), "stem3": hexc("d8c39a"),
    "cap": hexc("e0483e"), "cap2": hexc("c63a33"), "cap_dk": hexc("9e2c2a"),
    "spot": hexc("fbf3e4"), "gill": hexc("e9c9a0"),
    "door": hexc("7a4a2c"), "door2": hexc("5f381f"), "knob": hexc("f2c14e"),
    "window": hexc("ffe08a"), "frame": hexc("7a4a2c"),
    "stone": hexc("b9b2a6"), "stone2": hexc("9d968a"),
    "fence": hexc("c99b66"), "lantern": hexc("ffd25e"), "iron": hexc("3a3f4b"),
    "flower_r": hexc("ff7a8a"), "flower_b": hexc("7ab8ff"), "flower_y": hexc("ffd34d"),
    "leaf": hexc("4f9a48"), "pond": hexc("5fb7e0"), "pond2": hexc("8fd3ee"),
    "small_cap": hexc("f29b38"), "small_cap2": hexc("8a6ad8"),
}


def build():
    s = Scene(seed=3)
    rng = s.rng
    R = 22  # round plot radius
    G = 6   # grass top y

    # round floating plot: grass cap, soil body tapering into a root tip
    for x in range(-R, R + 1):
        for z in range(-R, R + 1):
            d = math.hypot(x, z)
            if d > R + 0.3:
                continue
            bottom = int((d / R) ** 1.6 * G)
            for y in range(bottom, G + 1):
                if y == G:
                    r = rng.random()
                    c = P["grass"] if r < 0.7 else (P["grass2"] if r < 0.9 else P["grass3"])
                elif y == G - 1 and d > R - 1.5:
                    c = P["grass2"]
                else:
                    c = P["soil"] if (x * 7 + y * 3 + z * 5) % 9 else P["soil2"]
                s.put(x, y, z, c)
    # dangling roots on the visible rim
    for _ in range(26):
        a = rng.uniform(0, math.pi * 2)
        rx, rz = int(math.cos(a) * (R - 1)), int(math.sin(a) * (R - 1))
        y = int((math.hypot(rx, rz) / R) ** 1.6 * G) - 1
        for k in range(rng.randint(1, 3)):
            s.put(rx, y - k, rz, P["root"])

    # small pond toward the front-left
    for x in range(-14, -5):
        for z in range(6, 15):
            if ((x + 9.5) / 4.6) ** 2 + ((z - 10) / 4.0) ** 2 <= 1:
                s.put(x, G, z, P["pond"] if rng.random() < 0.85 else P["pond2"])
                s.put(x, G - 1, z, P["pond"])
    for (x, z) in ((-11, 9), (-8, 11)):
        s.put(x, G, z, P["leaf"])
        s.put(x + 1, G, z, P["leaf"])

    # -- the mushroom house ------------------------------------------------
    hx, hz = 2, -3
    stem_h = 17
    for y in range(G + 1, G + 1 + stem_h):
        t = (y - G) / stem_h
        r = 6.2 - 1.6 * math.sin(t * math.pi * 0.9)   # waisted stem
        s.cyl(hx, hz, r, y, y, lambda a, b, c: P["stem"] if (a * 2 + c + b // 3) % 7 else P["stem2"])
    # flared base
    s.cyl(hx, hz, 7.2, G + 1, G + 1, P["stem3"])
    s.cyl(hx, hz, 6.8, G + 2, G + 2, P["stem2"])

    # cap: squashed dome with an overhanging lip and gills underneath
    cap_y = G + stem_h - 1
    CR, CH = 10.5, 8.5
    for x in range(-11, 12):
        for z in range(-11, 12):
            for y in range(0, 10):
                d = (x / CR) ** 2 + (y / CH) ** 2 + (z / CR) ** 2
                if d <= 1.0:
                    c = P["cap_dk"] if y == 0 else (P["cap2"] if y == 1 else P["cap"])
                    s.put(hx + x, cap_y + y, hz + z, c)
    for x in range(-11, 12):
        for z in range(-11, 12):
            if math.hypot(x, z) <= CR - 0.3 and math.hypot(x, z) > 5.0:
                s.put(hx + x, cap_y - 1, hz + z, P["gill"])
    # white spots on the dome surface
    for _ in range(26):
        th = rng.uniform(0, math.pi * 2)
        ph = rng.uniform(0.15, 1.25)
        cx = hx + int(math.cos(th) * math.sin(ph) * (CR + 0.1))
        cz = hz + int(math.sin(th) * math.sin(ph) * (CR + 0.1))
        cy = cap_y + int(math.cos(ph) * (CH + 0.1))
        rad = rng.choice((1, 1, 2))
        for dx in range(-rad, rad + 1):
            for dy in range(-rad, rad + 1):
                for dz in range(-rad, rad + 1):
                    if dx * dx + dy * dy + dz * dz <= rad * rad + 0.5:
                        p = (cx + dx, cy + dy, cz + dz)
                        if s.get(*p) in (P["cap"], P["cap2"]):
                            s.put(*p, P["spot"])

    # door on +z (camera-left) face with arched top
    dz = hz + 5
    for y in range(G + 2, G + 9):
        for x in range(hx - 2, hx + 3):
            if y == G + 8 and abs(x - hx) == 2:
                continue
            s.put(x, y, dz, P["door"] if (x - hx) % 2 == 0 else P["door2"])
            s.clear(x, y, dz + 1)
    s.put(hx + 1, G + 5, dz + 1, P["knob"])
    # round windows on +z and +x faces, lit
    def round_window(px, py, pz, axis):
        for a in range(-2, 3):
            for b in range(-2, 3):
                d = math.hypot(a, b)
                if d > 2.4:
                    continue
                c = P["window"] if d < 1.5 else P["frame"]
                if axis == "z":
                    s.put(px + a, py + b, pz, c)
                else:
                    s.put(px, py + b, pz + a, c)
    def stem_r(y):
        return 6.2 - 1.6 * math.sin((y - G) / stem_h * math.pi * 0.9)
    round_window(hx + int(stem_r(G + 7)), G + 7, hz - 1, "x")
    round_window(hx - 4, G + 9, hz + int(stem_r(G + 9)), "z")
    # chimney poking out of the cap
    s.box(hx + 6, hx + 7, cap_y + 6, cap_y + 11, hz - 4, hz - 3, P["stone2"])
    s.box(hx + 5, hx + 8, cap_y + 12, cap_y + 12, hz - 5, hz - 2, P["stone"])

    # stepping-stone path from the door toward the front edge
    for i, (x, z) in enumerate(((hx, hz + 8), (hx + 1, hz + 11), (hx, hz + 14),
                                (hx + 2, hz + 17), (hx + 1, hz + 20))):
        for a in range(-1, 2):
            for b in range(-1, 1):
                if not (a and b and rng.random() < 0.5):
                    s.put(x + a, G, z + b, P["stone"] if (a + b + i) % 2 else P["stone2"])

    # lantern post beside the path
    lx, lz = hx - 5, hz + 11
    s.box(lx, lx, G + 1, G + 7, lz, lz, P["iron"])
    s.box(lx - 1, lx + 1, G + 8, G + 9, lz - 1, lz + 1, P["lantern"])
    s.box(lx - 1, lx + 1, G + 10, G + 10, lz - 1, lz + 1, P["iron"])

    # little mushrooms
    def mini(x, z, h, r, c):
        s.box(x, x, G + 1, G + h, z, z, P["stem"])
        s.cyl(x, z, r, G + h + 1, G + h + 1, c)
        s.cyl(x, z, max(0.5, r - 1), G + h + 2, G + h + 2, c)
    mini(14, 4, 3, 2.2, P["small_cap"])
    mini(16, 8, 2, 1.5, P["small_cap"])
    mini(-12, -10, 4, 2.5, P["small_cap2"])
    mini(-9, -14, 2, 1.5, P["small_cap2"])
    mini(9, 15, 2, 1.4, P["cap"])

    # picket fence arc around the back
    for deg in range(110, 340, 6):
        a = math.radians(deg)
        x, z = int(math.cos(a) * (R - 2)), int(math.sin(a) * (R - 2))
        s.box(x, x, G + 1, G + 3, z, z, P["fence"])

    # flowers
    for _ in range(45):
        x, z = rng.randint(-R + 2, R - 2), rng.randint(-R + 2, R - 2)
        if math.hypot(x, z) < R - 1.5 and s.get(x, G, z) in (P["grass"], P["grass2"], P["grass3"]) \
                and s.get(x, G + 1, z) is None:
            s.put(x, G + 1, z, rng.choice((P["flower_r"], P["flower_b"], P["flower_y"], P["leaf"])))

    s.recenter()
    return s


if __name__ == "__main__":
    s = build()
    print(len(s.v), s.bounds())
    s.render(sys.argv[1] if len(sys.argv) > 1 else "mushroom.png", tile=10)
