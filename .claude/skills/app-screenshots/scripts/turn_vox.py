#!/usr/bin/env python3
"""Turn a single-model MagicaVoxel .vox by quarter turns around the vertical
axis. Roxel Shot always renders from the same isometric corner, so to show
a model's front, export from a turned copy:

    rox convert scene.rox /tmp/scene.vox
    turn_vox.py /tmp/scene.vox /tmp/scene-t2.vox 2
    rox convert /tmp/scene-t2.vox /tmp/scene-t2.rox

Each turn matches one `rox render --view` step.
"""

import struct
import sys


def chunks(buf, pos, end):
    while pos < end:
        cid, n, m = struct.unpack_from("<4sii", buf, pos)
        yield cid, buf[pos + 12:pos + 12 + n], buf[pos + 12 + n:pos + 12 + n + m]
        pos += 12 + n + m


def chunk(cid, content=b"", children=b""):
    return struct.pack("<4sii", cid, len(content), len(children)) + content + children


def main():
    src, dst, turns = sys.argv[1], sys.argv[2], int(sys.argv[3]) % 4
    buf = open(src, "rb").read()
    assert buf[:4] == b"VOX "
    _, main_content, main_children = next(chunks(buf, 8, len(buf)))
    parts = list(chunks(main_children, 0, len(main_children)))
    if sum(1 for c, _, _ in parts if c == b"SIZE") != 1:
        sys.exit("only single-model .vox files are supported")

    sx, sy, sz = next(struct.unpack("<3i", c) for i, c, _ in parts if i == b"SIZE")
    xyzi = next(c for i, c, _ in parts if i == b"XYZI")
    n = struct.unpack_from("<i", xyzi)[0]
    vox = [struct.unpack_from("<4B", xyzi, 4 + 4 * k) for k in range(n)]
    for _ in range(turns):
        vox = [(sy - 1 - y, x, z, i) for x, y, z, i in vox]
        sx, sy = sy, sx

    out = b""
    for cid, content, children in parts:
        if cid == b"SIZE":
            content = struct.pack("<3i", sx, sy, sz)
        elif cid == b"XYZI":
            content = struct.pack("<i", n) + b"".join(struct.pack("<4B", *v) for v in vox)
        out += chunk(cid, content, children)
    with open(dst, "wb") as f:
        f.write(buf[:8] + chunk(b"MAIN", main_content, out))


if __name__ == "__main__":
    main()
