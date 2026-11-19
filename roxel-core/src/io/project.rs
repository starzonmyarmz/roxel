//! `.rox` project files.
//!
//! Layout (all integers little-endian):
//!
//! ```text
//! "ROXL"  u16 format version
//! section*  = [u8; 4] tag, u32 length, payload
//! ```
//!
//! Readers skip sections they don't know, so new optional data can be added
//! without bumping [`FORMAT_VERSION`]. Sections:
//!
//! - `VOXL` (required): u32 decompressed length, then a zstd frame holding
//!   `u8 chunk edge E`, `u32 n` + `n` RGBA colors (sorted), `u32` chunk count,
//!   then per chunk (sorted by coord) `i32 cx, cy, cz`, a mode byte and cells.
//!   Cell values are color index + 1 (0 = empty), `w` bytes wide where `w` is
//!   1, 2 or 4 depending on `n`. Mode 0 (sparse): `u32` count, then
//!   `u16 local, value` pairs. Mode 1 (dense): `E³` values. Local index is
//!   `lx·E² + ly·E + lz`.
//! - `PALT` (optional): the palette that was active when saving.
//!   `u16` name length + UTF-8 name, `u32 n`, then `n` RGBA colors in
//!   palette order.
//! - `PREV` (optional): transparent PNG preview.
//!
//! Files that don't start with the magic are pre-0.9 RON projects and still
//! load.

use super::reader::LeReader;
use crate::grid::{CHUNK, Color8, VoxelGrid};
use crate::palette::Palette;
use anyhow::{Context, Result, bail, ensure};
use glam::IVec3;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::io::Read;
use std::path::Path;

const MAGIC: &[u8; 4] = b"ROXL";
pub const FORMAT_VERSION: u16 = 1;
const TAG_VOXELS: [u8; 4] = *b"VOXL";
const TAG_PALETTE: [u8; 4] = *b"PALT";
const TAG_PREVIEW: [u8; 4] = *b"PREV";
const ZSTD_LEVEL: i32 = 3;
const MAX_VOXEL_PAYLOAD: usize = 1 << 30;
/// Largest chunk edge whose `E³` local indices still fit a `u16`.
const MAX_CHUNK_EDGE: usize = 40;
const MODE_SPARSE: u8 = 0;
const MODE_DENSE: u8 = 1;

/// A decoded project, not yet applied to a grid.
#[derive(Debug, Default, PartialEq)]
pub struct Project {
    pub voxels: Vec<(IVec3, Color8)>,
    /// Palette active at save time. Loaded palettes are never `builtin`.
    pub palette: Option<Palette>,
    pub preview: Option<Vec<u8>>,
}

/// Writes the project, embedding the active `palette` and a transparent PNG
/// `preview` when given.
pub fn save(
    path: &Path,
    grid: &VoxelGrid,
    palette: Option<&Palette>,
    preview: Option<&[u8]>,
) -> Result<()> {
    let bytes = encode(grid, palette, preview)?;
    super::atomic::write(path, bytes)?;
    Ok(())
}

/// Replaces `grid`'s contents with the project at `path`. The file is fully
/// decoded first, so a load that fails leaves `grid` untouched.
pub fn load(path: &Path, grid: &mut VoxelGrid) -> Result<Project> {
    let bytes = std::fs::read(path)?;
    let mut project = decode(&bytes)?;
    grid.clear();
    for (p, c) in project.voxels.drain(..) {
        grid.set(p, Some(c));
    }
    Ok(project)
}

pub fn encode(
    grid: &VoxelGrid,
    palette: Option<&Palette>,
    preview: Option<&[u8]>,
) -> Result<Vec<u8>> {
    let body = encode_voxels(grid);
    let compressed = zstd::bulk::compress(&body, ZSTD_LEVEL)?;
    let mut voxl = Vec::with_capacity(4 + compressed.len());
    voxl.extend_from_slice(&len_u32(body.len())?.to_le_bytes());
    voxl.extend_from_slice(&compressed);

    let mut out = Vec::with_capacity(16 + voxl.len() + preview.map_or(0, <[u8]>::len));
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
    push_section(&mut out, TAG_VOXELS, &voxl)?;
    if let Some(palette) = palette {
        push_section(&mut out, TAG_PALETTE, &encode_palette(palette)?)?;
    }
    if let Some(png) = preview {
        push_section(&mut out, TAG_PREVIEW, png)?;
    }
    Ok(out)
}

