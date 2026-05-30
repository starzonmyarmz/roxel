use bevy::prelude::*;
use std::collections::{HashMap, HashSet};

/// Chunk edge length. Each chunk stores `CHUNK³` cells in a flat array. The
/// world is sparse: chunks allocate on first write and drop when they empty.
pub const CHUNK: usize = 32;
pub const CHUNK_I: i32 = CHUNK as i32;
pub const CHUNK_VOL: usize = CHUNK * CHUNK * CHUNK;

/// Soft perf-warning thresholds. Either crossing fires a one-shot toast.
/// 5 M cells matches the point where greedy meshing starts hurting framerate
/// on typical hardware; 4 K loaded chunks captures the surface-area case
/// (long thin walls) where cell count alone underestimates load.
pub const LARGE_SCENE_CELLS: u32 = 5_000_000;
pub const LARGE_SCENE_CHUNKS: u32 = 4_000;

/// Hysteresis: clear the latched warning only after both counters fall below
/// 80 % of their respective thresholds, so a value oscillating near the line
/// does not re-toast every frame.
const HYSTERESIS_NUM: u32 = 4;
const HYSTERESIS_DEN: u32 = 5;

pub type Color8 = [u8; 4];

/// One 32³ block of voxels. `count` lets us drop the chunk from the map the
/// moment it empties so the open-world `chunks: HashMap<...>` only holds
/// chunks that actually carry data.
pub struct Chunk {
    pub cells: Box<[Option<Color8>]>,
    pub count: u32,
}

impl Chunk {
    fn new() -> Self {
        Self {
            cells: vec![None; CHUNK_VOL].into_boxed_slice(),
            count: 0,
        }
    }
}

/// World-coord → chunk coord (the IVec3 key into `VoxelGrid.chunks`).
/// `div_euclid` keeps the result correct for negative axes (e.g. y always
/// >= 0 in practice but x/z can go negative).
#[inline]
pub fn chunk_coord(p: IVec3) -> IVec3 {
    IVec3::new(
        p.x.div_euclid(CHUNK_I),
        p.y.div_euclid(CHUNK_I),
        p.z.div_euclid(CHUNK_I),
    )
}

#[inline]
fn local_idx(p: IVec3) -> usize {
    let lx = p.x.rem_euclid(CHUNK_I) as usize;
    let ly = p.y.rem_euclid(CHUNK_I) as usize;
    let lz = p.z.rem_euclid(CHUNK_I) as usize;
    lx * CHUNK * CHUNK + ly * CHUNK + lz
}

/// State for the New-project confirm modal. There is no longer a size to
/// pick — the open-world grid has no fixed extent — so this collapses to a
/// dialog-open flag plus an `apply` flag the application reads next frame.
#[derive(Resource, Default)]
pub struct NewProject {
    pub dialog_open: bool,
    pub apply: bool,
}

#[derive(Resource, Default)]
pub struct VoxelGrid {
    /// Sparse storage keyed by chunk coordinate. Allocates on first write,
    /// drops when the chunk's `count` hits zero.
    pub chunks: HashMap<IVec3, Chunk>,
    /// Chunk coords that need a mesh rebuild this frame. Mesher drains.
    pub dirty_chunks: HashSet<IVec3>,
    /// Coarse "anything changed" flag — peers may use this as a cheap remesh
    /// trigger without traversing the dirty set.
    pub dirty: bool,
    /// Total occupied cells across all loaded chunks, kept incrementally.
    pub total_count: u32,
    /// One-shot latch for the large-scene perf warning. Resets when both
    /// counters fall below 80 % of their thresholds.
    pub warned_large: bool,
}

impl VoxelGrid {
    /// The only hard rule in the open-world grid: no cells below the floor.
    /// Kept on `VoxelGrid` (rather than as a free fn) so call sites still
    /// read `grid.in_bounds(p)`.
    #[inline]
    pub fn in_bounds(&self, p: IVec3) -> bool {
        p.y >= 0
    }

    #[inline]
    pub fn get(&self, p: IVec3) -> Option<Color8> {
        if p.y < 0 {
            return None;
        }
        self.chunks.get(&chunk_coord(p))?.cells[local_idx(p)]
    }

