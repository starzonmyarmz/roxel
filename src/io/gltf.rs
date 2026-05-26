use crate::grid::VoxelGrid;
use crate::mesh::for_each_exposed_face;
use anyhow::Result;
use std::io::Write;
use std::path::Path;

// glTF 2.0 binary (.glb) exporter. Single-file, embedded buffer. Y-up matches
// the spec default and Roxel's runtime orientation, so Unity and Godot import
// upright with no extra transform. Per-face quads with per-vertex sRGB colors
// in COLOR_0; greedy meshing intentionally skipped so the importer sees the
// same face-quad structure as the OBJ export.

const GLB_MAGIC: u32 = 0x46546C67; // "glTF"
const GLB_VERSION: u32 = 2;
const CHUNK_JSON: u32 = 0x4E4F534A; // "JSON"
const CHUNK_BIN: u32 = 0x004E4942; // "BIN\0"

pub fn export(path: &Path, grid: &VoxelGrid) -> Result<()> {
    let mesh = build_mesh(grid);
    let bin = pack_bin(&mesh);
    let json = build_json(&mesh, bin.len() as u32);

    let mut file = std::io::BufWriter::new(std::fs::File::create(path)?);
    write_glb(&mut file, &json, &bin)?;
    Ok(())
}

struct MeshData {
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    colors: Vec<[u8; 4]>,
    indices: Vec<u32>,
    pos_min: [f32; 3],
    pos_max: [f32; 3],
}

fn build_mesh(grid: &VoxelGrid) -> MeshData {
    let mut positions: Vec<[f32; 3]> = Vec::new();
    let mut normals: Vec<[f32; 3]> = Vec::new();
    let mut colors: Vec<[u8; 4]> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();
    let mut pos_min = [f32::INFINITY; 3];
    let mut pos_max = [f32::NEG_INFINITY; 3];

    for_each_exposed_face(grid, |cell, face, rgba| {
        let base = positions.len() as u32;
        for c in &face.corners {
            let v = [
                (cell.x + c[0]) as f32,
                (cell.y + c[1]) as f32,
                (cell.z + c[2]) as f32,
            ];
            for k in 0..3 {
                if v[k] < pos_min[k] {
                    pos_min[k] = v[k];
                }
                if v[k] > pos_max[k] {
                    pos_max[k] = v[k];
                }
            }
            positions.push(v);
            normals.push(face.normal);
            colors.push(rgba);
        }
        // Two triangles per quad: (0,1,2) + (0,2,3). FACES corners are CCW
        // viewed from outside, which matches glTF's default front-face winding.
        indices.push(base);
        indices.push(base + 1);
        indices.push(base + 2);
        indices.push(base);
        indices.push(base + 2);
        indices.push(base + 3);
    });

    if positions.is_empty() {
        pos_min = [0.0; 3];
        pos_max = [0.0; 3];
    }

    MeshData {
        positions,
        normals,
        colors,
        indices,
        pos_min,
        pos_max,
    }
}

fn pack_bin(mesh: &MeshData) -> Vec<u8> {
    let mut bin = Vec::new();
    // Positions: f32 vec3.
    for p in &mesh.positions {
        for v in p {
            bin.extend_from_slice(&v.to_le_bytes());
        }
    }
    pad_to_4(&mut bin);
    let _normals_start = bin.len();
    for n in &mesh.normals {
        for v in n {
            bin.extend_from_slice(&v.to_le_bytes());
        }
    }
    pad_to_4(&mut bin);
    let _colors_start = bin.len();
    for c in &mesh.colors {
        bin.extend_from_slice(c);
    }
    pad_to_4(&mut bin);
    let _indices_start = bin.len();
    for i in &mesh.indices {
        bin.extend_from_slice(&i.to_le_bytes());
    }
    pad_to_4(&mut bin);
    bin
}

fn pad_to_4(buf: &mut Vec<u8>) {
    while buf.len() % 4 != 0 {
        buf.push(0);
    }
}

