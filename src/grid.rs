use bevy::prelude::*;

pub const GRID: usize = 128;
pub const GRID_I: i32 = GRID as i32;

/// Chunk edge length. Must divide GRID evenly. The mesher rebuilds one chunk
/// at a time so a single-cell edit doesn't re-walk the whole grid.
pub const CHUNK: usize = 32;
pub const CHUNKS_PER_AXIS: usize = GRID / CHUNK;
const _: () = assert!(GRID % CHUNK == 0, "GRID must be a multiple of CHUNK");

pub type Color8 = [u8; 4];

/// X-major flat index into a GRID³ cell array. Heap-allocated via `Vec` so
/// construction never touches the stack (a `Box::new([[[None; G]; G]; G])`
/// builds the array on the stack first; at GRID=128 that's ~10 MB and will
/// overflow the main-thread stack).
#[inline]
pub fn flat_idx(x: usize, y: usize, z: usize) -> usize {
    x * GRID * GRID + y * GRID + z
}

/// X-major flat index into a CHUNKS_PER_AXIS³ chunk array.
#[inline]
pub fn chunk_flat_idx(cx: usize, cy: usize, cz: usize) -> usize {
    cx * CHUNKS_PER_AXIS * CHUNKS_PER_AXIS + cy * CHUNKS_PER_AXIS + cz
}

#[derive(Resource)]
pub struct VoxelGrid {
    pub cells: Box<[Option<Color8>]>,
    pub dirty: bool,
    /// Per-chunk dirty flag. `set` flips the owning chunk and its neighbours
    /// when the modified cell is on a chunk-boundary face. Consumed by
    /// `regenerate_mesh_system` to rebuild only changed chunks.
    pub chunk_dirty: Box<[bool]>,
}

impl Default for VoxelGrid {
    fn default() -> Self {
        let cells: Box<[Option<Color8>]> =
            vec![None; GRID * GRID * GRID].into_boxed_slice();
        let chunk_dirty: Box<[bool]> =
            vec![true; CHUNKS_PER_AXIS.pow(3)].into_boxed_slice();
        Self { cells, dirty: true, chunk_dirty }
    }
}

impl VoxelGrid {
    #[inline]
    pub fn in_bounds(p: IVec3) -> bool {
        p.x >= 0 && p.y >= 0 && p.z >= 0 && p.x < GRID_I && p.y < GRID_I && p.z < GRID_I
    }

    #[inline]
    pub fn get(&self, p: IVec3) -> Option<Color8> {
        if !Self::in_bounds(p) {
            return None;
        }
        self.cells[flat_idx(p.x as usize, p.y as usize, p.z as usize)]
    }

    #[inline]
    pub fn set(&mut self, p: IVec3, c: Option<Color8>) {
        if !Self::in_bounds(p) {
            return;
        }
        let (x, y, z) = (p.x as usize, p.y as usize, p.z as usize);
        self.cells[flat_idx(x, y, z)] = c;
        self.dirty = true;
        self.mark_chunk_dirty(x, y, z);
    }

    fn mark_chunk_dirty(&mut self, x: usize, y: usize, z: usize) {
        let (cx, cy, cz) = (x / CHUNK, y / CHUNK, z / CHUNK);
        self.chunk_dirty[chunk_flat_idx(cx, cy, cz)] = true;
        // Boundary cells affect the neighbour chunk's face-occlusion across
        // the chunk seam — that chunk's mesh must rebuild too.
        if x % CHUNK == 0 && cx > 0 {
            self.chunk_dirty[chunk_flat_idx(cx - 1, cy, cz)] = true;
        }
        if x % CHUNK == CHUNK - 1 && cx + 1 < CHUNKS_PER_AXIS {
            self.chunk_dirty[chunk_flat_idx(cx + 1, cy, cz)] = true;
        }
        if y % CHUNK == 0 && cy > 0 {
            self.chunk_dirty[chunk_flat_idx(cx, cy - 1, cz)] = true;
        }
        if y % CHUNK == CHUNK - 1 && cy + 1 < CHUNKS_PER_AXIS {
            self.chunk_dirty[chunk_flat_idx(cx, cy + 1, cz)] = true;
        }
        if z % CHUNK == 0 && cz > 0 {
            self.chunk_dirty[chunk_flat_idx(cx, cy, cz - 1)] = true;
        }
        if z % CHUNK == CHUNK - 1 && cz + 1 < CHUNKS_PER_AXIS {
            self.chunk_dirty[chunk_flat_idx(cx, cy, cz + 1)] = true;
        }
    }