pub fn decode(bytes: &[u8]) -> Result<Project> {
    if !bytes.starts_with(MAGIC) {
        return decode_legacy_ron(bytes);
    }
    let mut r = LeReader::new(&bytes[MAGIC.len()..]);
    let version = r.u16().context("missing format version")?;
    if version > FORMAT_VERSION {
        bail!("this project was saved by a newer version of Roxel (format {version})");
    }
    let mut voxels = None;
    let mut palette = None;
    let mut preview = None;
    while !r.eof() {
        let tag: [u8; 4] = r.bytes(4)?.try_into().unwrap();
        let len = r.u32()? as usize;
        let payload = r.bytes(len)?;
        match tag {
            TAG_VOXELS => voxels = Some(decode_voxels(payload)?),
            TAG_PALETTE => palette = Some(decode_palette(payload)?),
            TAG_PREVIEW => preview = Some(payload.to_vec()),
            _ => {}
        }
    }
    let voxels = voxels.context("project has no voxel data")?;
    Ok(Project {
        voxels,
        palette,
        preview,
    })
}

fn encode_palette(palette: &Palette) -> Result<Vec<u8>> {
    let name = palette.name.as_bytes();
    let name_len = u16::try_from(name.len()).context("palette name is too long")?;
    let mut out = Vec::with_capacity(6 + name.len() + palette.colors.len() * 4);
    out.extend_from_slice(&name_len.to_le_bytes());
    out.extend_from_slice(name);
    out.extend_from_slice(&len_u32(palette.colors.len())?.to_le_bytes());
    for c in &palette.colors {
        out.extend_from_slice(c);
    }
    Ok(out)
}

fn decode_palette(payload: &[u8]) -> Result<Palette> {
    let mut r = LeReader::new(payload);
    let name_len = r.u16()? as usize;
    let name = std::str::from_utf8(r.bytes(name_len)?)
        .context("palette name is not valid UTF-8")?
        .to_string();
    let n = r.u32()? as usize;
    let colors = r
        .bytes(n.checked_mul(4).context("too many palette colors")?)?
        .chunks_exact(4)
        .map(|c| c.try_into().unwrap())
        .collect();
    Ok(Palette {
        name,
        colors,
        builtin: false,
    })
}

fn push_section(out: &mut Vec<u8>, tag: [u8; 4], payload: &[u8]) -> Result<()> {
    out.extend_from_slice(&tag);
    out.extend_from_slice(&len_u32(payload.len())?.to_le_bytes());
    out.extend_from_slice(payload);
    Ok(())
}

fn len_u32(n: usize) -> Result<u32> {
    u32::try_from(n).context("project is too large to save")
}

fn value_width(colors: usize) -> usize {
    match colors {
        0..=0xFF => 1,
        0x100..=0xFFFF => 2,
        _ => 4,
    }
}

fn push_value(out: &mut Vec<u8>, v: u32, w: usize) {
    out.extend_from_slice(&v.to_le_bytes()[..w]);
}

fn read_value(r: &mut LeReader, w: usize) -> Result<u32> {
    let mut buf = [0u8; 4];
    buf[..w].copy_from_slice(r.bytes(w)?);
    Ok(u32::from_le_bytes(buf))
}