    pub fn set(&mut self, p: IVec3, c: Option<Color8>) {
        if p.y < 0 {
            return;
        }
        let coord = chunk_coord(p);
        let idx = local_idx(p);

        let chunk = self.chunks.entry(coord).or_insert_with(Chunk::new);
        let prev = chunk.cells[idx];
        let delta: i32 = match (prev.is_some(), c.is_some()) {
            (false, true) => {
                chunk.count += 1;
                1
            }
            (true, false) => {
                chunk.count -= 1;
                -1
            }
            _ => 0,
        };
        chunk.cells[idx] = c;
        let count_now = chunk.count;

        self.dirty = true;
        self.dirty_chunks.insert(coord);

        if count_now == 0 {
            self.chunks.remove(&coord);
        }

        if delta > 0 {
            self.total_count = self.total_count.saturating_add(1);
        } else if delta < 0 {
            self.total_count = self.total_count.saturating_sub(1);
        }

        // Boundary cells change face occlusion in the neighbour chunk across
        // the seam. Those chunks must rebuild too. We mark every seam
        // unconditionally; the mesher tolerates dirty coords with no loaded
        // chunk (it just skips them).
        let lx = p.x.rem_euclid(CHUNK_I);
        let ly = p.y.rem_euclid(CHUNK_I);
        let lz = p.z.rem_euclid(CHUNK_I);
        if lx == 0 {
            self.dirty_chunks.insert(coord + IVec3::new(-1, 0, 0));
        }
        if lx == CHUNK_I - 1 {
            self.dirty_chunks.insert(coord + IVec3::new(1, 0, 0));
        }
        if ly == 0 {
            self.dirty_chunks.insert(coord + IVec3::new(0, -1, 0));
        }
        if ly == CHUNK_I - 1 {
            self.dirty_chunks.insert(coord + IVec3::new(0, 1, 0));
        }
        if lz == 0 {
            self.dirty_chunks.insert(coord + IVec3::new(0, 0, -1));
        }
        if lz == CHUNK_I - 1 {
            self.dirty_chunks.insert(coord + IVec3::new(0, 0, 1));
        }
    }

    pub fn clear(&mut self) {
        for coord in self.chunks.keys().copied().collect::<Vec<_>>() {
            self.dirty_chunks.insert(coord);
        }
        self.chunks.clear();
        self.dirty = true;
        self.total_count = 0;
        self.warned_large = false;
    }

    #[inline]
    pub fn count(&self) -> usize {
        self.total_count as usize
    }

    pub fn iter_occupied(&self) -> impl Iterator<Item = (IVec3, Color8)> + '_ {
        self.chunks.iter().flat_map(|(coord, chunk)| {
            let coord = *coord;
            chunk
                .cells
                .iter()
                .enumerate()
                .filter_map(move |(idx, cell)| {
                    let c = (*cell)?;
                    let lx = (idx / (CHUNK * CHUNK)) as i32;
                    let ly = ((idx / CHUNK) % CHUNK) as i32;
                    let lz = (idx % CHUNK) as i32;
                    Some((coord * CHUNK_I + IVec3::new(lx, ly, lz), c))
                })
        })
    }

    pub fn bounding_box(&self) -> Option<(IVec3, IVec3)> {
        let mut iter = self.iter_occupied();
        let (first, _) = iter.next()?;
        let (mut min, mut max) = (first, first);
        for (p, _) in iter {
            min = min.min(p);
            max = max.max(p);
        }
        Some((min, max))
    }
}

/// Pure predicate used by the per-stroke perf-warning toast. Free function
/// so unit tests can poke at the threshold logic without allocating millions
/// of voxels.
pub fn large_scene_threshold_crossed(total_count: u32, chunk_count: u32) -> bool {
    total_count >= LARGE_SCENE_CELLS || chunk_count >= LARGE_SCENE_CHUNKS
}

