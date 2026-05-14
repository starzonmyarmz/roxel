use crate::grid::{VoxelGrid, snap_to_allowed_size};
use crate::io::reader::LeReader;
use anyhow::{Result, bail};
use bevy::math::IVec3;
use std::path::Path;

// Qubicle Binary (.qb) reader. Format reference:
//   minddesk.com/learn/article.php?id=47
//
// All integers are little-endian. .qb is Y-up, matches Roxel — no axis remap.
// .qbcl (zlib-compressed) is deferred.

const CODEFLAG: u32 = 2;
const NEXTSLICE: u32 = 6;

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

    // Max world-space index reached by this matrix.
    let max_extent = ((px + sx as i32).max(py + sy as i32).max(pz + sz as i32) as usize).max(1);
    grid.resize(snap_to_allowed_size(max_extent));

    let mut dropped = 0usize;
    let limit = grid.size as i32;
    let mut place = |x: i32, y: i32, z: i32, raw: u32, dropped: &mut usize| {
        // Alpha byte == 0 means empty voxel.
        let bytes = raw.to_le_bytes();
        let alpha = bytes[3];
        if alpha == 0 {
            return;
        }
        let rgb = if color_format == 0 {
            [bytes[0], bytes[1], bytes[2], 255]
        } else {
            [bytes[2], bytes[1], bytes[0], 255]
        };
        let p = IVec3::new(px + x, py + y, pz + z);
        if p.x < 0 || p.y < 0 || p.z < 0 || p.x >= limit || p.y >= limit || p.z >= limit {
            *dropped += 1;
            return;
        }
        grid.set(p, Some(rgb));
    };

    if !compressed {
        for z in 0..sz {
            for y in 0..sy {
                for x in 0..sx {
                    let v = r.u32()?;
                    place(x as i32, y as i32, z as i32, v, &mut dropped);
                }
            }
        }
    } else {
        for z in 0..sz {
            let mut i: u32 = 0;
            loop {
                let data = r.u32()?;
                if data == NEXTSLICE {
                    break;
                }
                if data == CODEFLAG {
                    let count = r.u32()?;
                    let color = r.u32()?;
                    for _ in 0..count {
                        let x = i % sx;
                        let y = i / sx;
                        place(x as i32, y as i32, z as i32, color, &mut dropped);
                        i += 1;
                    }
                } else {
                    let x = i % sx;
                    let y = i / sx;
                    place(x as i32, y as i32, z as i32, data, &mut dropped);
                    i += 1;
                }
            }
        }
    }

    if dropped > 0 {
        eprintln!(
            "Import .qb: dropped {dropped} voxels outside {sz}³",
            sz = grid.size
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grid::MAX_GRID;
    use crate::io::test_util::tmp_path;

    fn push_u32(buf: &mut Vec<u8>, v: u32) {
        buf.extend_from_slice(&v.to_le_bytes());
    }
    fn push_i32(buf: &mut Vec<u8>, v: i32) {
        buf.extend_from_slice(&v.to_le_bytes());
    }

    /// rgba_to_u32 packs bytes as little-endian u32: byte[0]=R, byte[1]=G,
    /// byte[2]=B, byte[3]=A. Reader reads via to_le_bytes() which yields the
    /// same byte order, so for RGBA format the first byte is R.
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
        // (x=0,y=0,z=0) empty, (x=1,y=0,z=0) red.
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
        // One slice: CODEFLAG, count=4, color=green; then NEXTSLICE.
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
        // raw bytes are [R=10, G=20, B=30, A=255] in RGBA, so under BGRA
        // interpretation byte[0]=B=10, byte[2]=R=30 → grid stores [30, 20, 10, 255].
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
        // Second matrix is never read. Pad with junk so .qb file has bytes
        // beyond the first matrix end (the reader stops at the first matrix).
        matrix_header(&mut buf, "b", (1, 1, 1), (20, 0, 0));
        push_u32(&mut buf, rgba(99, 99, 99, 255));
        let path = tmp_path("first-only", "qb");
        std::fs::write(&path, &buf).unwrap();
        let mut g = VoxelGrid::default();
        import(&path, &mut g).unwrap();
        assert_eq!(g.get(IVec3::new(0, 0, 0)), Some([1, 2, 3, 255]));
        // The second matrix's voxel at (20, 0, 0) must NOT be placed.
        assert_eq!(g.get(IVec3::new(20, 0, 0)), None);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn import_snaps_to_smallest_fitting_size() {
        let mut buf = Vec::new();
        header(&mut buf, false, 0, 1);
        matrix_header(&mut buf, "m", (40, 1, 1), (0, 0, 0));
        for x in 0..40 {
            let _ = x;
            push_u32(&mut buf, rgba(1, 1, 1, 255));
        }
        let path = tmp_path("snap", "qb");
        std::fs::write(&path, &buf).unwrap();
        let mut g = VoxelGrid::default();
        import(&path, &mut g).unwrap();
        assert_eq!(g.size, 64);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn import_crops_voxels_outside_max_grid() {
        // pos offset = 130 pushes the single voxel past MAX_GRID=128. The
        // matrix declares its own max extent though, so grid.size resolves to
        // 128 and the voxel gets dropped.
        let mut buf = Vec::new();
        header(&mut buf, false, 0, 1);
        matrix_header(&mut buf, "m", (1, 1, 1), (130, 0, 0));
        push_u32(&mut buf, rgba(50, 50, 50, 255));
        let path = tmp_path("crop", "qb");
        std::fs::write(&path, &buf).unwrap();
        let mut g = VoxelGrid::default();
        import(&path, &mut g).unwrap();
        assert_eq!(g.size, MAX_GRID);
        assert_eq!(g.count(), 0);
        let _ = std::fs::remove_file(&path);
    }
}
