use crate::grid::{Color8, GRID, VoxelGrid};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Serialize, Deserialize)]
pub struct ProjectFile {
    pub version: u32,
    pub size: [u32; 3],
    pub voxels: Vec<([i32; 3], Color8)>,
}

const VERSION: u32 = 1;

pub fn save(path: &Path, grid: &VoxelGrid) -> Result<()> {
    let mut voxels = Vec::new();
    for x in 0..GRID {
        for y in 0..GRID {
            for z in 0..GRID {
                if let Some(c) = grid.cells[x][y][z] {
                    voxels.push(([x as i32, y as i32, z as i32], c));
                }
            }
        }
    }
    let pf = ProjectFile {
        version: VERSION,
        size: [GRID as u32; 3],
        voxels,
    };
    let s = ron::ser::to_string_pretty(&pf, ron::ser::PrettyConfig::default())?;
    std::fs::write(path, s)?;
    Ok(())
}

pub fn load(path: &Path, grid: &mut VoxelGrid) -> Result<()> {
    let s = std::fs::read_to_string(path)?;
    let pf: ProjectFile = ron::from_str(&s)?;
    grid.clear();
    for ([x, y, z], c) in pf.voxels {
        let p = bevy::math::IVec3::new(x, y, z);
        grid.set(p, Some(c));
    }
    Ok(())
}