fn encode_voxels(grid: &VoxelGrid) -> Vec<u8> {
    let edge = CHUNK as i32;
    let vol = CHUNK * CHUNK * CHUNK;
    let mut chunks: BTreeMap<[i32; 3], Vec<(u16, Color8)>> = BTreeMap::new();
    let mut colors: Vec<Color8> = Vec::new();
    for (p, c) in grid.iter_occupied() {
        let coord = [
            p.x.div_euclid(edge),
            p.y.div_euclid(edge),
            p.z.div_euclid(edge),
        ];
        let [lx, ly, lz] = [p.x, p.y, p.z].map(|v| v.rem_euclid(edge) as usize);
        let local = (lx * CHUNK * CHUNK + ly * CHUNK + lz) as u16;
        chunks.entry(coord).or_default().push((local, c));
        colors.push(c);
    }
    colors.sort_unstable();
    colors.dedup();
    let w = value_width(colors.len());
    let value = |c: &Color8| colors.binary_search(c).unwrap() as u32 + 1;

    let mut out = Vec::new();
    out.push(CHUNK as u8);
    out.extend_from_slice(&(colors.len() as u32).to_le_bytes());
    for c in &colors {
        out.extend_from_slice(c);
    }
    out.extend_from_slice(&(chunks.len() as u32).to_le_bytes());
    for (coord, mut cells) in chunks {
        for v in coord {
            out.extend_from_slice(&v.to_le_bytes());
        }
        cells.sort_unstable_by_key(|(local, _)| *local);
        if 4 + cells.len() * (2 + w) <= vol * w {
            out.push(MODE_SPARSE);
            out.extend_from_slice(&(cells.len() as u32).to_le_bytes());
            for (local, c) in &cells {
                out.extend_from_slice(&local.to_le_bytes());
                push_value(&mut out, value(c), w);
            }
        } else {
            out.push(MODE_DENSE);
            let mut dense = vec![0u32; vol];
            for (local, c) in &cells {
                dense[*local as usize] = value(c);
            }
            for v in dense {
                push_value(&mut out, v, w);
            }
        }
    }
    out
}

fn decode_voxels(payload: &[u8]) -> Result<Vec<(IVec3, Color8)>> {
    let mut r = LeReader::new(payload);
    let raw_len = r.u32()? as usize;
    ensure!(raw_len <= MAX_VOXEL_PAYLOAD, "voxel data is too large");
    let frame = r.bytes(payload.len() - 4)?;
    let mut body = Vec::new();
    zstd::Decoder::new(frame)?
        .take(raw_len as u64 + 1)
        .read_to_end(&mut body)
        .context("voxel data is corrupt")?;
    ensure!(body.len() == raw_len, "voxel data has the wrong length");

    let mut r = LeReader::new(&body);
    let edge = r.u8()? as usize;
    ensure!(
        (1..=MAX_CHUNK_EDGE).contains(&edge),
        "invalid chunk size {edge}"
    );
    let vol = edge * edge * edge;
    let n_colors = r.u32()? as usize;
    let colors: Vec<Color8> = r
        .bytes(n_colors.checked_mul(4).context("too many colors")?)?
        .chunks_exact(4)
        .map(|c| c.try_into().unwrap())
        .collect();
    let w = value_width(n_colors);
    let color_of = |v: u32| -> Result<Color8> {
        match (v as usize).checked_sub(1).and_then(|i| colors.get(i)) {
            Some(c) => Ok(*c),
            None => bail!("invalid color index {v}"),
        }
    };

    let n_chunks = r.u32()?;
    let mut voxels = Vec::new();
    for _ in 0..n_chunks {
        let coord = [r.i32()?, r.i32()?, r.i32()?];
        let mut place = |local: usize, c: Color8| -> Result<()> {
            ensure!(local < vol, "invalid cell index {local}");
            let l = [local / (edge * edge), (local / edge) % edge, local % edge];
            let axis = |i: usize| {
                i32::try_from(i64::from(coord[i]) * edge as i64 + l[i] as i64)
                    .context("voxel coordinate out of range")
            };
            voxels.push((IVec3::new(axis(0)?, axis(1)?, axis(2)?), c));
            Ok(())
        };
        match r.u8()? {
            MODE_SPARSE => {
                let count = r.u32()? as usize;
                ensure!(count <= vol, "chunk holds too many cells");
                for _ in 0..count {
                    let local = r.u16()? as usize;
                    place(local, color_of(read_value(&mut r, w)?)?)?;
                }
            }
            MODE_DENSE => {
                for local in 0..vol {
                    let v = read_value(&mut r, w)?;
                    if v != 0 {
                        place(local, color_of(v)?)?;
                    }
                }
            }
            m => bail!("unknown chunk encoding {m}"),
        }
    }
    ensure!(r.eof(), "unexpected data after the last chunk");
    Ok(voxels)
}

/// Pre-0.9 projects: pretty-printed RON. Unknown fields (the old base64
/// `preview`, the even older `version`/`size`) are ignored.
#[derive(Deserialize)]
struct LegacyProjectFile {
    voxels: Vec<([i32; 3], Color8)>,
}

