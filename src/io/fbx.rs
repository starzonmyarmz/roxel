use crate::grid::VoxelGrid;
use crate::mesh::FACES;
use anyhow::Result;
use std::io::Write;
use std::path::Path;

// Binary FBX 7.4. Accepted by Blender, Maya, 3ds Max, Unity, Unreal.
pub fn export(path: &Path, grid: &VoxelGrid) -> Result<()> {
    let (verts, polys, normals, colors) = build_mesh(grid);

    let mut b = FbxBuilder::new();
    write_header_ext(&mut b);
    write_global_settings(&mut b);
    write_documents(&mut b);
    write_references(&mut b);
    write_definitions(&mut b);
    write_objects(&mut b, &verts, &polys, &normals, &colors);
    write_connections(&mut b);
    write_takes(&mut b);

    // Top-level NULL terminator.
    b.buf.extend_from_slice(&[0u8; 13]);
    write_footer(&mut b.buf);

    let mut file = std::io::BufWriter::new(std::fs::File::create(path)?);
    // 23-byte ASCII magic + NULL + 0x1a + NULL = 27 bytes total, then version.
    file.write_all(b"Kaydara FBX Binary  ")?;
    file.write_all(&[0x00, 0x1a, 0x00])?;
    file.write_all(&7400u32.to_le_bytes())?;
    file.write_all(&b.buf)?;
    Ok(())
}

fn build_mesh(grid: &VoxelGrid) -> (Vec<f64>, Vec<i32>, Vec<f64>, Vec<f64>) {
    let mut verts: Vec<f64> = Vec::new();
    let mut polys: Vec<i32> = Vec::new();
    let mut normals: Vec<f64> = Vec::new();
    let mut colors: Vec<f64> = Vec::new();
    let mut vidx: i32 = 0;

    let size_i = grid.size_i();
    for x in 0..grid.size {
        for y in 0..grid.size {
            for z in 0..grid.size {
                let Some(rgba) = grid.cell(x, y, z) else {
                    continue;
                };
                let cx = x as i32;
                let cy = y as i32;
                let cz = z as i32;
                let r = rgba[0] as f64 / 255.0;
                let g = rgba[1] as f64 / 255.0;
                let bl = rgba[2] as f64 / 255.0;
                let a = rgba[3] as f64 / 255.0;

                for f in &FACES {
                    let nx = cx + f.d.x;
                    let ny = cy + f.d.y;
                    let nz = cz + f.d.z;
                    let neighbor_filled = nx >= 0
                        && nx < size_i
                        && ny >= 0
                        && ny < size_i
                        && nz >= 0
                        && nz < size_i
                        && grid.cell(nx as usize, ny as usize, nz as usize).is_some();
                    if neighbor_filled {
                        continue;
                    }

                    let mut idx = [0i32; 4];
                    for (i, c) in f.corners.iter().enumerate() {
                        verts.push((cx + c[0]) as f64);
                        verts.push((cy + c[1]) as f64);
                        verts.push((cz + c[2]) as f64);
                        idx[i] = vidx;
                        vidx += 1;
                    }
                    polys.push(idx[0]);
                    polys.push(idx[1]);
                    polys.push(idx[2]);
                    polys.push(-(idx[3]) - 1);

                    for _ in 0..4 {
                        normals.push(f.normal[0] as f64);
                        normals.push(f.normal[1] as f64);
                        normals.push(f.normal[2] as f64);
                        colors.push(r);
                        colors.push(g);
                        colors.push(bl);
                        colors.push(a);
                    }
                }
            }
        }
    }
    (verts, polys, normals, colors)
}

const GEO_ID: i64 = 100_000;
const MODEL_ID: i64 = 200_000;
const DOC_ID: i64 = 300_000;

