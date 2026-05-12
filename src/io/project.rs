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
                if let Some(c) = grid.cell(x, y, z) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::math::IVec3;
    use std::path::PathBuf;

    fn tmp_path(name: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        let pid = std::process::id();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        p.push(format!("roxel-test-{pid}-{nanos}-{name}.roxel"));
        p
    }

    #[test]
    fn save_load_roundtrip_preserves_voxels() {
        let mut g = VoxelGrid::default();
        let pts: [(IVec3, [u8; 4]); 3] = [
            (IVec3::new(0, 0, 0), [10, 20, 30, 255]),
            (IVec3::new(5, 6, 7), [200, 100, 50, 255]),
            (IVec3::new(63, 63, 63), [1, 2, 3, 128]),
        ];
        for (p, c) in pts {
            g.set(p, Some(c));
        }
        let path = tmp_path("roundtrip");
        save(&path, &g).expect("save");

        let mut loaded = VoxelGrid::default();
        loaded.set(IVec3::new(1, 1, 1), Some([9, 9, 9, 255])); // pre-existing data must be cleared
        load(&path, &mut loaded).expect("load");

        for (p, c) in pts {
            assert_eq!(loaded.get(p), Some(c));
        }
        assert_eq!(loaded.count(), 3);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn save_only_writes_occupied_cells() {
        let mut g = VoxelGrid::default();
        g.set(IVec3::new(1, 2, 3), Some([1, 2, 3, 255]));
        let path = tmp_path("sparse");
        save(&path, &g).expect("save");
        let s = std::fs::read_to_string(&path).expect("read");
        let pf: ProjectFile = ron::from_str(&s).expect("parse");
        assert_eq!(pf.voxels.len(), 1);
        assert_eq!(pf.version, VERSION);
        let _ = std::fs::remove_file(&path);
    }
}
