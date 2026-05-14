use bevy::prelude::*;

/// Storage upper bound. Cell array is always allocated at this size; the
/// active editable box is `VoxelGrid.size`, which the user can pick when
/// starting a new project. Keeping storage fixed avoids reallocating the
/// 10 MB cell array (and re-spawning chunk entities) on resize.
pub const MAX_GRID: usize = 128;
#[allow(dead_code)]
pub const MAX_GRID_I: i32 = MAX_GRID as i32;

/// Chunk edge length. Must divide every legal `VoxelGrid.size`.
pub const CHUNK: usize = 32;
pub const MAX_CHUNKS_PER_AXIS: usize = MAX_GRID / CHUNK;
const _: () = assert!(
    MAX_GRID % CHUNK == 0,
    "MAX_GRID must be a multiple of CHUNK"
);

/// Sizes offered by the New-project dialog. Each must divide CHUNK evenly so
/// the chunked mesher needs no partial-chunk handling.
pub const ALLOWED_SIZES: [usize; 4] = [32, 64, 96, 128];
pub const DEFAULT_SIZE: usize = 32;

pub type Color8 = [u8; 4];

/// X-major flat index into the MAX_GRID³ cell array.
#[inline]
pub fn flat_idx(x: usize, y: usize, z: usize) -> usize {
    x * MAX_GRID * MAX_GRID + y * MAX_GRID + z
}

/// X-major flat index into a MAX_CHUNKS_PER_AXIS³ chunk-flag array.
#[inline]
pub fn chunk_flat_idx(cx: usize, cy: usize, cz: usize) -> usize {
    cx * MAX_CHUNKS_PER_AXIS * MAX_CHUNKS_PER_AXIS + cy * MAX_CHUNKS_PER_AXIS + cz
}

/// State for the New-project modal. `dialog_open` toggles visibility,
/// `picker_size` carries the currently-selected radio button, `apply` is set
/// when the user confirms — picked up by `apply_new_project_system` to
/// reshape the scene on the next frame.
#[derive(Resource)]
pub struct NewProject {
    pub dialog_open: bool,
    pub picker_size: usize,
    pub apply: Option<usize>,
}

impl Default for NewProject {
    fn default() -> Self {
        Self {
            dialog_open: false,
            picker_size: DEFAULT_SIZE,
            apply: None,
        }
    }
}

#[derive(Resource)]
pub struct VoxelGrid {
    pub cells: Box<[Option<Color8>]>,
    pub dirty: bool,
    /// Per-chunk dirty flag, indexed by `chunk_flat_idx`. Always sized for
    /// the max grid; chunks outside `[0, chunks_per_axis())` stay empty.
    pub chunk_dirty: Box<[bool]>,
    /// Active edit box edge length. Cells outside `[0, size)` are rejected
    /// by `set`/`get`. Always a member of `ALLOWED_SIZES`.
    pub size: usize,
}

impl Default for VoxelGrid {
    fn default() -> Self {
        let cells: Box<[Option<Color8>]> =
            vec![None; MAX_GRID * MAX_GRID * MAX_GRID].into_boxed_slice();
        let chunk_dirty: Box<[bool]> = vec![true; MAX_CHUNKS_PER_AXIS.pow(3)].into_boxed_slice();
        Self {
            cells,
            dirty: true,
            chunk_dirty,
            size: DEFAULT_SIZE,
        }
    }
}

impl VoxelGrid {
    #[inline]
    pub fn size_i(&self) -> i32 {
        self.size as i32
    }

    #[inline]
    pub fn chunks_per_axis(&self) -> usize {
        self.size / CHUNK
    }

    #[inline]
    pub fn in_bounds(&self, p: IVec3) -> bool {
        let s = self.size_i();
        p.x >= 0 && p.y >= 0 && p.z >= 0 && p.x < s && p.y < s && p.z < s
    }

    #[inline]
    pub fn get(&self, p: IVec3) -> Option<Color8> {
        if !self.in_bounds(p) {
            return None;
        }
        self.cells[flat_idx(p.x as usize, p.y as usize, p.z as usize)]
    }

    #[inline]
    pub fn set(&mut self, p: IVec3, c: Option<Color8>) {
        if !self.in_bounds(p) {
            return;
        }
        let (x, y, z) = (p.x as usize, p.y as usize, p.z as usize);
        self.cells[flat_idx(x, y, z)] = c;
        self.dirty = true;
        self.mark_chunk_dirty(x, y, z);
    }