fn write_header_ext(b: &mut FbxBuilder) {
    b.begin("FBXHeaderExtension");
    b.leaf_i32("FBXHeaderVersion", 1003);
    b.leaf_i32("FBXVersion", 7400);
    b.begin("CreationTimeStamp");
    b.leaf_i32("Version", 1000);
    b.leaf_i32("Year", 2026);
    b.leaf_i32("Month", 1);
    b.leaf_i32("Day", 1);
    b.leaf_i32("Hour", 0);
    b.leaf_i32("Minute", 0);
    b.leaf_i32("Second", 0);
    b.leaf_i32("Millisecond", 0);
    b.end();
    b.leaf_str("Creator", "Roxel");
    b.begin("SceneInfo");
    b.prop_str("SceneInfo::GlobalInfo");
    b.prop_str("UserData");
    b.leaf_str("Type", "UserData");
    b.leaf_i32("Version", 100);
    b.begin("MetaData");
    b.leaf_i32("Version", 100);
    b.leaf_str("Title", "");
    b.leaf_str("Subject", "");
    b.leaf_str("Author", "");
    b.leaf_str("Keywords", "");
    b.leaf_str("Revision", "");
    b.leaf_str("Comment", "");
    b.end();
    b.begin("Properties70");
    b.end();
    b.end();
    b.end();

    b.begin("FileId");
    b.prop_raw(&[0u8; 16]);
    b.end();
    b.leaf_str("CreationTime", "2026-01-01 00:00:00:000");
    b.leaf_str("Creator", "Roxel");
}

fn write_global_settings(b: &mut FbxBuilder) {
    b.begin("GlobalSettings");
    b.leaf_i32("Version", 1000);
    b.begin("Properties70");
    p70_int(b, "UpAxis", 1);
    p70_int(b, "UpAxisSign", 1);
    p70_int(b, "FrontAxis", 2);
    p70_int(b, "FrontAxisSign", 1);
    p70_int(b, "CoordAxis", 0);
    p70_int(b, "CoordAxisSign", 1);
    p70_int(b, "OriginalUpAxis", 1);
    p70_int(b, "OriginalUpAxisSign", 1);
    p70_double(b, "UnitScaleFactor", 1.0);
    p70_double(b, "OriginalUnitScaleFactor", 1.0);
    b.end();
    b.end();
}

fn write_documents(b: &mut FbxBuilder) {
    b.begin("Documents");
    b.leaf_i32("Count", 1);
    b.begin("Document");
    b.prop_i64(DOC_ID);
    b.prop_str("Scene");
    b.prop_str("Scene");
    b.begin("Properties70");
    b.end();
    b.begin("RootNode");
    b.prop_i64(0);
    b.end();
    b.end();
    b.end();
}

fn write_references(b: &mut FbxBuilder) {
    b.begin("References");
    b.end();
}

fn write_definitions(b: &mut FbxBuilder) {
    b.begin("Definitions");
    b.leaf_i32("Version", 100);
    b.leaf_i32("Count", 3);

    b.begin("ObjectType");
    b.prop_str("GlobalSettings");
    b.leaf_i32("Count", 1);
    b.end();

    b.begin("ObjectType");
    b.prop_str("Geometry");
    b.leaf_i32("Count", 1);
    b.begin("PropertyTemplate");
    b.prop_str("FbxMesh");
    b.begin("Properties70");
    b.end();
    b.end();
    b.end();

    b.begin("ObjectType");
    b.prop_str("Model");
    b.leaf_i32("Count", 1);
    b.begin("PropertyTemplate");
    b.prop_str("FbxNode");
    b.begin("Properties70");
    b.end();
    b.end();
    b.end();
}

