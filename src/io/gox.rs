use crate::grid::{ALLOWED_SIZES, VoxelGrid, snap_to_allowed_size};
use crate::io::reader::LeReader;
use anyhow::{Result, anyhow, bail};
use bevy::math::IVec3;
use std::path::Path;

// Goxel .gox reader/writer. Format (version 2):
//
//   magic:    b"GOX "
//   version:  i32 LE = 2
//   chunks until EOF, each:
//     type:    4 bytes
//     size:    i32 LE (data byte count)
//     data:    `size` bytes
//     crc:     i32 LE (ignored on read, zero on write)
//
// Roxel writes BL16 blocks as raw 16×16×16×4 RGBA bytes and a single LAYR
// referencing them. Foreign Goxel files that store BL16 as a PNG image are
// rejected with a clear error — Goxel is Z-up, so positions are remapped to
// Roxel's Y-up convention on both read and write.
//
// Block-local byte offset for voxel (lx, ly, lz) in Goxel coords:
//   offset = (lz * 256 + ly * 16 + lx) * 4
// Goxel Y-up→Z-up remap: Roxel(rx, ry, rz) → Goxel(gx = rx, gy = rz, gz = ry).

const BLOCK_SIZE: usize = 16;
const BLOCK_VOXELS: usize = BLOCK_SIZE * BLOCK_SIZE * BLOCK_SIZE;
const BLOCK_BYTES: usize = BLOCK_VOXELS * 4;
const PNG_MAGIC: [u8; 4] = [0x89, 0x50, 0x4E, 0x47];

pub fn export(path: &Path, grid: &VoxelGrid) -> Result<()> {
    let mut buf = Vec::new();
    buf.extend_from_slice(b"GOX ");
    buf.extend_from_slice(&2i32.to_le_bytes());

    // Minimal IMG chunk: empty dict (zero-size terminator).
    let mut img = Vec::new();
    img.extend_from_slice(&0i32.to_le_bytes());
    write_chunk(&mut buf, b"IMG ", &img);

    // Collect non-empty Roxel 16³ chunks. The block at Roxel (cx, cy, cz) is
    // stored at Goxel position (cx, cz, cy).
    let chunks_per_axis = grid.size.div_ceil(BLOCK_SIZE);
    let mut blocks: Vec<(i32, i32, i32, Box<[u8; BLOCK_BYTES]>)> = Vec::new();
    for cx in 0..chunks_per_axis {
        for cy in 0..chunks_per_axis {
            for cz in 0..chunks_per_axis {
                if let Some(block) = build_block(grid, cx, cy, cz) {
                    let gx = (cx * BLOCK_SIZE) as i32;
                    let gy = (cz * BLOCK_SIZE) as i32;
                    let gz = (cy * BLOCK_SIZE) as i32;
                    blocks.push((gx, gy, gz, block));
                }
            }
        }
    }

    for (_, _, _, data) in &blocks {
        write_chunk(&mut buf, b"BL16", data.as_ref());
    }

    let mut layr = Vec::new();
    layr.extend_from_slice(&(blocks.len() as i32).to_le_bytes());
    for (i, (gx, gy, gz, _)) in blocks.iter().enumerate() {
        layr.extend_from_slice(&(i as i32).to_le_bytes());
        layr.extend_from_slice(&gx.to_le_bytes());
        layr.extend_from_slice(&gy.to_le_bytes());
        layr.extend_from_slice(&gz.to_le_bytes());
        layr.extend_from_slice(&[0u8; 8]);
    }
    write_dict_entry(&mut layr, "name", b"Layer 0");
    write_dict_entry(&mut layr, "visible", &[1u8]);
    layr.extend_from_slice(&0i32.to_le_bytes()); // dict terminator
    write_chunk(&mut buf, b"LAYR", &layr);

    std::fs::write(path, &buf)?;
    Ok(())
}

fn build_block(
    grid: &VoxelGrid,
    cx: usize,
    cy: usize,
    cz: usize,
) -> Option<Box<[u8; BLOCK_BYTES]>> {
    let mut data: Box<[u8; BLOCK_BYTES]> = Box::new([0u8; BLOCK_BYTES]);
    let mut any = false;
    for lx in 0..BLOCK_SIZE {
        for ly in 0..BLOCK_SIZE {
            for lz in 0..BLOCK_SIZE {
                let rx = cx * BLOCK_SIZE + lx;
                let ry = cy * BLOCK_SIZE + ly;
                let rz = cz * BLOCK_SIZE + lz;
                if rx >= grid.size || ry >= grid.size || rz >= grid.size {
                    continue;
                }
                let Some(c) = grid.cell(rx, ry, rz) else {
                    continue;
                };
                // Roxel (lx, ly, lz) → Goxel block-local (gx=lx, gy=lz, gz=ly).
                let off = (ly * 256 + lz * 16 + lx) * 4;
                data[off] = c[0];
                data[off + 1] = c[1];
                data[off + 2] = c[2];
                data[off + 3] = 255;
                any = true;
            }
        }
    }
    if any { Some(data) } else { None }
}

