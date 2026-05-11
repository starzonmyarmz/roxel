use bevy::prelude::*;

pub const GRID: usize = 64;
pub const GRID_I: i32 = GRID as i32;

pub type Color8 = [u8; 4];

#[derive(Resource)]
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