fn write_objects(
    b: &mut FbxBuilder,
    verts: &[f64],
    polys: &[i32],
    normals: &[f64],
    colors: &[f64],
) {
    b.begin("Objects");

    // Geometry
    b.begin("Geometry");
    b.prop_i64(GEO_ID);
    b.prop_str("Geometry::voxels");
    b.prop_str("Mesh");
    b.begin("Vertices");
    b.prop_arr_f64(verts);
    b.end();
    b.begin("PolygonVertexIndex");
    b.prop_arr_i32(polys);
    b.end();
    b.leaf_i32("GeometryVersion", 124);

    b.begin("LayerElementNormal");
    b.prop_i32(0);
    b.leaf_i32("Version", 101);
    b.leaf_str("Name", "");
    b.leaf_str("MappingInformationType", "ByPolygonVertex");
    b.leaf_str("ReferenceInformationType", "Direct");
    b.begin("Normals");
    b.prop_arr_f64(normals);
    b.end();
    b.end();

    b.begin("LayerElementColor");
    b.prop_i32(0);
    b.leaf_i32("Version", 101);
    b.leaf_str("Name", "colorSet");
    b.leaf_str("MappingInformationType", "ByPolygonVertex");
    b.leaf_str("ReferenceInformationType", "Direct");
    b.begin("Colors");
    b.prop_arr_f64(colors);
    b.end();
    b.end();

    b.begin("Layer");
    b.prop_i32(0);
    b.leaf_i32("Version", 100);
    b.begin("LayerElement");
    b.leaf_str("Type", "LayerElementNormal");
    b.leaf_i32("TypedIndex", 0);
    b.end();
    b.begin("LayerElement");
    b.leaf_str("Type", "LayerElementColor");
    b.leaf_i32("TypedIndex", 0);
    b.end();
    b.end();

    b.end();

    // Model
    b.begin("Model");
    b.prop_i64(MODEL_ID);
    b.prop_str("Model::voxels");
    b.prop_str("Mesh");
    b.leaf_i32("Version", 232);
    b.begin("Properties70");
    b.end();
    b.leaf_bool("Shading", true);
    b.leaf_str("Culling", "CullingOff");
    b.end();

    b.end();
}

fn write_connections(b: &mut FbxBuilder) {
    b.begin("Connections");
    b.begin("C");
    b.prop_str("OO");
    b.prop_i64(MODEL_ID);
    b.prop_i64(0);
    b.end();
    b.begin("C");
    b.prop_str("OO");
    b.prop_i64(GEO_ID);
    b.prop_i64(MODEL_ID);
    b.end();
    b.end();
}

fn write_takes(b: &mut FbxBuilder) {
    b.begin("Takes");
    b.leaf_str("Current", "");
    b.end();
}

fn p70_int(b: &mut FbxBuilder, name: &str, v: i32) {
    b.begin("P");
    b.prop_str(name);
    b.prop_str("int");
    b.prop_str("Integer");
    b.prop_str("");
    b.prop_i32(v);
    b.end();
}

fn p70_double(b: &mut FbxBuilder, name: &str, v: f64) {
    b.begin("P");
    b.prop_str(name);
    b.prop_str("double");
    b.prop_str("Number");
    b.prop_str("");
    b.prop_f64(v);
    b.end();
}

const FOOT_MAGIC: [u8; 16] = [
    0xfa, 0xbc, 0xab, 0x09, 0xd0, 0xc8, 0xd4, 0x66, 0xb1, 0x76, 0xfb, 0x83, 0x1c, 0xf7, 0x26, 0x7e,
];

fn write_footer(buf: &mut Vec<u8>) {
    // Includes the 27-byte file header + 4-byte version that the file starts with,
    // but footer alignment is computed from the file's total position. We're appending
    // to `buf` (which represents bytes after the header+version), so the actual file
    // pos = header(27) + version(4) + buf.len() = 31 + buf.len().
    const FILE_HEADER_LEN: usize = 31;

    buf.extend_from_slice(&[0u8; 16]); // foot_id (zeros accepted)

    let pos = FILE_HEADER_LEN + buf.len();
    let pad = (16 - (pos % 16)) % 16;
    let pad = if pad < 4 { pad + 16 } else { pad };
    buf.extend(std::iter::repeat(0u8).take(pad));

    buf.extend_from_slice(&[0u8; 4]);
    buf.extend_from_slice(&7400u32.to_le_bytes());
    buf.extend_from_slice(&[0u8; 120]);
    buf.extend_from_slice(&FOOT_MAGIC);
}