/// Counterpart that drops the latch once both counters fall below 80 % of
/// their thresholds.
pub fn large_scene_warning_cleared(total_count: u32, chunk_count: u32) -> bool {
    total_count < LARGE_SCENE_CELLS * HYSTERESIS_NUM / HYSTERESIS_DEN
        && chunk_count < LARGE_SCENE_CHUNKS * HYSTERESIS_NUM / HYSTERESIS_DEN
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_empty() {
        let g = VoxelGrid::default();
        assert!(g.chunks.is_empty());
        assert_eq!(g.count(), 0);
        assert_eq!(g.total_count, 0);
        assert!(!g.warned_large);
    }

    #[test]
    fn set_below_floor_is_refused() {
        let mut g = VoxelGrid::default();
        g.set(IVec3::new(0, -1, 0), Some([1, 2, 3, 255]));
        assert_eq!(g.count(), 0);
        assert!(g.chunks.is_empty());
        assert_eq!(g.get(IVec3::new(0, -1, 0)), None);
    }

    #[test]
    fn set_at_floor_y_zero_is_allowed() {
        let mut g = VoxelGrid::default();
        g.set(IVec3::new(0, 0, 0), Some([1, 2, 3, 255]));
        assert_eq!(g.get(IVec3::new(0, 0, 0)), Some([1, 2, 3, 255]));
    }

    #[test]
    fn set_get_roundtrip_and_dirty_flag() {
        let mut g = VoxelGrid {
            dirty: false,
            ..Default::default()
        };
        let c: Color8 = [10, 20, 30, 255];
        g.set(IVec3::new(1, 2, 3), Some(c));
        assert_eq!(g.get(IVec3::new(1, 2, 3)), Some(c));
        assert!(g.dirty);
    }

    #[test]
    fn set_at_negative_x_or_z_works() {
        let mut g = VoxelGrid::default();
        g.set(IVec3::new(-50, 0, 100), Some([5, 6, 7, 255]));
        assert_eq!(g.get(IVec3::new(-50, 0, 100)), Some([5, 6, 7, 255]));
    }

    #[test]
    fn chunk_allocates_on_first_write_and_drops_when_emptied() {
        let mut g = VoxelGrid::default();
        g.set(IVec3::new(5, 5, 5), Some([1, 1, 1, 255]));
        assert_eq!(g.chunks.len(), 1);
        assert!(g.chunks.contains_key(&IVec3::ZERO));
        g.set(IVec3::new(5, 5, 5), None);
        assert!(g.chunks.is_empty());
        assert_eq!(g.count(), 0);
    }

    #[test]
    fn iter_occupied_visits_all_set_cells() {
        let mut g = VoxelGrid::default();
        let positions = [
            IVec3::new(0, 0, 0),
            IVec3::new(35, 17, 9),
            IVec3::new(-50, 0, 100),
        ];
        for p in &positions {
            g.set(*p, Some([1, 2, 3, 255]));
        }
        let mut seen: Vec<IVec3> = g.iter_occupied().map(|(p, _)| p).collect();
        seen.sort_by_key(|p| (p.x, p.y, p.z));
        let mut want: Vec<IVec3> = positions.to_vec();
        want.sort_by_key(|p| (p.x, p.y, p.z));
        assert_eq!(seen, want);
    }

    #[test]
    fn bounding_box_handles_negative_coords() {
        let mut g = VoxelGrid::default();
        g.set(IVec3::new(-10, 0, 5), Some([1, 1, 1, 255]));
        g.set(IVec3::new(20, 100, 50), Some([1, 1, 1, 255]));
        let (min, max) = g.bounding_box().unwrap();
        assert_eq!(min, IVec3::new(-10, 0, 5));
        assert_eq!(max, IVec3::new(20, 100, 50));
    }

    #[test]
    fn bounding_box_none_when_empty() {
        let g = VoxelGrid::default();
        assert!(g.bounding_box().is_none());
    }

    #[test]
    fn set_marks_owning_chunk_dirty() {
        let mut g = VoxelGrid::default();
        g.dirty_chunks.clear();
        g.set(IVec3::new(5, 5, 5), Some([1, 1, 1, 255]));
        assert!(g.dirty_chunks.contains(&IVec3::ZERO));
    }

    #[test]
    fn set_marks_seam_neighbour_dirty_positive() {
        let mut g = VoxelGrid::default();
        g.dirty_chunks.clear();
        // Last column of chunk (0,0,0); the X-seam neighbour (1,0,0) flags.
        let p = IVec3::new(CHUNK_I - 1, 5, 5);
        g.set(p, Some([1, 1, 1, 255]));
        assert!(g.dirty_chunks.contains(&IVec3::ZERO));
        assert!(g.dirty_chunks.contains(&IVec3::new(1, 0, 0)));
    }

    #[test]
    fn chunk_dirty_seam_propagation_with_negative_coords() {
        let mut g = VoxelGrid::default();
        g.dirty_chunks.clear();
        // x=0 in world coords is x=0 inside chunk (0,*,*); seam neighbour
        // at chunk (-1,0,0) must flag too.
        let p = IVec3::new(0, 5, 5);
        g.set(p, Some([1, 1, 1, 255]));
        assert!(g.dirty_chunks.contains(&IVec3::ZERO));
        assert!(g.dirty_chunks.contains(&IVec3::new(-1, 0, 0)));
    }

    #[test]
    fn set_does_not_mark_distant_chunks_dirty() {
        let mut g = VoxelGrid::default();
        g.dirty_chunks.clear();
        g.set(IVec3::new(5, 5, 5), Some([1, 1, 1, 255]));
        assert!(g.dirty_chunks.contains(&IVec3::ZERO));
        assert!(!g.dirty_chunks.contains(&IVec3::new(1, 0, 0)));
        assert!(!g.dirty_chunks.contains(&IVec3::new(-1, 0, 0)));
    }

    #[test]
    fn clear_drops_all_chunks_and_dirties_them() {
        let mut g = VoxelGrid::default();
        g.set(IVec3::new(0, 0, 0), Some([1, 1, 1, 255]));
        g.set(IVec3::new(50, 60, 70), Some([2, 2, 2, 255]));
        g.dirty_chunks.clear();
        g.clear();
        assert!(g.chunks.is_empty());
        assert_eq!(g.count(), 0);
        assert!(g.dirty);
        // The two former chunk coords are now dirty so the mesher despawns
        // their entities next frame.
        assert!(g.dirty_chunks.contains(&IVec3::ZERO));
        assert!(g.dirty_chunks.contains(&IVec3::new(1, 1, 2)));
    }

    #[test]
    fn count_tracks_set_and_overwrite() {
        let mut g = VoxelGrid::default();
        g.set(IVec3::new(0, 0, 0), Some([1, 1, 1, 255]));
        g.set(IVec3::new(0, 0, 0), Some([2, 2, 2, 255])); // overwrite, no delta
        assert_eq!(g.count(), 1);
        g.set(IVec3::new(0, 0, 0), None);
        assert_eq!(g.count(), 0);
    }

    #[test]
    fn large_scene_threshold_crossed_by_cells() {
        assert!(!large_scene_threshold_crossed(LARGE_SCENE_CELLS - 1, 0));
        assert!(large_scene_threshold_crossed(LARGE_SCENE_CELLS, 0));
    }

    #[test]
    fn large_scene_threshold_crossed_by_chunks() {
        assert!(!large_scene_threshold_crossed(0, LARGE_SCENE_CHUNKS - 1));
        assert!(large_scene_threshold_crossed(0, LARGE_SCENE_CHUNKS));
    }

    #[test]
    fn large_scene_warning_clears_only_when_both_below_hysteresis() {
        let cells_below = LARGE_SCENE_CELLS * HYSTERESIS_NUM / HYSTERESIS_DEN - 1;
        let chunks_below = LARGE_SCENE_CHUNKS * HYSTERESIS_NUM / HYSTERESIS_DEN - 1;
        let cells_above = LARGE_SCENE_CELLS * HYSTERESIS_NUM / HYSTERESIS_DEN;
        let chunks_above = LARGE_SCENE_CHUNKS * HYSTERESIS_NUM / HYSTERESIS_DEN;
        assert!(large_scene_warning_cleared(cells_below, chunks_below));
        assert!(!large_scene_warning_cleared(cells_above, chunks_below));
        assert!(!large_scene_warning_cleared(cells_below, chunks_above));
    }
}