fn write_chunk(out: &mut Vec<u8>, typ: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(typ);
    out.extend_from_slice(&(data.len() as i32).to_le_bytes());
    out.extend_from_slice(data);
    out.extend_from_slice(&0i32.to_le_bytes()); // CRC placeholder
}

fn write_dict_entry(out: &mut Vec<u8>, key: &str, value: &[u8]) {
    out.extend_from_slice(&(key.len() as i32).to_le_bytes());
    out.extend_from_slice(key.as_bytes());
    out.extend_from_slice(&(value.len() as i32).to_le_bytes());
    out.extend_from_slice(value);
}

pub fn import(path: &Path, grid: &mut VoxelGrid) -> Result<()> {
    let bytes = std::fs::read(path)?;
    let mut r = LeReader::new(&bytes);
    let magic = r.bytes(4)?;
    if magic != b"GOX " {
        bail!("not a Goxel file: bad magic");
    }
    let _version = r.i32()?;

    let mut blocks: Vec<[u8; BLOCK_BYTES]> = Vec::new();
    let mut refs: Option<Vec<(i32, i32, i32, i32)>> = None;

    while !r.eof() {
        let mut typ = [0u8; 4];
        typ.copy_from_slice(r.bytes(4)?);
        let size = r.i32()? as usize;
        let data = r.bytes(size)?.to_vec();
        let _crc = r.i32()?;

        match &typ {
            b"BL16" => {
                if data.len() == BLOCK_BYTES {
                    let mut arr = [0u8; BLOCK_BYTES];
                    arr.copy_from_slice(&data);
                    blocks.push(arr);
                } else if data.len() >= 4 && data[0..4] == PNG_MAGIC {
                    bail!("Goxel block is PNG-encoded; Roxel only supports raw RGBA blocks");
                } else {
                    bail!(
                        "unexpected BL16 block size: got {} bytes, want {BLOCK_BYTES}",
                        data.len()
                    );
                }
            }
            b"LAYR" => {
                if refs.is_some() {
                    continue; // first layer only
                }
                let mut lr = LeReader::new(&data);
                let n = lr.i32()? as usize;
                let mut collected = Vec::with_capacity(n);
                for _ in 0..n {
                    let idx = lr.i32()?;
                    let gx = lr.i32()?;
                    let gy = lr.i32()?;
                    let gz = lr.i32()?;
                    let _reserved = lr.bytes(8)?;
                    collected.push((idx, gx, gy, gz));
                }
                refs = Some(collected);
            }
            _ => {} // IMG/PREV/CAMR/MATE/etc. ignored
        }
    }

    let refs = refs.ok_or_else(|| anyhow!("no LAYR chunk in file"))?;

    // Goxel positions can be negative. Find min/max in Roxel coords so we can
    // shift everything to start at (0, 0, 0) before snapping to a grid size.
    let mut min = IVec3::new(i32::MAX, i32::MAX, i32::MAX);
    let mut max = IVec3::new(i32::MIN, i32::MIN, i32::MIN);
    for (_, gx, gy, gz) in &refs {
        let rx = *gx;
        let ry = *gz;
        let rz = *gy;
        min = min.min(IVec3::new(rx, ry, rz));
        max = max.max(IVec3::new(rx + 16, ry + 16, rz + 16));
    }
    if refs.is_empty() {
        grid.resize(ALLOWED_SIZES[0]);
        return Ok(());
    }
    let shift = -min;
    let extent = (max - min).max_element() as usize;
    grid.resize(snap_to_allowed_size(extent));

    let mut dropped = 0usize;
    let limit = grid.size as i32;
    for (idx, gx, gy, gz) in refs {
        let Some(block) = blocks.get(idx as usize) else {
            continue;
        };
        let base_rx = gx + shift.x;
        let base_ry = gz + shift.y;
        let base_rz = gy + shift.z;
        // Loop variables track Goxel block-local axes (gzl outer, gyl mid,
        // gxl inner) so the byte offset matches `(gzl*256 + gyl*16 + gxl)*4`.
        // The Z↔Y remap then maps Goxel (gxl, gyl, gzl) → Roxel local
        // (lx=gxl, ly=gzl, lz=gyl).
        for gzl in 0..BLOCK_SIZE {
            for gyl in 0..BLOCK_SIZE {
                for gxl in 0..BLOCK_SIZE {
                    let off = (gzl * 256 + gyl * 16 + gxl) * 4;
                    let a = block[off + 3];
                    if a == 0 {
                        continue;
                    }
                    let rx = base_rx + gxl as i32;
                    let ry = base_ry + gzl as i32;
                    let rz = base_rz + gyl as i32;
                    if rx < 0 || ry < 0 || rz < 0 || rx >= limit || ry >= limit || rz >= limit {
                        dropped += 1;
                        continue;
                    }
                    grid.set(
                        IVec3::new(rx, ry, rz),
                        Some([block[off], block[off + 1], block[off + 2], 255]),
                    );
                }
            }
        }
    }
    if dropped > 0 {
        eprintln!(
            "Import .gox: dropped {dropped} voxels outside {sz}³",
            sz = grid.size
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io::test_util::tmp_path;

    #[test]
    fn export_writes_magic_and_version() {
        let mut g = VoxelGrid::default();
        g.set(IVec3::new(0, 0, 0), Some([1, 2, 3, 255]));
        let path = tmp_path("magic", "gox");
        export(&path, &g).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        assert_eq!(&bytes[0..4], b"GOX ");
        let version = i32::from_le_bytes(bytes[4..8].try_into().unwrap());
        assert_eq!(version, 2);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn empty_grid_export_then_import_yields_empty_grid() {
        let g = VoxelGrid::default();
        let path = tmp_path("empty", "gox");
        export(&path, &g).unwrap();
        let mut g2 = VoxelGrid::default();
        g2.set(IVec3::new(0, 0, 0), Some([9, 9, 9, 255]));
        import(&path, &mut g2).unwrap();
        assert_eq!(g2.count(), 0);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn roundtrip_preserves_voxels_across_block_boundary() {
        // Voxels in three different 16³ blocks at the corners of a 64³ grid.
        let mut g = VoxelGrid::default();
        g.resize(64);
        let pts: [(IVec3, [u8; 4]); 4] = [
            (IVec3::new(0, 0, 0), [255, 0, 0, 255]),
            (IVec3::new(15, 5, 7), [0, 255, 0, 255]),
            (IVec3::new(20, 20, 20), [0, 0, 255, 255]),
            (IVec3::new(63, 63, 63), [200, 100, 50, 255]),
        ];
        for (p, c) in pts {
            g.set(p, Some(c));
        }
        let path = tmp_path("roundtrip", "gox");
        export(&path, &g).unwrap();
        let mut g2 = VoxelGrid::default();
        import(&path, &mut g2).unwrap();
        for (p, c) in pts {
            assert_eq!(g2.get(p), Some(c), "voxel at {:?} mismatch", p);
        }
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn export_remaps_y_up_to_z_up() {
        // A voxel at Roxel (0, 5, 0) lives in block (cx=0, cy=0, cz=0). Inside
        // the BL16 it should land at Goxel local (gx=0, gy=0, gz=5), i.e. byte
        // offset (5*256 + 0*16 + 0)*4 = 5120.
        let mut g = VoxelGrid::default();
        g.set(IVec3::new(0, 5, 0), Some([10, 20, 30, 255]));
        let path = tmp_path("axis-export", "gox");
        export(&path, &g).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        let _ = std::fs::remove_file(&path);

        // Walk chunks to find BL16.
        let mut pos = 8;
        let mut found = None;
        while pos + 8 <= bytes.len() {
            let typ = &bytes[pos..pos + 4];
            let size = i32::from_le_bytes(bytes[pos + 4..pos + 8].try_into().unwrap()) as usize;
            pos += 8;
            if typ == b"BL16" {
                found = Some(bytes[pos..pos + size].to_vec());
                break;
            }
            pos += size + 4;
        }
        let block = found.expect("BL16 present");
        assert_eq!(block.len(), BLOCK_BYTES);
        // ly=0, lz=5, lx=0 → off = (0*256 + 5*16 + 0)*4 = 320 -- WAIT.
        // The mapping above (Roxel→Goxel block-local) is gx=lx=0, gy=lz=0,
        // gz=ly=5 in this voxel's case? Re-derive:
        //   Roxel voxel at world (0,5,0) → block-local (lx=0, ly=5, lz=0).
        //   Goxel block-local (gx=lx=0, gy=lz=0, gz=ly=5).
        //   Byte offset = (gz*256 + gy*16 + gx) * 4 = (5*256)*4 = 5120.
        let off = 5120;
        assert_eq!(&block[off..off + 4], &[10, 20, 30, 255]);
    }

    #[test]
    fn import_rejects_png_encoded_block() {
        // Forge a .gox with a PNG-magic'd BL16 payload (8 bytes total).
        let mut buf = Vec::new();
        buf.extend_from_slice(b"GOX ");
        buf.extend_from_slice(&2i32.to_le_bytes());
        // BL16 chunk with 8-byte fake PNG payload.
        let payload = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
        buf.extend_from_slice(b"BL16");
        buf.extend_from_slice(&(payload.len() as i32).to_le_bytes());
        buf.extend_from_slice(&payload);
        buf.extend_from_slice(&0i32.to_le_bytes());
        let path = tmp_path("png-block", "gox");
        std::fs::write(&path, &buf).unwrap();
        let mut g = VoxelGrid::default();
        let err = import(&path, &mut g).unwrap_err().to_string();
        assert!(err.contains("PNG"), "want PNG error, got: {err}");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn import_rejects_bad_magic() {
        let path = tmp_path("bad-magic", "gox");
        std::fs::write(&path, b"NOPE\x00\x00\x00\x00").unwrap();
        let mut g = VoxelGrid::default();
        assert!(import(&path, &mut g).is_err());
        let _ = std::fs::remove_file(&path);
    }
}
