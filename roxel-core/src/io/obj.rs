use crate::grid::VoxelGrid;
use crate::mesh_util::{FACES, greedy_quads};
use anyhow::Result;
use std::io::Write;
use std::path::Path;

// Greedy-merged quads (the renderer's mesher) with per-vertex colors
// (Blender-style "v x y z r g b" extension).
pub fn export(path: &Path, grid: &VoxelGrid) -> Result<()> {
    super::atomic::write_with(path, |file| write_obj(file, grid))
}

fn write_obj<W: Write>(file: &mut W, grid: &VoxelGrid) -> Result<()> {
    writeln!(file, "# Exported by Roxel")?;

    let quads = greedy_quads(grid, None, None);
    if quads.is_empty() {
        return Ok(());
    }
    for f in &FACES {
        writeln!(file, "vn {} {} {}", f.d.x, f.d.y, f.d.z)?;
    }
    for q in &quads {
        let r = q.color[0] as f32 / 255.0;
        let g = q.color[1] as f32 / 255.0;
        let b = q.color[2] as f32 / 255.0;
        for c in &q.corners {
            writeln!(file, "v {} {} {} {r:.3} {g:.3} {b:.3}", c.x, c.y, c.z)?;
        }
    }
    for (i, q) in quads.iter().enumerate() {
        let v = i * 4 + 1; // OBJ indices are 1-based.
        let n = q.face_idx + 1;
        writeln!(
            file,
            "f {}//{n} {}//{n} {}//{n} {}//{n}",
            v,
            v + 1,
            v + 2,
            v + 3
        )?;
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
        raw_tmp_path(name, "obj")
    }

    fn count_prefix(s: &str, prefix: &str) -> usize {
        s.lines().filter(|l| l.starts_with(prefix)).count()
    }

    #[test]
    fn single_voxel_emits_six_faces_24_verts_6_normals() {
        let mut g = VoxelGrid::default();
        g.set(IVec3::new(1, 1, 1), Some([255, 128, 64, 255]));
        let path = tmp_path("single");
        export(&path, &g).expect("export");
        let s = std::fs::read_to_string(&path).expect("read");
        assert_eq!(count_prefix(&s, "v "), 24);
        assert_eq!(count_prefix(&s, "vn "), 6);
        assert_eq!(count_prefix(&s, "f "), 6);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn adjacent_voxels_omit_shared_face() {
        let mut g = VoxelGrid::default();
        g.set(IVec3::new(0, 0, 0), Some([255, 0, 0, 255]));
        g.set(IVec3::new(1, 0, 0), Some([0, 255, 0, 255]));
        let path = tmp_path("adjacent");
        export(&path, &g).expect("export");
        let s = std::fs::read_to_string(&path).expect("read");
        // 12 total faces minus 2 shared = 10
        assert_eq!(count_prefix(&s, "f "), 10);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn same_color_row_merges_and_shares_normals() {
        let mut g = VoxelGrid::default();
        for x in 0..4 {
            g.set(IVec3::new(x, 0, 0), Some([255, 0, 0, 255]));
        }
        let path = tmp_path("row");
        export(&path, &g).expect("export");
        let s = std::fs::read_to_string(&path).expect("read");
        assert_eq!(count_prefix(&s, "f "), 6);
        assert_eq!(count_prefix(&s, "vn "), 6);
        assert!(s.lines().any(|l| l.starts_with("v 4 ")));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn vertex_lines_carry_normalized_color() {
        let mut g = VoxelGrid::default();
        g.set(IVec3::new(0, 0, 0), Some([255, 0, 0, 255]));
        let path = tmp_path("color");
        export(&path, &g).expect("export");
        let s = std::fs::read_to_string(&path).expect("read");
        // r=1.000 g=0.000 b=0.000 appears on every vertex line
        let any_red = s
            .lines()
            .any(|l| l.starts_with("v ") && l.contains("1.000 0.000 0.000"));
        assert!(any_red, "expected red vertex color line");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn empty_grid_writes_only_header() {
        let g = VoxelGrid::default();
        let path = tmp_path("empty");
        export(&path, &g).expect("export");
        let s = std::fs::read_to_string(&path).expect("read");
        assert_eq!(count_prefix(&s, "v "), 0);
        assert_eq!(count_prefix(&s, "f "), 0);
        assert!(s.starts_with("# Exported by Roxel"));
        let _ = std::fs::remove_file(&path);
    }
}