fn build_json(mesh: &MeshData, bin_len: u32) -> Vec<u8> {
    let pos_bytes = mesh.positions.len() * 12;
    let pos_padded = align4(pos_bytes);
    let normals_bytes = mesh.normals.len() * 12;
    let normals_padded = align4(normals_bytes);
    let colors_bytes = mesh.colors.len() * 4;
    let colors_padded = align4(colors_bytes);
    let indices_bytes = mesh.indices.len() * 4;
    let _indices_padded = align4(indices_bytes);

    let pos_off = 0;
    let normals_off = pos_off + pos_padded;
    let colors_off = normals_off + normals_padded;
    let indices_off = colors_off + colors_padded;

    let pos_min = mesh.pos_min;
    let pos_max = mesh.pos_max;

    let count = mesh.positions.len();
    let idx_count = mesh.indices.len();

    let json = format!(
        r#"{{"asset":{{"version":"2.0","generator":"Roxel"}},"scene":0,"scenes":[{{"nodes":[0]}}],"nodes":[{{"mesh":0,"name":"voxels"}}],"meshes":[{{"name":"voxels","primitives":[{{"attributes":{{"POSITION":0,"NORMAL":1,"COLOR_0":2}},"indices":3,"mode":4}}]}}],"accessors":[{{"bufferView":0,"componentType":5126,"count":{count},"type":"VEC3","min":[{pmin0},{pmin1},{pmin2}],"max":[{pmax0},{pmax1},{pmax2}]}},{{"bufferView":1,"componentType":5126,"count":{count},"type":"VEC3"}},{{"bufferView":2,"componentType":5121,"count":{count},"type":"VEC4","normalized":true}},{{"bufferView":3,"componentType":5125,"count":{idx_count},"type":"SCALAR"}}],"bufferViews":[{{"buffer":0,"byteOffset":{pos_off},"byteLength":{pos_bytes},"target":34962}},{{"buffer":0,"byteOffset":{normals_off},"byteLength":{normals_bytes},"target":34962}},{{"buffer":0,"byteOffset":{colors_off},"byteLength":{colors_bytes},"target":34962}},{{"buffer":0,"byteOffset":{indices_off},"byteLength":{indices_bytes},"target":34963}}],"buffers":[{{"byteLength":{bin_len}}}]}}"#,
        count = count,
        idx_count = idx_count,
        pos_off = pos_off,
        normals_off = normals_off,
        colors_off = colors_off,
        indices_off = indices_off,
        pos_bytes = pos_bytes,
        normals_bytes = normals_bytes,
        colors_bytes = colors_bytes,
        indices_bytes = indices_bytes,
        bin_len = bin_len,
        pmin0 = fmt_f32(pos_min[0]),
        pmin1 = fmt_f32(pos_min[1]),
        pmin2 = fmt_f32(pos_min[2]),
        pmax0 = fmt_f32(pos_max[0]),
        pmax1 = fmt_f32(pos_max[1]),
        pmax2 = fmt_f32(pos_max[2]),
    );
    let mut bytes = json.into_bytes();
    // Pad JSON chunk with spaces to 4-byte boundary (spec requirement).
    while bytes.len() % 4 != 0 {
        bytes.push(b' ');
    }
    bytes
}

fn align4(n: usize) -> usize {
    (n + 3) & !3
}

fn fmt_f32(v: f32) -> String {
    if v.fract() == 0.0 {
        format!("{}", v as i64)
    } else {
        format!("{v}")
    }
}