struct FbxBuilder {
    buf: Vec<u8>,
    stack: Vec<NodeFrame>,
}

struct NodeFrame {
    start: usize,
    props_start: usize,
    props_end: Option<usize>,
    num_props: u32,
    child_count: u32,
}

impl FbxBuilder {
    fn new() -> Self {
        Self {
            buf: Vec::with_capacity(1 << 16),
            stack: Vec::new(),
        }
    }

    fn begin(&mut self, name: &str) {
        let start = self.buf.len();
        if let Some(parent) = self.stack.last_mut() {
            parent.child_count += 1;
            if parent.props_end.is_none() {
                parent.props_end = Some(start);
            }
        }
        self.buf.extend_from_slice(&[0u8; 12]);
        self.buf.push(name.len() as u8);
        self.buf.extend_from_slice(name.as_bytes());
        let props_start = self.buf.len();
        self.stack.push(NodeFrame {
            start,
            props_start,
            props_end: None,
            num_props: 0,
            child_count: 0,
        });
    }

    fn end(&mut self) {
        let has_children;
        let num_props;
        let prop_list_len;
        let start;
        {
            let frame = self.stack.last_mut().unwrap();
            let props_end = frame.props_end.unwrap_or(self.buf.len());
            prop_list_len = (props_end - frame.props_start) as u32;
            num_props = frame.num_props;
            start = frame.start;
            has_children = frame.child_count > 0;
        }
        self.stack.pop();

        if has_children {
            self.buf.extend_from_slice(&[0u8; 13]);
        }

        let end_off = self.buf.len() as u32;
        let header = &mut self.buf[start..start + 12];
        header[0..4].copy_from_slice(&end_off.to_le_bytes());
        header[4..8].copy_from_slice(&num_props.to_le_bytes());
        header[8..12].copy_from_slice(&prop_list_len.to_le_bytes());
    }

    fn prop_i32(&mut self, v: i32) {
        self.stack.last_mut().unwrap().num_props += 1;
        self.buf.push(b'I');
        self.buf.extend_from_slice(&v.to_le_bytes());
    }
    fn prop_i64(&mut self, v: i64) {
        self.stack.last_mut().unwrap().num_props += 1;
        self.buf.push(b'L');
        self.buf.extend_from_slice(&v.to_le_bytes());
    }
    fn prop_f64(&mut self, v: f64) {
        self.stack.last_mut().unwrap().num_props += 1;
        self.buf.push(b'D');
        self.buf.extend_from_slice(&v.to_le_bytes());
    }
    fn prop_bool(&mut self, v: bool) {
        self.stack.last_mut().unwrap().num_props += 1;
        self.buf.push(b'C');
        self.buf.push(if v { 1 } else { 0 });
    }
    fn prop_str(&mut self, s: &str) {
        self.stack.last_mut().unwrap().num_props += 1;
        self.buf.push(b'S');
        self.buf
            .extend_from_slice(&(s.len() as u32).to_le_bytes());
        self.buf.extend_from_slice(s.as_bytes());
    }
    fn prop_raw(&mut self, bytes: &[u8]) {
        self.stack.last_mut().unwrap().num_props += 1;
        self.buf.push(b'R');
        self.buf
            .extend_from_slice(&(bytes.len() as u32).to_le_bytes());
        self.buf.extend_from_slice(bytes);
    }
    fn prop_arr_f64(&mut self, arr: &[f64]) {
        self.stack.last_mut().unwrap().num_props += 1;
        self.buf.push(b'd');
        self.buf
            .extend_from_slice(&(arr.len() as u32).to_le_bytes());
        self.buf.extend_from_slice(&0u32.to_le_bytes()); // encoding raw
        self.buf
            .extend_from_slice(&((arr.len() * 8) as u32).to_le_bytes());
        for v in arr {
            self.buf.extend_from_slice(&v.to_le_bytes());
        }
    }
    fn prop_arr_i32(&mut self, arr: &[i32]) {
        self.stack.last_mut().unwrap().num_props += 1;
        self.buf.push(b'i');
        self.buf
            .extend_from_slice(&(arr.len() as u32).to_le_bytes());
        self.buf.extend_from_slice(&0u32.to_le_bytes());
        self.buf
            .extend_from_slice(&((arr.len() * 4) as u32).to_le_bytes());
        for v in arr {
            self.buf.extend_from_slice(&v.to_le_bytes());
        }
    }

