use crate::grid::VoxelGrid;
use crate::io::reader::LeReader;
use anyhow::{Result, bail};
use glam::IVec3;
use std::path::Path;

// Qubicle Binary (.qb) reader. Format reference:
//   minddesk.com/learn/article.php?id=47
//
// All integers are little-endian. .qb is Y-up, matches Roxel — no axis remap.
// .qbcl (zlib-compressed) is deferred.

const CODEFLAG: u32 = 2;
const NEXTSLICE: u32 = 6;

const MAX_CELLS: u64 = 1 << 28;

pub fn import(path: &Path, grid: &mut VoxelGrid) -> Result<()> {
    let bytes = std::fs::read(path)?;
    let mut r = LeReader::new(&bytes);
    let _version = r.u32()?;
    let color_format = r.u32()?; // 0 RGBA, 1 BGRA
    let _z_orient = r.u32()?;
    let compressed = r.u32()? != 0;
    let _vis_mask = r.u32()?;
    let num_matrices = r.u32()?;
    if num_matrices == 0 {
        bail!("Import .qb: file contains no matrices");
    }
    if num_matrices > 1 {
        eprintln!("Import .qb: {num_matrices} matrices found, using first only");
    }

    let name_len = r.u8()? as usize;
    let _name = r.bytes(name_len)?;
    let sx = r.u32()?;
    let sy = r.u32()?;
    let sz = r.u32()?;
    let px = r.i32()?;
    let py = r.i32()?;
    let pz = r.i32()?;

    if sx == 0 || sy == 0 || sz == 0 {
        bail!("Import .qb: matrix has a zero-sized axis ({sx}×{sy}×{sz})");
    }
    let slice = u64::from(sx) * u64::from(sy);
    if slice
        .checked_mul(u64::from(sz))
        .is_none_or(|v| v > MAX_CELLS)
    {
        bail!("Import .qb: matrix {sx}×{sy}×{sz} is too large");
    }
    let origin = IVec3::new(px, py, pz);
    let far = [(px, sx), (py, sy), (pz, sz)];
    if far
        .iter()
        .any(|&(p, s)| p.checked_add_unsigned(s).is_none())
    {
        bail!("Import .qb: matrix position is out of range");
    }

    let mut dropped = 0usize;
    let mut place = |x: u32, y: u32, z: u32, raw: u32| {
        let bytes = raw.to_le_bytes();
        if bytes[3] == 0 {
            return;
        }
        let rgb = if color_format == 0 {
            [bytes[0], bytes[1], bytes[2], 255]
        } else {
            [bytes[2], bytes[1], bytes[0], 255]
        };
        let p = origin + IVec3::new(x as i32, y as i32, z as i32);
        if p.y < 0 {
            dropped += 1;
            return;
        }
        grid.set(p, Some(rgb));
    };

    if !compressed {
        for z in 0..sz {
            for y in 0..sy {
                for x in 0..sx {
                    let v = r.u32()?;
                    place(x, y, z, v);
                }
            }
        }
    } else {
        for z in 0..sz {
            let mut i: u64 = 0;
            loop {
                let data = r.u32()?;
                if data == NEXTSLICE {
                    break;
                }
                let (count, color) = if data == CODEFLAG {
                    (u64::from(r.u32()?), r.u32()?)
                } else {
                    (1, data)
                };
                if i + count > slice {
                    bail!("Import .qb: run overflows slice {z}");
                }
                for j in i..i + count {
                    place(
                        (j % u64::from(sx)) as u32,
                        (j / u64::from(sx)) as u32,
                        z,
                        color,
                    );
                }
                i += count;
            }
        }
    }

    if dropped > 0 {
        eprintln!("Import .qb: dropped {dropped} voxels below the floor");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io::test_util::tmp_path;

    fn push_u32(buf: &mut Vec<u8>, v: u32) {
        buf.extend_from_slice(&v.to_le_bytes());
    }
    fn push_i32(buf: &mut Vec<u8>, v: i32) {
        buf.extend_from_slice(&v.to_le_bytes());
    }

    fn rgba(r: u8, g: u8, b: u8, a: u8) -> u32 {
        u32::from_le_bytes([r, g, b, a])
    }

    fn header(buf: &mut Vec<u8>, compressed: bool, color_format: u32, num_matrices: u32) {
        push_u32(buf, 257); // version
        push_u32(buf, color_format); // 0 RGBA, 1 BGRA
        push_u32(buf, 1); // z-axis orient
        push_u32(buf, if compressed { 1 } else { 0 });
        push_u32(buf, 0); // visibility mask
        push_u32(buf, num_matrices);
    }

    fn matrix_header(buf: &mut Vec<u8>, name: &str, size: (u32, u32, u32), pos: (i32, i32, i32)) {
        buf.push(name.len() as u8);
        buf.extend_from_slice(name.as_bytes());
        push_u32(buf, size.0);
        push_u32(buf, size.1);
        push_u32(buf, size.2);
        push_i32(buf, pos.0);
        push_i32(buf, pos.1);
        push_i32(buf, pos.2);
    }

    #[test]
    fn import_uncompressed_single_voxel() {
        let mut buf = Vec::new();
        header(&mut buf, false, 0, 1);
        matrix_header(&mut buf, "m", (2, 1, 1), (0, 0, 0));
        push_u32(&mut buf, 0);
        push_u32(&mut buf, rgba(200, 50, 25, 255));
        let path = tmp_path("uncompressed", "qb");
        std::fs::write(&path, &buf).unwrap();
        let mut g = VoxelGrid::default();
        import(&path, &mut g).unwrap();
        assert_eq!(g.get(IVec3::new(0, 0, 0)), None);
        assert_eq!(g.get(IVec3::new(1, 0, 0)), Some([200, 50, 25, 255]));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn import_rle_run_expands() {
        let mut buf = Vec::new();
        header(&mut buf, true, 0, 1);
        matrix_header(&mut buf, "m", (4, 1, 1), (0, 0, 0));
        push_u32(&mut buf, CODEFLAG);
        push_u32(&mut buf, 4);
        push_u32(&mut buf, rgba(0, 200, 0, 255));
        push_u32(&mut buf, NEXTSLICE);
        let path = tmp_path("rle", "qb");
        std::fs::write(&path, &buf).unwrap();
        let mut g = VoxelGrid::default();
        import(&path, &mut g).unwrap();
        for x in 0..4 {
            assert_eq!(g.get(IVec3::new(x, 0, 0)), Some([0, 200, 0, 255]));
        }
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn import_bgra_color_format_swizzles() {
        let mut buf = Vec::new();
        header(&mut buf, false, 1, 1);
        matrix_header(&mut buf, "m", (1, 1, 1), (0, 0, 0));
        push_u32(&mut buf, rgba(10, 20, 30, 255));
        let path = tmp_path("bgra", "qb");
        std::fs::write(&path, &buf).unwrap();
        let mut g = VoxelGrid::default();
        import(&path, &mut g).unwrap();
        assert_eq!(g.get(IVec3::new(0, 0, 0)), Some([30, 20, 10, 255]));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn import_uses_first_matrix_only() {
        let mut buf = Vec::new();
        header(&mut buf, false, 0, 2);
        matrix_header(&mut buf, "a", (1, 1, 1), (0, 0, 0));
        push_u32(&mut buf, rgba(1, 2, 3, 255));
        matrix_header(&mut buf, "b", (1, 1, 1), (20, 0, 0));
        push_u32(&mut buf, rgba(99, 99, 99, 255));
        let path = tmp_path("first-only", "qb");
        std::fs::write(&path, &buf).unwrap();
        let mut g = VoxelGrid::default();
        import(&path, &mut g).unwrap();
        assert_eq!(g.get(IVec3::new(0, 0, 0)), Some([1, 2, 3, 255]));
        assert_eq!(g.get(IVec3::new(20, 0, 0)), None);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn import_accepts_voxels_far_from_origin() {
        // Open-world: a matrix offset 200 voxels from origin must land at
        // (200, 0, 0); no resizing, no clipping.
        let mut buf = Vec::new();
        header(&mut buf, false, 0, 1);
        matrix_header(&mut buf, "m", (1, 1, 1), (200, 0, 0));
        push_u32(&mut buf, rgba(50, 60, 70, 255));
        let path = tmp_path("far", "qb");
        std::fs::write(&path, &buf).unwrap();
        let mut g = VoxelGrid::default();
        import(&path, &mut g).unwrap();
        assert_eq!(g.get(IVec3::new(200, 0, 0)), Some([50, 60, 70, 255]));
        let _ = std::fs::remove_file(&path);
    }

    fn import_err(name: &str, buf: &[u8]) -> String {
        let path = tmp_path(name, "qb");
        std::fs::write(&path, buf).unwrap();
        let mut g = VoxelGrid::default();
        let err = import(&path, &mut g).unwrap_err().to_string();
        let _ = std::fs::remove_file(&path);
        err
    }

    #[test]
    fn import_rejects_a_zero_sized_axis() {
        let mut buf = Vec::new();
        header(&mut buf, true, 0, 1);
        matrix_header(&mut buf, "m", (0, 1, 1), (0, 0, 0));
        push_u32(&mut buf, rgba(1, 1, 1, 255));
        push_u32(&mut buf, NEXTSLICE);
        assert!(import_err("zero-axis", &buf).contains("zero-sized"));
    }

    #[test]
    fn import_rejects_a_run_longer_than_its_slice() {
        let mut buf = Vec::new();
        header(&mut buf, true, 0, 1);
        matrix_header(&mut buf, "m", (2, 2, 1), (0, 0, 0));
        push_u32(&mut buf, CODEFLAG);
        push_u32(&mut buf, u32::MAX);
        push_u32(&mut buf, rgba(1, 1, 1, 255));
        push_u32(&mut buf, NEXTSLICE);
        assert!(import_err("long-run", &buf).contains("overflows"));
    }

    #[test]
    fn import_rejects_an_oversized_matrix() {
        let mut buf = Vec::new();
        header(&mut buf, true, 0, 1);
        matrix_header(&mut buf, "m", (u32::MAX, u32::MAX, 2), (0, 0, 0));
        assert!(import_err("huge", &buf).contains("too large"));
    }

    #[test]
    fn import_rejects_a_position_that_overflows() {
        let mut buf = Vec::new();
        header(&mut buf, false, 0, 1);
        matrix_header(&mut buf, "m", (2, 1, 1), (i32::MAX, 0, 0));
        push_u32(&mut buf, rgba(1, 1, 1, 255));
        push_u32(&mut buf, rgba(1, 1, 1, 255));
        assert!(import_err("overflow", &buf).contains("out of range"));
    }

    #[test]
    fn import_drops_voxels_below_floor() {
        // Negative-Y pos must refuse.
        let mut buf = Vec::new();
        header(&mut buf, false, 0, 1);
        matrix_header(&mut buf, "m", (1, 1, 1), (0, -3, 0));
        push_u32(&mut buf, rgba(5, 5, 5, 255));
        let path = tmp_path("below-floor", "qb");
        std::fs::write(&path, &buf).unwrap();
        let mut g = VoxelGrid::default();
        import(&path, &mut g).unwrap();
        assert_eq!(g.count(), 0);
        let _ = std::fs::remove_file(&path);
    }
}
