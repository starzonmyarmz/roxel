use crate::grid::{Color8, VoxelGrid};
use anyhow::Result;
use base64::Engine as _;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Serialize, Deserialize)]
struct ProjectFile {
    voxels: Vec<([i32; 3], Color8)>,
    /// Base64-encoded PNG preview of the model, `None` for older files.
    #[serde(default)]
    preview: Option<String>,
}

pub fn save(path: &Path, grid: &VoxelGrid) -> Result<()> {
    let voxels: Vec<([i32; 3], Color8)> = grid
        .iter_occupied()
        .map(|(p, c)| ([p.x, p.y, p.z], c))
        .collect();
    let pf = ProjectFile {
        voxels,
        preview: None,
    };
    let s = ron::ser::to_string_pretty(&pf, ron::ser::PrettyConfig::default())?;
    std::fs::write(path, s)?;
    Ok(())
}

/// Same as [`save`] but embeds a transparent PNG preview into the file.
pub fn save_with_preview(path: &Path, grid: &VoxelGrid, png_bytes: &[u8]) -> Result<()> {
    let voxels: Vec<([i32; 3], Color8)> = grid
        .iter_occupied()
        .map(|(p, c)| ([p.x, p.y, p.z], c))
        .collect();
    let pf = ProjectFile {
        voxels,
        preview: Some(base64::engine::general_purpose::STANDARD.encode(png_bytes)),
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
        grid.set(glam::IVec3::new(x, y, z), Some(c));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io::test_util::tmp_path as raw_tmp_path;
    use glam::IVec3;
    use std::path::PathBuf;

    fn tmp_path(name: &str) -> PathBuf {
        raw_tmp_path(name, "rox")
    }

    #[test]
    fn roundtrip_preserves_voxels() {
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
    fn roundtrip_with_negative_and_far_voxels() {
        let mut g = VoxelGrid::default();
        let pts: [(IVec3, [u8; 4]); 3] = [
            (IVec3::new(-50, 0, -25), [10, 20, 30, 255]),
            (IVec3::new(500, 100, -300), [200, 100, 50, 255]),
            (IVec3::new(0, 0, 0), [1, 2, 3, 255]),
        ];
        for (p, c) in pts {
            g.set(p, Some(c));
        }
        let path = tmp_path("openworld_roundtrip");
        save(&path, &g).expect("save");
        let mut loaded = VoxelGrid::default();
        load(&path, &mut loaded).expect("load");
        for (p, c) in pts {
            assert_eq!(loaded.get(p), Some(c), "{p:?}");
        }
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
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn v1_extra_fields_are_silently_ignored() {
        // The previous (`version`, `size`, `voxels`) layout no longer round-
        // trips, but the loader is intentionally lax: ron drops unknown
        // fields, so a v1 file whose voxel coords already live in the open
        // world's coordinate space happens to load. Documenting the behaviour
        // so a future strict-mode change is a conscious decision.
        let v1 = "(version: 1, size: [32, 32, 32], voxels: [((1, 2, 3), (10, 20, 30, 255))])";
        let mut g = VoxelGrid::default();
        let path = tmp_path("v1_lax");
        std::fs::write(&path, v1).expect("write");
        load(&path, &mut g).expect("load");
        assert_eq!(g.get(IVec3::new(1, 2, 3)), Some([10, 20, 30, 255]));
        let _ = std::fs::remove_file(&path);
    }
}