    fn leaf_i32(&mut self, name: &str, v: i32) {
        self.begin(name);
        self.prop_i32(v);
        self.end();
    }
    fn leaf_bool(&mut self, name: &str, v: bool) {
        self.begin(name);
        self.prop_bool(v);
        self.end();
    }
    fn leaf_str(&mut self, name: &str, v: &str) {
        self.begin(name);
        self.prop_str(v);
        self.end();
    }
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
        p.push(format!("roxel-test-{pid}-{nanos}-{name}.fbx"));
        p
    }

    #[test]
    fn export_writes_header_magic_and_version() {
        let mut g = VoxelGrid::default();
        g.set(IVec3::new(0, 0, 0), Some([100, 100, 100, 255]));
        let path = tmp_path("header");
        export(&path, &g).expect("export");
        let bytes = std::fs::read(&path).expect("read");
        // 23-byte ASCII magic.
        assert_eq!(&bytes[0..20], b"Kaydara FBX Binary  ");
        assert_eq!(bytes[20], 0x00);
        assert_eq!(bytes[21], 0x1a);
        assert_eq!(bytes[22], 0x00);
        // Version 7400 at bytes 23..27 little-endian.
        let version = u32::from_le_bytes([bytes[23], bytes[24], bytes[25], bytes[26]]);
        assert_eq!(version, 7400);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn export_terminates_with_foot_magic() {
        let mut g = VoxelGrid::default();
        g.set(IVec3::new(0, 0, 0), Some([100, 100, 100, 255]));
        let path = tmp_path("footer");
        export(&path, &g).expect("export");
        let bytes = std::fs::read(&path).expect("read");
        let tail = &bytes[bytes.len() - FOOT_MAGIC.len()..];
        assert_eq!(tail, &FOOT_MAGIC);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn build_mesh_single_voxel_emits_six_quads() {
        let mut g = VoxelGrid::default();
        g.set(IVec3::new(0, 0, 0), Some([1, 2, 3, 255]));
        let (verts, polys, normals, colors) = build_mesh(&g);
        // 6 faces × 4 corners = 24 verts × 3 coords = 72 doubles.
        assert_eq!(verts.len(), 72);
        // polys: 4 indices per face, last index encoded as -(idx+1).
        assert_eq!(polys.len(), 24);
        // 6 faces × 4 corners × 3 components (per-vertex normals).
        assert_eq!(normals.len(), 72);
        // 24 vertex-colors × 4 channels.
        assert_eq!(colors.len(), 96);
    }

    #[test]
    fn build_mesh_adjacent_voxels_drop_shared_face() {
        let mut g = VoxelGrid::default();
        g.set(IVec3::new(0, 0, 0), Some([1, 2, 3, 255]));
        g.set(IVec3::new(1, 0, 0), Some([4, 5, 6, 255]));
        let (_verts, polys, _normals, _colors) = build_mesh(&g);
        // 10 faces × 4 indices.
        assert_eq!(polys.len(), 40);
    }

    #[test]
    fn polygon_index_run_terminates_with_negated_last() {
        let mut g = VoxelGrid::default();
        g.set(IVec3::new(0, 0, 0), Some([1, 2, 3, 255]));
        let (_, polys, _, _) = build_mesh(&g);
        // Every 4th index is the run terminator: -(idx + 1) < 0.
        for chunk in polys.chunks_exact(4) {
            assert!(chunk[3] < 0, "expected negated terminator, got {}", chunk[3]);
        }
    }
}