fn decode_legacy_ron(bytes: &[u8]) -> Result<Project> {
    let s = std::str::from_utf8(bytes).context("not a Roxel project")?;
    let pf: LegacyProjectFile = ron::from_str(s)?;
    Ok(Project {
        voxels: pf
            .voxels
            .into_iter()
            .map(|(p, c)| (IVec3::from_array(p), c))
            .collect(),
        palette: None,
        preview: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io::test_util::tmp_path as raw_tmp_path;
    use std::path::PathBuf;

    fn tmp_path(name: &str) -> PathBuf {
        raw_tmp_path(name, "rox")
    }

    fn grid_of(pts: &[(IVec3, Color8)]) -> VoxelGrid {
        let mut g = VoxelGrid::default();
        for (p, c) in pts {
            g.set(*p, Some(*c));
        }
        g
    }

    fn sorted(mut v: Vec<(IVec3, Color8)>) -> Vec<(IVec3, Color8)> {
        v.sort_by_key(|(p, _)| p.to_array());
        v
    }

    #[test]
    fn roundtrip_preserves_voxels() {
        let pts = [
            (IVec3::new(0, 0, 0), [10, 20, 30, 255]),
            (IVec3::new(5, 6, 7), [200, 100, 50, 255]),
            (IVec3::new(63, 63, 63), [1, 2, 3, 128]),
        ];
        let g = grid_of(&pts);
        let path = tmp_path("roundtrip");
        save(&path, &g, None, None).expect("save");

        let mut loaded = VoxelGrid::default();
        loaded.set(IVec3::new(1, 1, 1), Some([9, 9, 9, 255]));
        load(&path, &mut loaded).expect("load");

        for (p, c) in pts {
            assert_eq!(loaded.get(p), Some(c));
        }
        assert_eq!(loaded.count(), 3);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn roundtrip_with_negative_and_far_voxels() {
        let pts = [
            (IVec3::new(-50, 0, -25), [10, 20, 30, 255]),
            (IVec3::new(500, 100, -300), [200, 100, 50, 255]),
            (IVec3::new(0, 0, 0), [1, 2, 3, 255]),
            (IVec3::new(-1, 31, -33), [1, 2, 3, 255]),
        ];
        let decoded = decode(&encode(&grid_of(&pts), None, None).unwrap()).unwrap();
        assert_eq!(sorted(decoded.voxels), sorted(pts.to_vec()));
    }

    #[test]
    fn dense_chunks_roundtrip() {
        let mut pts = Vec::new();
        for x in 0..32 {
            for y in 0..32 {
                for z in 0..32 {
                    pts.push((IVec3::new(x, y, z), [(x * 8) as u8, y as u8, z as u8, 255]));
                }
            }
        }
        let bytes = encode(&grid_of(&pts), None, None).unwrap();
        let decoded = decode(&bytes).unwrap();
        assert_eq!(sorted(decoded.voxels), sorted(pts));
    }

    #[test]
    fn wide_color_tables_roundtrip() {
        let pts: Vec<_> = (0..300)
            .map(|i| {
                (
                    IVec3::new(i, 0, 0),
                    [(i % 256) as u8, (i / 256) as u8, 7, 255],
                )
            })
            .collect();
        let decoded = decode(&encode(&grid_of(&pts), None, None).unwrap()).unwrap();
        assert_eq!(sorted(decoded.voxels), sorted(pts));
    }

    #[test]
    fn empty_grid_roundtrips() {
        let decoded = decode(&encode(&VoxelGrid::default(), None, None).unwrap()).unwrap();
        assert!(decoded.voxels.is_empty());
    }

    #[test]
    fn preview_roundtrips() {
        let g = grid_of(&[(IVec3::ZERO, [1, 2, 3, 255])]);
        let png = [0x89, b'P', b'N', b'G', 1, 2, 3];
        let decoded = decode(&encode(&g, None, Some(&png)).unwrap()).unwrap();
        assert_eq!(decoded.preview.as_deref(), Some(&png[..]));
    }

    #[test]
    fn palette_roundtrips_in_order() {
        let palette = Palette {
            name: "Sunset ☀".into(),
            colors: vec![[255, 0, 0, 255], [0, 0, 0, 255], [10, 20, 30, 128]],
            builtin: true,
        };
        let g = grid_of(&[(IVec3::ZERO, [1, 2, 3, 255])]);
        let decoded = decode(&encode(&g, Some(&palette), Some(&[7, 7])).unwrap()).unwrap();
        let loaded = decoded.palette.expect("palette");
        assert_eq!(loaded.name, palette.name);
        assert_eq!(loaded.colors, palette.colors);
        assert!(!loaded.builtin);
        assert_eq!(decoded.preview.as_deref(), Some(&[7, 7][..]));
    }

    #[test]
    fn projects_without_a_palette_load_with_none() {
        let g = grid_of(&[(IVec3::ZERO, [1, 2, 3, 255])]);
        assert!(
            decode(&encode(&g, None, None).unwrap())
                .unwrap()
                .palette
                .is_none()
        );
    }

    #[test]
    fn malformed_palette_section_errors() {
        let g = grid_of(&[(IVec3::ZERO, [1, 2, 3, 255])]);
        for payload in [
            &[5u8, 0, b'a'][..],
            &[1, 0, 0xFF, 0, 0, 0, 0][..],
            &[0, 0, 0xFF, 0xFF, 0xFF, 0xFF][..],
        ] {
            let mut bytes = encode(&g, None, None).unwrap();
            push_section(&mut bytes, TAG_PALETTE, payload).unwrap();
            assert!(decode(&bytes).is_err(), "{payload:?}");
        }
    }

    #[test]
    fn saves_are_deterministic_regardless_of_insertion_order() {
        let pts: Vec<_> = (0..500)
            .map(|i| {
                let p = IVec3::new((i * 37) % 97 - 48, (i * 13) % 41, (i * 7) % 89 - 44);
                (p, [(i % 5) as u8 * 40, 10, 20, 255])
            })
            .collect();
        let forward = grid_of(&pts);
        let mut reversed_pts = pts.clone();
        reversed_pts.reverse();
        let mut reversed = VoxelGrid::default();
        for (p, _) in &reversed_pts {
            reversed.set(*p, forward.get(*p));
        }
        assert_eq!(
            encode(&forward, None, None).unwrap(),
            encode(&reversed, None, None).unwrap()
        );
    }

    #[test]
    fn file_is_far_smaller_than_ron() {
        let mut pts = Vec::new();
        for x in 0..64 {
            for y in 0..32 {
                for z in 0..64 {
                    let c = ((x * 7 + y * 3 + z * 5) % 16) as u8;
                    pts.push((IVec3::new(x, y, z), [c * 16, 255 - c * 8, c, 255]));
                }
            }
        }
        let bytes = encode(&grid_of(&pts), None, None).unwrap();
        assert!(
            bytes.len() < pts.len(),
            "{} bytes for {} voxels",
            bytes.len(),
            pts.len()
        );
    }

    #[test]
    fn starts_with_magic_and_version() {
        let bytes = encode(&VoxelGrid::default(), None, None).unwrap();
        assert_eq!(&bytes[..4], b"ROXL");
        assert_eq!(u16::from_le_bytes([bytes[4], bytes[5]]), FORMAT_VERSION);
    }

    #[test]
    fn unknown_sections_are_skipped() {
        let g = grid_of(&[(IVec3::new(1, 2, 3), [4, 5, 6, 255])]);
        let mut bytes = encode(&g, None, None).unwrap();
        push_section(&mut bytes, *b"XTRA", b"future data").unwrap();
        let decoded = decode(&bytes).unwrap();
        assert_eq!(decoded.voxels, vec![(IVec3::new(1, 2, 3), [4, 5, 6, 255])]);
    }

    #[test]
    fn newer_format_version_is_refused() {
        let mut bytes = encode(&VoxelGrid::default(), None, None).unwrap();
        bytes[4..6].copy_from_slice(&(FORMAT_VERSION + 1).to_le_bytes());
        let err = decode(&bytes).unwrap_err().to_string();
        assert!(err.contains("newer version"), "{err}");
    }

    #[test]
    fn truncated_files_error_instead_of_panicking() {
        let g = grid_of(&[
            (IVec3::new(1, 2, 3), [4, 5, 6, 255]),
            (IVec3::new(-40, 9, 70), [7, 8, 9, 255]),
        ]);
        let bytes = encode(&g, None, None).unwrap();
        for len in 0..bytes.len() {
            assert!(decode(&bytes[..len]).is_err(), "len {len}");
        }
    }

    #[test]
    fn missing_voxel_section_errors() {
        let mut bytes = MAGIC.to_vec();
        bytes.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
        assert!(decode(&bytes).is_err());
    }

    fn voxl_file(body: &[u8]) -> Vec<u8> {
        let mut voxl = (body.len() as u32).to_le_bytes().to_vec();
        voxl.extend_from_slice(&zstd::bulk::compress(body, 1).unwrap());
        let mut out = MAGIC.to_vec();
        out.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
        push_section(&mut out, TAG_VOXELS, &voxl).unwrap();
        out
    }

    #[test]
    fn malformed_voxel_payloads_error() {
        let one_color = |rest: &[u8]| {
            let mut b = vec![32u8];
            b.extend_from_slice(&1u32.to_le_bytes());
            b.extend_from_slice(&[1, 2, 3, 255]);
            b.extend_from_slice(&1u32.to_le_bytes());
            b.extend_from_slice(&[0; 12]);
            b.extend_from_slice(rest);
            b
        };
        let cases: Vec<Vec<u8>> = vec![
            vec![0],
            vec![200],
            one_color(&[9]),
            one_color(&[0, 1, 0, 0, 0, 0, 0, 2]),
            one_color(&[0, 1, 0, 0, 0, 0, 0, 0]),
            one_color(&[0, 1, 0, 0, 0, 0xFF, 0xFF, 1]),
            one_color(&[0, 0xFF, 0xFF, 0xFF, 0xFF]),
        ];
        for body in cases {
            assert!(decode(&voxl_file(&body)).is_err(), "{body:?}");
        }
    }

    #[test]
    fn oversized_decompressed_length_is_refused() {
        let mut out = MAGIC.to_vec();
        out.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
        push_section(&mut out, TAG_VOXELS, &u32::MAX.to_le_bytes()).unwrap();
        assert!(decode(&out).is_err());
    }

    #[test]
    fn voxel_data_with_the_wrong_declared_length_errors() {
        let g = grid_of(&[(IVec3::new(1, 2, 3), [4, 5, 6, 255])]);
        let good = encode(&g, None, None).unwrap();
        let at = MAGIC.len() + 2 + TAG_VOXELS.len() + 4;
        for declared in [0u32, 1, 1 << 29] {
            let mut bytes = good.clone();
            bytes[at..at + 4].copy_from_slice(&declared.to_le_bytes());
            assert!(decode(&bytes).is_err(), "declared {declared}");
        }
    }

    #[test]
    fn trailing_bytes_after_the_last_chunk_error() {
        let g = grid_of(&[(IVec3::new(1, 2, 3), [4, 5, 6, 255])]);
        let mut body = encode_voxels(&g);
        body.push(0);
        assert!(decode(&voxl_file(&body)).is_err());
    }

    #[test]
    fn failed_load_leaves_grid_untouched() {
        let path = tmp_path("corrupt");
        std::fs::write(&path, b"ROXL\x01\x00VOXL\xff\xff\xff\xff").unwrap();
        let mut g = grid_of(&[(IVec3::new(1, 1, 1), [9, 9, 9, 255])]);
        assert!(load(&path, &mut g).is_err());
        assert_eq!(g.get(IVec3::new(1, 1, 1)), Some([9, 9, 9, 255]));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn legacy_ron_projects_still_load() {
        let ron = "(voxels: [((1, 2, 3), (10, 20, 30, 255)), ((-4, 0, 5), (1, 1, 1, 255))], preview: Some(\"iVBORw0KGgo=\"))";
        let path = tmp_path("legacy");
        std::fs::write(&path, ron).unwrap();
        let mut g = VoxelGrid::default();
        load(&path, &mut g).expect("load");
        assert_eq!(g.get(IVec3::new(1, 2, 3)), Some([10, 20, 30, 255]));
        assert_eq!(g.get(IVec3::new(-4, 0, 5)), Some([1, 1, 1, 255]));
        assert_eq!(g.count(), 2);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn pre_open_world_extra_fields_are_silently_ignored() {
        let v1 = "(version: 1, size: [32, 32, 32], voxels: [((1, 2, 3), (10, 20, 30, 255))])";
        let decoded = decode(v1.as_bytes()).expect("decode");
        assert_eq!(
            decoded.voxels,
            vec![(IVec3::new(1, 2, 3), [10, 20, 30, 255])]
        );
    }

    #[test]
    fn garbage_is_rejected() {
        assert!(decode(&[0xFF, 0xFE, 0x00]).is_err());
        assert!(decode(b"not a project").is_err());
    }
}