    fn mark_chunk_dirty(&mut self, x: usize, y: usize, z: usize) {
        let cpa = self.chunks_per_axis();
        let (cx, cy, cz) = (x / CHUNK, y / CHUNK, z / CHUNK);
        self.chunk_dirty[chunk_flat_idx(cx, cy, cz)] = true;
        // Boundary cells affect the neighbour chunk's face-occlusion across
        // the chunk seam — that chunk's mesh must rebuild too.
        if x % CHUNK == 0 && cx > 0 {
            self.chunk_dirty[chunk_flat_idx(cx - 1, cy, cz)] = true;
        }
        if x % CHUNK == CHUNK - 1 && cx + 1 < cpa {
            self.chunk_dirty[chunk_flat_idx(cx + 1, cy, cz)] = true;
        }
        if y % CHUNK == 0 && cy > 0 {
            self.chunk_dirty[chunk_flat_idx(cx, cy - 1, cz)] = true;
        }
        if y % CHUNK == CHUNK - 1 && cy + 1 < cpa {
            self.chunk_dirty[chunk_flat_idx(cx, cy + 1, cz)] = true;
        }
        if z % CHUNK == 0 && cz > 0 {
            self.chunk_dirty[chunk_flat_idx(cx, cy, cz - 1)] = true;
        }
        if z % CHUNK == CHUNK - 1 && cz + 1 < cpa {
            self.chunk_dirty[chunk_flat_idx(cx, cy, cz + 1)] = true;
        }
    }

    /// Bounds-check-free read for callers that already iterate over
    /// `0..self.size`.
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

    /// Reset to an empty grid at a new size. Caller must redraw the floor /
    /// walls and recenter the camera after this returns.
    pub fn resize(&mut self, new_size: usize) {
        debug_assert!(
            ALLOWED_SIZES.contains(&new_size),
            "new_size {new_size} not in ALLOWED_SIZES"
        );
        self.clear();
        self.size = new_size;
    }

    pub fn count(&self) -> usize {
        self.cells.iter().filter(|c| c.is_some()).count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_size_is_32() {
        let g = VoxelGrid::default();
        assert_eq!(g.size, 32);
        assert_eq!(g.size_i(), 32);
        assert_eq!(g.chunks_per_axis(), 1);
    }

    #[test]
    fn in_bounds_inclusive_zero_exclusive_size() {
        let g = VoxelGrid::default();
        assert!(g.in_bounds(IVec3::ZERO));
        assert!(g.in_bounds(IVec3::new(g.size_i() - 1, g.size_i() - 1, g.size_i() - 1)));
        assert!(!g.in_bounds(IVec3::new(-1, 0, 0)));
        assert!(!g.in_bounds(IVec3::new(0, g.size_i(), 0)));
        assert!(!g.in_bounds(IVec3::new(0, 0, g.size_i())));
    }

    #[test]
    fn in_bounds_rejects_cells_outside_active_box_even_inside_storage() {
        // Default size is 32; storage is 128. Cell at (50, 0, 0) is valid in
        // storage but outside the active box.
        let g = VoxelGrid::default();
        assert!(!g.in_bounds(IVec3::new(50, 0, 0)));
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
        g.set(IVec3::new(g.size_i(), 0, 0), Some([1, 1, 1, 255]));
        assert!(!g.dirty);
        assert_eq!(g.count(), 0);
    }

    #[test]
    fn get_out_of_bounds_returns_none() {
        let g = VoxelGrid::default();
        assert_eq!(g.get(IVec3::new(-1, 0, 0)), None);
        assert_eq!(g.get(IVec3::new(0, g.size_i(), 0)), None);
    }

    #[test]
    fn resize_clears_and_updates_size() {
        let mut g = VoxelGrid::default();
        g.set(IVec3::new(1, 1, 1), Some([1, 1, 1, 255]));
        assert_eq!(g.count(), 1);
        g.resize(64);
        assert_eq!(g.size, 64);
        assert_eq!(g.count(), 0);
        // Cells previously out of bounds (e.g. 40, 40, 40) are now reachable.
        g.set(IVec3::new(40, 40, 40), Some([2, 2, 2, 255]));
        assert_eq!(g.get(IVec3::new(40, 40, 40)), Some([2, 2, 2, 255]));
    }

    #[test]
    fn set_marks_owning_chunk_dirty() {
        let mut g = VoxelGrid::default();
        g.resize(128);
        for d in g.chunk_dirty.iter_mut() {
            *d = false;
        }
        // Cell at (5, 5, 5) → chunk (0, 0, 0).
        g.set(IVec3::new(5, 5, 5), Some([1, 1, 1, 255]));
        assert!(g.chunk_dirty[chunk_flat_idx(0, 0, 0)]);
    }

    #[test]
    fn set_marks_neighbor_chunk_dirty_on_boundary() {
        let mut g = VoxelGrid::default();
        g.resize(128);
        for d in g.chunk_dirty.iter_mut() {
            *d = false;
        }
        // Cell at x = CHUNK-1: last column of chunk (0,*,*). Neighbour (1,0,0)
        // should also flag — its face-occlusion across the X seam changed.
        let p = IVec3::new((CHUNK - 1) as i32, 5, 5);
        g.set(p, Some([1, 1, 1, 255]));
        assert!(g.chunk_dirty[chunk_flat_idx(0, 0, 0)]);
        assert!(g.chunk_dirty[chunk_flat_idx(1, 0, 0)]);
    }

    #[test]
    fn set_does_not_mark_distant_chunks_dirty() {
        let mut g = VoxelGrid::default();
        g.resize(128);
        for d in g.chunk_dirty.iter_mut() {
            *d = false;
        }
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