    /// Bounds-check-free read for callers that already iterate over `0..GRID`.
    #[inline]
    pub fn cell(&self, x: usize, y: usize, z: usize) -> Option<Color8> {
        self.cells[flat_idx(x, y, z)]
    }

    pub fn clear(&mut self) {
        for c in self.cells.iter_mut() {
            *c = None;
        }
        self.dirty = true;
        for d in self.chunk_dirty.iter_mut() {
            *d = true;
        }
    }

    pub fn count(&self) -> usize {
        self.cells.iter().filter(|c| c.is_some()).count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn in_bounds_inclusive_zero_exclusive_max() {
        assert!(VoxelGrid::in_bounds(IVec3::ZERO));
        assert!(VoxelGrid::in_bounds(IVec3::new(GRID_I - 1, GRID_I - 1, GRID_I - 1)));
        assert!(!VoxelGrid::in_bounds(IVec3::new(-1, 0, 0)));
        assert!(!VoxelGrid::in_bounds(IVec3::new(0, GRID_I, 0)));
        assert!(!VoxelGrid::in_bounds(IVec3::new(0, 0, GRID_I)));
    }

    #[test]
    fn set_get_roundtrip_and_dirty_flag() {
        let mut g = VoxelGrid::default();
        g.dirty = false;
        let c: Color8 = [10, 20, 30, 255];
        g.set(IVec3::new(1, 2, 3), Some(c));
        assert_eq!(g.get(IVec3::new(1, 2, 3)), Some(c));
        assert!(g.dirty);
    }

    #[test]
    fn set_out_of_bounds_is_noop() {
        let mut g = VoxelGrid::default();
        g.dirty = false;
        g.set(IVec3::new(-1, 0, 0), Some([1, 1, 1, 255]));
        g.set(IVec3::new(GRID_I, 0, 0), Some([1, 1, 1, 255]));
        assert!(!g.dirty);
        assert_eq!(g.count(), 0);
    }

    #[test]
    fn get_out_of_bounds_returns_none() {
        let g = VoxelGrid::default();
        assert_eq!(g.get(IVec3::new(-1, 0, 0)), None);
        assert_eq!(g.get(IVec3::new(0, GRID_I, 0)), None);
    }

    #[test]
    fn set_marks_owning_chunk_dirty() {
        let mut g = VoxelGrid::default();
        for d in g.chunk_dirty.iter_mut() { *d = false; }
        // Cell at (5, 5, 5) → chunk (0, 0, 0).
        g.set(IVec3::new(5, 5, 5), Some([1, 1, 1, 255]));
        assert!(g.chunk_dirty[chunk_flat_idx(0, 0, 0)]);
    }

    #[test]
    fn set_marks_neighbor_chunk_dirty_on_boundary() {
        // Skip when chunks-per-axis is 1 — no neighbours exist.
        if CHUNKS_PER_AXIS < 2 { return; }
        let mut g = VoxelGrid::default();
        for d in g.chunk_dirty.iter_mut() { *d = false; }
        // Cell at x = CHUNK-1: last column of chunk (0,*,*). Neighbour (1,0,0)
        // should also flag — its face-occlusion across the X seam changed.
        let p = IVec3::new((CHUNK - 1) as i32, 5, 5);
        g.set(p, Some([1, 1, 1, 255]));
        assert!(g.chunk_dirty[chunk_flat_idx(0, 0, 0)]);
        assert!(g.chunk_dirty[chunk_flat_idx(1, 0, 0)]);
    }

    #[test]
    fn set_does_not_mark_distant_chunks_dirty() {
        if CHUNKS_PER_AXIS < 2 { return; }
        let mut g = VoxelGrid::default();
        for d in g.chunk_dirty.iter_mut() { *d = false; }
        // Middle of chunk (0,0,0): no neighbour should flag.
        g.set(IVec3::new(1, 1, 1), Some([1, 1, 1, 255]));
        assert!(g.chunk_dirty[chunk_flat_idx(0, 0, 0)]);
        assert!(!g.chunk_dirty[chunk_flat_idx(1, 0, 0)]);
    }

    #[test]
    fn count_and_clear() {
        let mut g = VoxelGrid::default();
        g.set(IVec3::new(0, 0, 0), Some([1, 1, 1, 255]));
        g.set(IVec3::new(5, 6, 7), Some([2, 2, 2, 255]));
        assert_eq!(g.count(), 2);
        g.clear();
        assert_eq!(g.count(), 0);
        assert!(g.dirty);
    }
}
