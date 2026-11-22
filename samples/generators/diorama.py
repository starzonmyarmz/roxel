"""River diorama. Usage: python3 diorama.py | rox apply --new - -o diorama.rox"""

from voxlib import Scene

P = {
    "grass": "#7ec63e", "grass1": "#84cc44", "grass2": "#78c03a", "grass3": "#92d454",
    "grass_dk": "#60a830", "soil": "#f6a82e", "soil_dk": "#e08424",
    "water": "#42acc6", "water_deep": "#287ca8", "foam": "#e8f4f6", "sand": "#d6bc84",
    "trunk": "#603e28",
    "leaves1": "#a0d25c", "leaves2": "#7cbe3c", "leaves3": "#589630", "leaves4": "#6cb238",
    "flower": "#f4f4f4", "tuft": "#54922e",
}
GRASS = [P["grass1"], P["grass2"], P["grass3"]]
LEAVES = [P["leaves1"], P["leaves2"], P["leaves3"], P["leaves4"]]

PLOT = 48
GRASS_Y = 11
RIVER_Z0, RIVER_Z1 = 26, 34


def build():
    s = Scene(seed=7)
    rng = s.rng

    s.box(0, PLOT - 1, 0, 1, 0, PLOT - 1, P["soil_dk"])
    s.box(0, PLOT - 1, 2, GRASS_Y - 2, 0, PLOT - 1, P["soil"])

    tex0, tex1 = RIVER_Z0 - 4, RIVER_Z1 + 4

    def grass_color(z):
        if tex0 <= z <= tex1:
            r = rng.random()
            if r < 0.07:
                return P["grass_dk"]
            if r < 0.28:
                return rng.choice(GRASS)
            return P["grass"]
        if rng.random() < 0.04:
            return rng.choice(GRASS)
        return P["grass"]

    for x in range(PLOT):
        for z in range(PLOT):
            s.put(x, GRASS_Y, z, grass_color(z))
            s.put(x, GRASS_Y - 1, z, grass_color(z))

    def edge_overgrowth(cells, skip):
        """Ragged grass runs spilling down the soil sides, with a lip past the edge."""
        i, n = 0, len(cells)
        while i < n:
            rx, rz, _, _ = cells[i]
            if skip(rx, rz) or rng.random() < 0.35:
                i += 1
                continue
            run = rng.randint(2, 5)
            base_depth = rng.randint(2, 5)
            for j in range(run):
                if i + j >= n:
                    break
                rx, rz, ox, oz = cells[i + j]
                if skip(rx, rz):
                    break
                d = max(1, base_depth + rng.randint(-1, 1))
                for k in range(d):
                    s.put(rx, (GRASS_Y - 2) - k, rz, rng.choice(GRASS))
                if rng.random() < 0.5:
                    g = rng.choice(GRASS)
                    s.put(ox, GRASS_Y, oz, g)
                    s.put(ox, GRASS_Y - 1, oz, g)
                    if rng.random() < 0.5:
                        s.put(ox, GRASS_Y - 2, oz, g)
            i += run + rng.randint(1, 2)

    def on_river_edge(rx, rz):
        return RIVER_Z0 <= rz <= RIVER_Z1

    def never(rx, rz):
        return False

    edge_overgrowth([(0, i, -1, i) for i in range(PLOT)], on_river_edge)
    edge_overgrowth([(PLOT - 1, i, PLOT, i) for i in range(PLOT)], on_river_edge)
    edge_overgrowth([(i, 0, i, -1) for i in range(PLOT)], never)
    edge_overgrowth([(i, PLOT - 1, i, PLOT) for i in range(PLOT)], never)

    water_top = GRASS_Y - 1
    wb0, wb1 = RIVER_Z0 + 1, RIVER_Z1 - 1
    for x in range(PLOT):
        for z in range(wb0, wb1 + 1):
            s.clear(x, GRASS_Y, z)
            s.put(x, water_top, z, P["water"])
            s.put(x, water_top - 1, z, P["water_deep"])
            s.put(x, water_top - 2, z, P["sand"])
        s.put(x, water_top, RIVER_Z0, P["sand"])
        s.put(x, water_top, RIVER_Z1, P["sand"])
    for _ in range(12):
        z = rng.randint(wb0, wb1)
        sx = rng.randint(0, PLOT - 5)
        for k in range(rng.randint(2, 4)):
            if rng.random() < 0.85:
                s.put(sx + k, water_top, z, P["foam"])

    # The waterfall hangs one voxel past the +x face, outside the plot.
    fall_x = PLOT
    foam_cols = set(rng.sample(range(wb0, wb1 + 1), 3))
    for z in range(wb0, wb1 + 1):
        s.put(PLOT - 1, water_top, z, P["foam"])
        s.put(fall_x, water_top, z, P["foam"])
        for y in range(0, water_top):
            c = P["water"] if rng.random() < 0.5 else P["water_deep"]
            if z in foam_cols and rng.random() < 0.55:
                c = P["foam"]
            elif rng.random() < 0.06:
                c = P["foam"]
            s.put(fall_x, y, z, c)

    for x in range(PLOT, PLOT + 3):
        for z in range(wb0 - 1, wb1 + 2):
            if x - PLOT <= 1 or rng.random() < 0.55:
                s.put(x, 0, z, P["water"])
    for z in range(wb0 - 1, wb1 + 2):
        s.put(PLOT, 0, z, P["foam"])
        if rng.random() < 0.5:
            s.put(PLOT, 1, z, P["foam"])
    for x in range(PLOT, PLOT + 3):
        if rng.random() < 0.7:
            s.put(x, 0, wb0 - 1, P["foam"])
        if rng.random() < 0.7:
            s.put(x, 0, wb1 + 1, P["foam"])

    def tree(tx, tz, fh):
        base = GRASS_Y + 1
        trunk_h = rng.randint(3, 5)
        for y in range(base, base + trunk_h):
            s.put(tx, y, tz, P["trunk"])
        fy0 = base + trunk_h - 1
        for i in range(fh):
            y = fy0 + i
            for dx in (-1, 0, 1):
                for dz in (-1, 0, 1):
                    if abs(dx) + abs(dz) == 2 and rng.random() < 0.45:
                        continue
                    if i >= fh - 2 and (dx or dz) and rng.random() < 0.55:
                        continue
                    if rng.random() < 0.08:
                        continue
                    s.put(tx + dx, y, tz + dz, rng.choice(LEAVES))
            if rng.random() < 0.28:
                ndx, ndz = rng.choice([(2, 0), (-2, 0), (0, 2), (0, -2)])
                s.put(tx + ndx, y, tz + ndz, rng.choice(LEAVES))
        s.put(tx, fy0 + fh, tz, rng.choice(LEAVES))

    tree(12, 11, 13)
    tree(26, 12, 11)
    tree(43, 15, 12)
    tree(11, 42, 11)
    tree(45, 44, 12)

    def on_river(x, z):
        return RIVER_Z0 - 1 <= z <= RIVER_Z1 + 1

    for _ in range(16):
        x = rng.randint(0, PLOT - 1)
        z = rng.randint(0, PLOT - 1)
        if on_river(x, z):
            continue
        s.put(x, GRASS_Y + 1, z, P["flower"])

    for _ in range(8):
        x = rng.randint(0, PLOT - 1)
        z = rng.randint(0, PLOT - 1)
        if on_river(x, z):
            continue
        s.put(x, GRASS_Y + 1, z, P["tuft"])
        if rng.random() < 0.5:
            s.put(x + 1, GRASS_Y + 1, z, P["tuft"])

    return s


if __name__ == "__main__":
    build().emit(P)
