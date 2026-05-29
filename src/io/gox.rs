use crate::grid::VoxelGrid;
use crate::io::reader::LeReader;
use anyhow::{Result, anyhow, bail};
use bevy::math::IVec3;
use std::collections::HashMap;
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
// Roxel writes BL16 blocks as raw 16×16×16×4 RGBA bytes. Foreign Goxel files
// that store BL16 as a PNG image are rejected with a clear error. Goxel is
// Z-up, so positions are remapped to Roxel's Y-up convention on both read
// and write. .gox supports negative block coordinates natively — no
// AABB-shift on export.
//
// Block-local byte offset for voxel (lx, ly, lz) in Goxel coords:
//   offset = (lz * 256 + ly * 16 + lx) * 4
// Goxel Y-up→Z-up remap: Roxel(rx, ry, rz) → Goxel(gx = rx, gy = rz, gz = ry).

const BLOCK_SIZE: usize = 16;
const BLOCK_VOXELS: usize = BLOCK_SIZE * BLOCK_SIZE * BLOCK_SIZE;
const BLOCK_BYTES: usize = BLOCK_VOXELS * 4;
const BLOCK_I: i32 = BLOCK_SIZE as i32;
const PNG_MAGIC: [u8; 4] = [0x89, 0x50, 0x4E, 0x47];

/// A 16³ block keyed by its block coord: `((bx, by, bz), raw RGBA bytes)`.
type BlockEntry = ((i32, i32, i32), Box<[u8; BLOCK_BYTES]>);

pub fn export(path: &Path, grid: &VoxelGrid) -> Result<()> {
    let mut buf = Vec::new();
    buf.extend_from_slice(b"GOX ");
    buf.extend_from_slice(&2i32.to_le_bytes());

    let mut img = Vec::new();
    img.extend_from_slice(&0i32.to_le_bytes());
    write_chunk(&mut buf, b"IMG ", &img);

    // Bucket Roxel voxels into 16³ blocks keyed by 16-block coord. The grid's
    // own 32³ chunk layout doesn't line up with .gox's 16³ blocks, so we re-
    // bucket here. Block at Roxel block-coord (bx, by, bz) → Goxel position
    // (bx*16, bz*16, by*16) after axis remap.
    let mut blocks: HashMap<(i32, i32, i32), Box<[u8; BLOCK_BYTES]>> = HashMap::new();
    for (p, c) in grid.iter_occupied() {
        let bx = p.x.div_euclid(BLOCK_I);
        let by = p.y.div_euclid(BLOCK_I);
        let bz = p.z.div_euclid(BLOCK_I);
        let lx = p.x.rem_euclid(BLOCK_I) as usize;
        let ly = p.y.rem_euclid(BLOCK_I) as usize;
        let lz = p.z.rem_euclid(BLOCK_I) as usize;
        let entry = blocks
            .entry((bx, by, bz))
            .or_insert_with(|| Box::new([0u8; BLOCK_BYTES]));
        // Roxel local (lx, ly, lz) → Goxel block-local (gx=lx, gy=lz, gz=ly).
        let off = (ly * 256 + lz * 16 + lx) * 4;
        entry[off] = c[0];
        entry[off + 1] = c[1];
        entry[off + 2] = c[2];
        entry[off + 3] = 255;
    }

    let mut entries: Vec<BlockEntry> = blocks.into_iter().collect();
    entries.sort_by_key(|((bx, by, bz), _)| (*bx, *by, *bz));

    for (_, data) in &entries {
        write_chunk(&mut buf, b"BL16", data.as_ref());
    }

    let mut layr = Vec::new();
    layr.extend_from_slice(&(entries.len() as i32).to_le_bytes());
    for (i, ((bx, by, bz), _)) in entries.iter().enumerate() {
        layr.extend_from_slice(&(i as i32).to_le_bytes());
        // Y-up→Z-up: (gx, gy, gz) = (bx*16, bz*16, by*16).
        layr.extend_from_slice(&(bx * BLOCK_I).to_le_bytes());
        layr.extend_from_slice(&(bz * BLOCK_I).to_le_bytes());
        layr.extend_from_slice(&(by * BLOCK_I).to_le_bytes());
        layr.extend_from_slice(&[0u8; 8]);
    }
    write_dict_entry(&mut layr, "name", b"Layer 0");
    write_dict_entry(&mut layr, "visible", &[1u8]);
    layr.extend_from_slice(&0i32.to_le_bytes()); // dict terminator
    write_chunk(&mut buf, b"LAYR", &layr);

    std::fs::write(path, &buf)?;
    Ok(())
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
                    continue;
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
            _ => {}
        }
    }

    let refs = refs.ok_or_else(|| anyhow!("no LAYR chunk in file"))?;
    if refs.is_empty() {
        return Ok(());
    }

    let mut dropped = 0usize;
    for (idx, gx, gy, gz) in refs {
        let Some(block) = blocks.get(idx as usize) else {
            continue;
        };
        // Goxel block min in Roxel coords: (rx, ry, rz) = (gx, gz, gy).
        let base_rx = gx;
        let base_ry = gz;
        let base_rz = gy;
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
                    if ry < 0 {
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
        eprintln!("Import .gox: dropped {dropped} voxels below the floor");
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
        // Loading an empty file should leave g2 alone (no LAYR refs to apply).
        // The previous bounded version cleared via resize; the open-world
        // loader does not — we instead test that no new cells get added.
        let count_before = g2.count();
        // An empty grid export still writes a LAYR with 0 refs; import is a
        // no-op on the grid.
        let result = import(&path, &mut g2);
        assert!(result.is_ok());
        assert_eq!(g2.count(), count_before);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn roundtrip_preserves_voxels_across_block_boundary() {
        let mut g = VoxelGrid::default();
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
    fn roundtrip_handles_negative_coords() {
        let mut g = VoxelGrid::default();
        let pts: [(IVec3, [u8; 4]); 2] = [
            (IVec3::new(-30, 5, -10), [10, 20, 30, 255]),
            (IVec3::new(15, 0, 7), [40, 50, 60, 255]),
        ];
        for (p, c) in pts {
            g.set(p, Some(c));
        }
        let path = tmp_path("negative", "gox");
        export(&path, &g).unwrap();
        let mut g2 = VoxelGrid::default();
        import(&path, &mut g2).unwrap();
        for (p, c) in pts {
            assert_eq!(g2.get(p), Some(c), "voxel at {p:?} mismatch");
        }
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn export_remaps_y_up_to_z_up() {
        let mut g = VoxelGrid::default();
        g.set(IVec3::new(0, 5, 0), Some([10, 20, 30, 255]));
        let path = tmp_path("axis-export", "gox");
        export(&path, &g).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        let _ = std::fs::remove_file(&path);

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
        // Roxel (0, 5, 0) → block-local (lx=0, ly=5, lz=0) → Goxel byte offset
        // (ly*256 + lz*16 + lx)*4 = 5120.
        let off = 5120;
        assert_eq!(&block[off..off + 4], &[10, 20, 30, 255]);
    }

    #[test]
    fn import_rejects_png_encoded_block() {
        let mut buf = Vec::new();
        buf.extend_from_slice(b"GOX ");
        buf.extend_from_slice(&2i32.to_le_bytes());
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
