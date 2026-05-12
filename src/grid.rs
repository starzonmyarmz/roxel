use bevy::prelude::*;

pub const GRID: usize = 64;
pub const GRID_I: i32 = GRID as i32;

pub type Color8 = [u8; 4];

#[derive(Resource, Clone)]
pub struct VoxelGrid {
    pub cells: Box<[[[Option<Color8>; GRID]; GRID]; GRID]>,
    pub dirty: bool,
}

impl Default for VoxelGrid {
    fn default() -> Self {
        let cells: Box<[[[Option<Color8>; GRID]; GRID]; GRID]> =
            Box::new([[[None; GRID]; GRID]; GRID]);
        Self { cells, dirty: true }
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
        self.cells[p.x as usize][p.y as usize][p.z as usize]
    }

    #[inline]
    pub fn set(&mut self, p: IVec3, c: Option<Color8>) {
        if !Self::in_bounds(p) {
            return;
        }
        self.cells[p.x as usize][p.y as usize][p.z as usize] = c;
        self.dirty = true;
    }

    pub fn clear(&mut self) {
        for x in 0..GRID {
            for y in 0..GRID {
                for z in 0..GRID {
                    self.cells[x][y][z] = None;
                }
            }
        }
        self.dirty = true;
    }

    pub fn count(&self) -> usize {
        let mut n = 0;
        for x in 0..GRID {
            for y in 0..GRID {
                for z in 0..GRID {
                    if self.cells[x][y][z].is_some() {
                        n += 1;
                    }
                }
            }
        }
        n
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