fn write_glb<W: Write>(w: &mut W, json: &[u8], bin: &[u8]) -> Result<()> {
    let total_len = 12 + 8 + json.len() + 8 + bin.len();
    w.write_all(&GLB_MAGIC.to_le_bytes())?;
    w.write_all(&GLB_VERSION.to_le_bytes())?;
    w.write_all(&(total_len as u32).to_le_bytes())?;
    w.write_all(&(json.len() as u32).to_le_bytes())?;
    w.write_all(&CHUNK_JSON.to_le_bytes())?;
    w.write_all(json)?;
    w.write_all(&(bin.len() as u32).to_le_bytes())?;
    w.write_all(&CHUNK_BIN.to_le_bytes())?;
    w.write_all(bin)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io::test_util::tmp_path as raw_tmp_path;
    use bevy::math::IVec3;
    use std::path::PathBuf;

    fn tmp_path(name: &str) -> PathBuf {
        raw_tmp_path(name, "glb")
    }

    #[test]
    fn export_writes_glb_magic_and_version() {
        let mut g = VoxelGrid::default();
        g.set(IVec3::new(0, 0, 0), Some([255, 0, 0, 255]));
        let path = tmp_path("magic");
        export(&path, &g).expect("export");
        let bytes = std::fs::read(&path).expect("read");
        assert!(bytes.len() > 12);
        assert_eq!(&bytes[0..4], b"glTF");
        let version = u32::from_le_bytes(bytes[4..8].try_into().unwrap());
        assert_eq!(version, 2);
        let total = u32::from_le_bytes(bytes[8..12].try_into().unwrap()) as usize;
        assert_eq!(total, bytes.len());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn export_chunks_are_4_byte_aligned() {
        let mut g = VoxelGrid::default();
        // Three voxels of different colors → varied buffer sizes.
        g.set(IVec3::new(0, 0, 0), Some([255, 0, 0, 255]));
        g.set(IVec3::new(1, 0, 0), Some([0, 255, 0, 255]));
        g.set(IVec3::new(2, 1, 0), Some([0, 0, 255, 255]));
        let path = tmp_path("align");
        export(&path, &g).expect("export");
        let bytes = std::fs::read(&path).expect("read");

        // JSON chunk header at offset 12.
        let json_len = u32::from_le_bytes(bytes[12..16].try_into().unwrap()) as usize;
        let json_type = u32::from_le_bytes(bytes[16..20].try_into().unwrap());
        assert_eq!(json_type, CHUNK_JSON);
        assert_eq!(json_len % 4, 0, "JSON chunk length must be 4-byte aligned");

        let bin_header_off = 20 + json_len;
        let bin_len = u32::from_le_bytes(
            bytes[bin_header_off..bin_header_off + 4]
                .try_into()
                .unwrap(),
        ) as usize;
        let bin_type = u32::from_le_bytes(
            bytes[bin_header_off + 4..bin_header_off + 8]
                .try_into()
                .unwrap(),
        );
        assert_eq!(bin_type, CHUNK_BIN);
        assert_eq!(bin_len % 4, 0, "BIN chunk length must be 4-byte aligned");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn empty_grid_exports_minimal_glb() {
        let g = VoxelGrid::default();
        let path = tmp_path("empty");
        export(&path, &g).expect("export");
        let bytes = std::fs::read(&path).expect("read");
        assert_eq!(&bytes[0..4], b"glTF");
        // Should still parse — header + json + (possibly empty) bin chunk.
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn single_voxel_emits_24_vertices_36_indices() {
        let mut g = VoxelGrid::default();
        g.set(IVec3::new(0, 0, 0), Some([10, 20, 30, 255]));
        let mesh = build_mesh(&g);
        assert_eq!(mesh.positions.len(), 24, "6 faces × 4 verts");
        assert_eq!(mesh.indices.len(), 36, "6 faces × 2 tris × 3 verts");
        for c in &mesh.colors {
            assert_eq!(*c, [10, 20, 30, 255]);
        }
    }

    #[test]
    fn neighboring_voxels_omit_shared_faces() {
        let mut g = VoxelGrid::default();
        g.set(IVec3::new(0, 0, 0), Some([255, 0, 0, 255]));
        g.set(IVec3::new(1, 0, 0), Some([0, 255, 0, 255]));
        let mesh = build_mesh(&g);
        // Each voxel exposes 5 faces (one shared face hidden) = 10 faces total.
        assert_eq!(mesh.positions.len(), 40);
        assert_eq!(mesh.indices.len(), 60);
    }
}
