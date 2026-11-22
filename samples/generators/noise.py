import math
import random


class ValueNoise2D:
    def __init__(self, seed):
        self.r = random.Random(seed)
        self.cache = {}

    def _v(self, ix, iz):
        k = (ix, iz)
        if k not in self.cache:
            self.cache[k] = self.r.random()
        return self.cache[k]

    def at(self, x, z):
        ix, iz = math.floor(x), math.floor(z)
        fx, fz = x - ix, z - iz
        sx, sz = fx * fx * (3 - 2 * fx), fz * fz * (3 - 2 * fz)
        a = self._v(ix, iz) + (self._v(ix + 1, iz) - self._v(ix, iz)) * sx
        b = self._v(ix, iz + 1) + (self._v(ix + 1, iz + 1) - self._v(ix, iz + 1)) * sx
        return a + (b - a) * sz

    def fbm(self, x, z, octaves=3):
        t, amp, f, norm = 0.0, 1.0, 1.0, 0.0
        for o in range(octaves):
            t += amp * self.at(x * f + o * 17.3, z * f + o * 9.1)
            norm += amp
            amp *= 0.5
            f *= 2.0
        return t / norm
