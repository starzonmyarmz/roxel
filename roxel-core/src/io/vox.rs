use crate::grid::{Color8, VoxelGrid};
use anyhow::Result;
use dot_vox::{Color, DotVoxData, Model, SceneNode, Size, Voxel};
use glam::IVec3;
use std::collections::{BTreeSet, HashMap};
use std::path::Path;

/// `.vox` format hard cap on each axis: voxel coords are stored in a `u8`,
/// so the largest extent any one axis can carry is 256.
const VOX_MAX_EXTENT: i32 = 256;

// MagicaVoxel is Z-up; Roxel is Y-up. Both import and export remap
// (x, y_roxel, z_roxel) <-> (x_vox, z_vox, y_vox) so foreign .vox files load
// upright and Roxel-exported files open upright in MagicaVoxel.

/// Writes `grid` as a single-model `.vox`. Returns how many colors past the
/// format's 255 were merged into their nearest palette entry.
pub fn export(path: &Path, grid: &VoxelGrid) -> Result<usize> {
    // Open-world grids can carry negative coords; `.vox` coords are unsigned
    // and start at origin. Shift the whole model by `-min` so its min corner
    // lands at (0, 0, 0) in MagicaVoxel space. The shift is per-axis on the
    // *remapped* axes so the user sees the model upright in MV.
    let Some((min, max)) = grid.bounding_box() else {
        anyhow::bail!("nothing to export — grid is empty");
    };
    let extent = max - min;
    // After remap to MV (x, z_roxel, y_roxel) the size axes are still bounded
    // by the same per-axis extents — check each.
    if extent.x + 1 > VOX_MAX_EXTENT
        || extent.y + 1 > VOX_MAX_EXTENT
        || extent.z + 1 > VOX_MAX_EXTENT
    {
        anyhow::bail!(
            ".vox supports a maximum extent of {VOX_MAX_EXTENT} per axis; \
             this scene measures {}×{}×{}",
            extent.x + 1,
            extent.y + 1,
            extent.z + 1
        );
    }

    let (mut palette, palette_map, merged) = build_palette(grid);
    let mut voxels: Vec<Voxel> = Vec::new();

    for (p, c) in grid.iter_occupied() {
        let shifted = p - min;
        let idx = palette_map[&c];
        // Y-up → Z-up: emit (x, z_roxel, y_roxel) so columns point along MV's +Z.
        voxels.push(Voxel {
            x: shifted.x as u8,
            y: shifted.z as u8,
            z: shifted.y as u8,
            i: idx,
        });
    }

    while palette.len() < 256 {
        palette.push(Color {
            r: 0,
            g: 0,
            b: 0,
            a: 0,
        });
    }

    let data = DotVoxData {
        version: 150,
        index_map: (0u8..=255u8).collect(),
        models: vec![Model {
            size: Size {
                x: (extent.x + 1) as u32,
                y: (extent.z + 1) as u32,
                z: (extent.y + 1) as u32,
            },
            voxels,
        }],
        palette,
        materials: vec![],
        scenes: vec![],
        layers: vec![],
    };
    super::atomic::write_with(path, |file| data.write_vox(file))?;
    Ok(merged)
}

/// `.vox` palette slots usable for colors (index 0 is reserved as empty).
const VOX_MAX_COLORS: usize = 255;

/// The export palette and each scene color's index into it. The most-used
/// colors (ties broken by value, so output is deterministic) get exact slots;
/// any past [`VOX_MAX_COLORS`] map to their nearest slot. Also returns how
/// many colors were merged that way.
fn build_palette(grid: &VoxelGrid) -> (Vec<Color>, HashMap<Color8, u8>, usize) {
    let mut counts: HashMap<Color8, usize> = HashMap::new();
    for (_, c) in grid.iter_occupied() {
        *counts.entry(c).or_default() += 1;
    }
    let mut by_use: Vec<(Color8, usize)> = counts.into_iter().collect();
    by_use.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));

    let exact = by_use.len().min(VOX_MAX_COLORS);
    let palette: Vec<Color> = by_use[..exact]
        .iter()
        .map(|(c, _)| Color {
            r: c[0],
            g: c[1],
            b: c[2],
            a: 255,
        })
        .collect();
    let mut map: HashMap<Color8, u8> = by_use[..exact]
        .iter()
        .enumerate()
        .map(|(i, (c, _))| (*c, i as u8))
        .collect();
    for (c, _) in &by_use[exact..] {
        map.insert(*c, nearest(&palette, *c));
    }
    (palette, map, by_use.len() - exact)
}

/// Imports a `.vox` model into `grid` and returns the model's palette as the
/// distinct colors actually referenced by placed voxels, in ascending palette
/// index order. The caller turns this into a swatch palette; the full 256-entry
/// MagicaVoxel ramp is deliberately *not* returned — unused default entries
/// would be palette noise.
pub fn import(path: &Path, grid: &mut VoxelGrid) -> Result<Vec<Color8>> {
    let path_str = path
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("non-UTF-8 path"))?;
    let data = dot_vox::load(path_str).map_err(|e| anyhow::anyhow!("{e}"))?;
    place_models(&data, grid)
}

/// Places every visible model instance from the scene graph, then shifts the
/// whole import so its lowest model box corner sits at the origin — a single
/// untransformed model lands at its raw voxel coords, as before.
fn place_models(data: &DotVoxData, grid: &mut VoxelGrid) -> Result<Vec<Color8>> {
    if data.models.is_empty() {
        anyhow::bail!("file contains no models");
    }
    let mut instances = Vec::new();
    if data.scenes.is_empty() {
        instances.extend((0..data.models.len()).map(|i| (i, Xform::IDENTITY)));
    } else {
        collect_instances(data, 0, Xform::IDENTITY, 0, &mut instances);
    }

    let mut placed: Vec<([i64; 3], u8)> = Vec::new();
    let mut min = [i64::MAX; 3];
    for (model_id, xf) in instances {
        let Some(model) = data.models.get(model_id) else {
            continue;
        };
        let size = [model.size.x, model.size.y, model.size.z].map(i64::from);
        if size.contains(&0) {
            continue;
        }
        for corner in 0..8 {
            let v = [0, 1, 2].map(|a| if corner >> a & 1 == 1 { size[a] - 1 } else { 0 });
            let c = mv_to_roxel(xf.place(v, size));
            min = [0, 1, 2].map(|a| min[a].min(c[a]));
        }
        for v in &model.voxels {
            let cell = xf.place([v.x, v.y, v.z].map(i64::from), size);
            placed.push((mv_to_roxel(cell), v.i));
        }
    }

    let mut used: BTreeSet<u8> = BTreeSet::new();
    let mut cells = Vec::with_capacity(placed.len());
    for (p, i) in placed {
        let cell = [0, 1, 2].map(|a| i32::try_from(p[a] - min[a]));
        let [Ok(x), Ok(y), Ok(z)] = cell else {
            anyhow::bail!("Import .vox: scene extends past the supported coordinate range");
        };
        cells.push((IVec3::new(x, y, z), i));
    }
    for (p, i) in cells {
        used.insert(i);
        grid.set(p, Some(palette_color(data, i)));
    }
    Ok(used.into_iter().map(|i| palette_color(data, i)).collect())
}

fn palette_color(data: &DotVoxData, i: u8) -> Color8 {
    let c = data.palette.get(i as usize).copied().unwrap_or(Color {
        r: 255,
        g: 255,
        b: 255,
        a: 255,
    });
    [c.r, c.g, c.b, 255]
}

/// MagicaVoxel (x, y, z) Z-up → Roxel (x, z, y) Y-up.
fn mv_to_roxel(p: [i64; 3]) -> [i64; 3] {
    [p[0], p[2], p[1]]
}

/// Deep enough for any real scene; stops a cyclic graph from recursing forever.
const MAX_SCENE_DEPTH: usize = 64;

fn collect_instances(
    data: &DotVoxData,
    node: u32,
    parent: Xform,
    depth: usize,
    out: &mut Vec<(usize, Xform)>,
) {
    if depth > MAX_SCENE_DEPTH {
        return;
    }
    match data.scenes.get(node as usize) {
        Some(SceneNode::Transform {
            attributes,
            frames,
            child,
            layer_id,
        }) => {
            let layer_hidden = data
                .layers
                .get(*layer_id as usize)
                .is_some_and(|l| l.attributes.get("_hidden").is_some_and(|v| v == "1"));
            if layer_hidden || attributes.get("_hidden").is_some_and(|v| v == "1") {
                return;
            }
            let local = frames.first().map_or(Xform::IDENTITY, Xform::from_frame);
            collect_instances(data, *child, parent.then(local), depth + 1, out);
        }
        Some(SceneNode::Group { children, .. }) => {
            for &c in children {
                collect_instances(data, c, parent, depth + 1, out);
            }
        }
        Some(SceneNode::Shape { models, .. }) => {
            if let Some(m) = models.first() {
                out.push((m.model_id as usize, parent));
            }
        }
        None => {}
    }
}

/// A scene-graph transform: signed permutation matrix `r` (row-major, in
/// MagicaVoxel's Z-up space) and translation `t`. `i64` so hostile
/// translations nested [`MAX_SCENE_DEPTH`] deep can't overflow.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Xform {
    r: [[i64; 3]; 3],
    t: [i64; 3],
}

impl Xform {
    const IDENTITY: Xform = Xform {
        r: [[1, 0, 0], [0, 1, 0], [0, 0, 1]],
        t: [0, 0, 0],
    };

    fn from_frame(frame: &dot_vox::Frame) -> Xform {
        let get = |k: &str| frame.attributes.get(k).map(String::as_str);
        Xform {
            r: get("_r")
                .and_then(parse_rotation)
                .unwrap_or(Xform::IDENTITY.r),
            t: get("_t").and_then(parse_translation).unwrap_or([0; 3]),
        }
    }

    fn rotate(&self, p: [i64; 3]) -> [i64; 3] {
        [0, 1, 2].map(|row| (0..3).map(|c| self.r[row][c] * p[c]).sum())
    }

    /// `self` applied after `child`.
    fn then(self, child: Xform) -> Xform {
        let r = [0, 1, 2]
            .map(|row| [0, 1, 2].map(|col| (0..3).map(|k| self.r[row][k] * child.r[k][col]).sum()));
        let rt = self.rotate(child.t);
        Xform {
            r,
            t: [0, 1, 2].map(|a| rt[a] + self.t[a]),
        }
    }

    /// World cell of voxel `v` in a model of `size`. Models rotate about
    /// their center, so this works in doubled coordinates where voxel
    /// centers relative to it are whole numbers, then floors back.
    fn place(&self, v: [i64; 3], size: [i64; 3]) -> [i64; 3] {
        let centered = [0, 1, 2].map(|a| 2 * v[a] + 1 - size[a]);
        let r = self.rotate(centered);
        [0, 1, 2].map(|a| (r[a] + 2 * self.t[a]).div_euclid(2))
    }
}

/// `_r`: bits 0-1 and 2-3 give the column of row 0's and row 1's nonzero
/// entry (row 2 takes the remaining one); bits 4-6 make a row negative.
/// Invalid bytes return `None` (dot_vox's own parser panics on them).
fn parse_rotation(s: &str) -> Option<[[i64; 3]; 3]> {
    let b: u8 = s.trim().parse().ok()?;
    let c0 = (b & 3) as usize;
    let c1 = (b >> 2 & 3) as usize;
    if c0 == 3 || c1 == 3 || c0 == c1 {
        return None;
    }
    let cols = [c0, c1, 3 - c0 - c1];
    let mut r = [[0; 3]; 3];
    for (row, &col) in cols.iter().enumerate() {
        r[row][col] = if b >> (4 + row) & 1 == 1 { -1 } else { 1 };
    }
    Some(r)
}

fn parse_translation(s: &str) -> Option<[i64; 3]> {
    let mut it = s.split_whitespace().map(str::parse::<i32>);
    let t = [it.next()?.ok()?, it.next()?.ok()?, it.next()?.ok()?].map(i64::from);
    it.next().is_none().then_some(t)
}

fn nearest(palette: &[Color], c: Color8) -> u8 {
    let mut best = 0u8;
    let mut best_d = i32::MAX;
    for (i, p) in palette.iter().enumerate() {
        let dr = p.r as i32 - c[0] as i32;
        let dg = p.g as i32 - c[1] as i32;
        let db = p.b as i32 - c[2] as i32;
        let d = dr * dr + dg * dg + db * db;
        if d < best_d {
            best_d = d;
            best = i as u8;
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io::test_util::tmp_path as raw_tmp_path;
    use glam::IVec3;
    use std::path::PathBuf;

    fn tmp_path(name: &str) -> PathBuf {
        raw_tmp_path(name, "vox")
    }

    fn make_palette(rgb: &[(u8, u8, u8)]) -> Vec<Color> {
        let mut palette: Vec<Color> = rgb
            .iter()
            .map(|&(r, g, b)| Color { r, g, b, a: 255 })
            .collect();
        while palette.len() < 256 {
            palette.push(Color {
                r: 0,
                g: 0,
                b: 0,
                a: 0,
            });
        }
        palette
    }

    #[test]
    fn export_remaps_y_up_to_z_up() {
        let mut g = VoxelGrid::default();
        g.set(IVec3::new(0, 5, 0), Some([10, 20, 30, 255]));
        let path = tmp_path("axis-export");
        export(&path, &g).expect("export");
        let data = dot_vox::load(path.to_str().unwrap()).expect("load");
        let v = &data.models[0].voxels[0];
        // After AABB-shift, (0, 5, 0) is at the model's min corner → (0, 0, 0)
        // in Roxel space, which becomes (0, 0, 0) in MV after axis remap.
        assert_eq!((v.x, v.y, v.z), (0, 0, 0));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn export_shifts_to_origin_for_negative_coords() {
        let mut g = VoxelGrid::default();
        g.set(IVec3::new(-5, 0, -10), Some([1, 2, 3, 255]));
        g.set(IVec3::new(5, 7, 0), Some([4, 5, 6, 255]));
        let path = tmp_path("shift");
        export(&path, &g).expect("export");
        let data = dot_vox::load(path.to_str().unwrap()).expect("load");
        // Model size is the AABB extent (max - min + 1) after axis remap:
        // x = 5 - (-5) + 1 = 11, y_mv = z_roxel extent = 0 - (-10) + 1 = 11,
        // z_mv = y_roxel extent = 7 - 0 + 1 = 8.
        let model = &data.models[0];
        assert_eq!(model.size.x, 11);
        assert_eq!(model.size.y, 11);
        assert_eq!(model.size.z, 8);
        // First voxel was at Roxel min → MV (0, 0, 0) after both shift and remap.
        let zero = model.voxels.iter().any(|v| (v.x, v.y, v.z) == (0, 0, 0));
        assert!(zero, "voxels = {:?}", model.voxels);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn export_refuses_when_extent_exceeds_256() {
        let mut g = VoxelGrid::default();
        g.set(IVec3::new(0, 0, 0), Some([1, 2, 3, 255]));
        g.set(IVec3::new(300, 0, 0), Some([4, 5, 6, 255]));
        let path = tmp_path("too-big");
        let err = export(&path, &g).expect_err("must refuse");
        assert!(err.to_string().contains("256"), "msg={err}");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn import_remaps_z_up_to_y_up() {
        let path = tmp_path("axis-import");
        let palette = make_palette(&[(99, 88, 77)]);
        let data = DotVoxData {
            version: 150,
            index_map: (0u8..=255u8).collect(),
            models: vec![Model {
                size: Size {
                    x: 32,
                    y: 32,
                    z: 32,
                },
                voxels: vec![Voxel {
                    x: 0,
                    y: 0,
                    z: 5,
                    i: 0,
                }],
            }],
            palette,
            materials: vec![],
            scenes: vec![],
            layers: vec![],
        };
        let mut f = std::fs::File::create(&path).expect("create");
        data.write_vox(&mut f).expect("write");

        let mut g = VoxelGrid::default();
        import(&path, &mut g).expect("import");
        assert_eq!(g.get(IVec3::new(0, 5, 0)), Some([99, 88, 77, 255]));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn import_roundtrips_through_export() {
        let mut g = VoxelGrid::default();
        let pts: [(IVec3, [u8; 4]); 3] = [
            (IVec3::new(0, 0, 0), [255, 0, 0, 255]),
            (IVec3::new(5, 12, 7), [0, 255, 0, 255]),
            (IVec3::new(31, 31, 31), [0, 0, 255, 255]),
        ];
        for (p, c) in pts {
            g.set(p, Some(c));
        }
        let path = tmp_path("roundtrip");
        export(&path, &g).expect("export");

        let mut loaded = VoxelGrid::default();
        loaded.set(IVec3::new(1, 1, 1), Some([9, 9, 9, 255]));
        import(&path, &mut loaded).expect("import");

        // After roundtrip the model is AABB-shifted to min=(0,0,0), so the
        // input pts land at (p - (0,0,0)) — same coords.
        for (p, c) in pts {
            assert_eq!(loaded.get(p), Some(c));
        }
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn import_does_not_resize_grid() {
        // Open world has no resize; the importer just `grid.set`s each cell.
        let path = tmp_path("noresize");
        let palette = make_palette(&[(1, 2, 3)]);
        let data = DotVoxData {
            version: 150,
            index_map: (0u8..=255u8).collect(),
            models: vec![Model {
                size: Size {
                    x: 40,
                    y: 40,
                    z: 40,
                },
                voxels: vec![Voxel {
                    x: 39,
                    y: 0,
                    z: 0,
                    i: 0,
                }],
            }],
            palette,
            materials: vec![],
            scenes: vec![],
            layers: vec![],
        };
        let mut f = std::fs::File::create(&path).unwrap();
        data.write_vox(&mut f).unwrap();
        let mut g = VoxelGrid::default();
        import(&path, &mut g).expect("import");
        // Voxel landed at (39, 0, 0) — well outside the old 32³ default,
        // proving the open-world grid took it without resizing.
        assert_eq!(g.get(IVec3::new(39, 0, 0)), Some([1, 2, 3, 255]));
        let _ = std::fs::remove_file(&path);
    }

    fn model(size: (u32, u32, u32), voxels: &[(u8, u8, u8, u8)]) -> Model {
        Model {
            size: Size {
                x: size.0,
                y: size.1,
                z: size.2,
            },
            voxels: voxels
                .iter()
                .map(|&(x, y, z, i)| Voxel { x, y, z, i })
                .collect(),
        }
    }

    fn dict(pairs: &[(&str, &str)]) -> dot_vox::Dict {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    fn trn(child: u32, frame: &[(&str, &str)], attrs: &[(&str, &str)]) -> SceneNode {
        SceneNode::Transform {
            attributes: dict(attrs),
            frames: vec![dot_vox::Frame::new(dict(frame))],
            child,
            layer_id: 0,
        }
    }

    fn shp(model_id: u32) -> SceneNode {
        SceneNode::Shape {
            attributes: dict(&[]),
            models: vec![dot_vox::ShapeModel {
                model_id,
                attributes: dict(&[]),
            }],
        }
    }

    fn scene_data(models: Vec<Model>, scenes: Vec<SceneNode>) -> DotVoxData {
        DotVoxData {
            version: 150,
            index_map: (0u8..=255u8).collect(),
            models,
            palette: make_palette(&[(10, 10, 10), (200, 200, 200)]),
            materials: vec![],
            scenes,
            layers: vec![],
        }
    }

    /// Root transform → group → one transform + shape per model, the second
    /// translated 10 along MV x; `hide_second` marks its transform hidden.
    fn two_model_scene(hide_second: bool) -> DotVoxData {
        let hidden: &[(&str, &str)] = if hide_second {
            &[("_hidden", "1")]
        } else {
            &[]
        };
        scene_data(
            vec![
                model((4, 4, 4), &[(0, 0, 0, 0)]),
                model((4, 4, 4), &[(0, 0, 0, 1)]),
            ],
            vec![
                trn(1, &[], &[]),
                SceneNode::Group {
                    attributes: dict(&[]),
                    children: vec![2, 4],
                },
                trn(3, &[("_t", "0 0 0")], &[]),
                shp(0),
                trn(5, &[("_t", "10 0 0")], hidden),
                shp(1),
            ],
        )
    }

    #[test]
    fn import_places_every_model_at_its_transform() {
        let mut g = VoxelGrid::default();
        place_models(&two_model_scene(false), &mut g).unwrap();
        assert_eq!(g.count(), 2);
        assert_eq!(g.get(IVec3::new(0, 0, 0)), Some([10, 10, 10, 255]));
        assert_eq!(g.get(IVec3::new(10, 0, 0)), Some([200, 200, 200, 255]));
    }

    #[test]
    fn import_reads_the_scene_graph_from_disk() {
        let path = tmp_path("multi");
        let mut f = std::fs::File::create(&path).unwrap();
        two_model_scene(false).write_vox(&mut f).unwrap();
        let mut g = VoxelGrid::default();
        import(&path, &mut g).expect("import");
        let _ = std::fs::remove_file(&path);
        assert_eq!(g.get(IVec3::new(10, 0, 0)), Some([200, 200, 200, 255]));
    }

    #[test]
    fn import_skips_hidden_nodes() {
        let mut g = VoxelGrid::default();
        let colors = place_models(&two_model_scene(true), &mut g).unwrap();
        assert_eq!(g.count(), 1);
        assert_eq!(colors, vec![[10, 10, 10, 255]]);
    }

    #[test]
    fn import_applies_rotation_about_the_model_center() {
        // _r 17: row 0 takes -y, row 1 takes x — 90° about MV's Z. The
        // model's MV-x line turns to run along MV y, which is Roxel z.
        let data = scene_data(
            vec![model((3, 1, 1), &[(0, 0, 0, 0), (2, 0, 0, 1)])],
            vec![trn(1, &[("_r", "17")], &[]), shp(0)],
        );
        let mut g = VoxelGrid::default();
        place_models(&data, &mut g).unwrap();
        assert_eq!(g.get(IVec3::new(0, 0, 0)), Some([10, 10, 10, 255]));
        assert_eq!(g.get(IVec3::new(0, 0, 2)), Some([200, 200, 200, 255]));
    }

    #[test]
    fn import_ignores_a_malformed_rotation() {
        let data = scene_data(
            vec![model((2, 1, 1), &[(1, 0, 0, 0)])],
            vec![trn(1, &[("_r", "15"), ("_t", "nope")], &[]), shp(0)],
        );
        let mut g = VoxelGrid::default();
        place_models(&data, &mut g).unwrap();
        assert_eq!(g.get(IVec3::new(1, 0, 0)), Some([10, 10, 10, 255]));
    }

    #[test]
    fn import_survives_a_cyclic_scene_graph() {
        let data = scene_data(
            vec![model((1, 1, 1), &[(0, 0, 0, 0)])],
            vec![trn(1, &[], &[]), trn(0, &[], &[])],
        );
        let mut g = VoxelGrid::default();
        place_models(&data, &mut g).unwrap();
        assert_eq!(g.count(), 0);
    }

    #[test]
    fn import_survives_hostile_transforms() {
        let max = i32::MAX.to_string();
        let t = format!("{max} {max} {max}");
        let nest = |depth: u32| {
            let mut scenes = Vec::new();
            for d in 0..depth {
                scenes.push(trn(d + 1, &[("_t", &t)], &[]));
            }
            scenes.push(shp(0));
            scene_data(vec![model((1, 1, 1), &[(0, 0, 0, 0)])], scenes)
        };
        let mut g = VoxelGrid::default();
        // Two models would pin `min`; one nested chain just must not panic or wrap.
        place_models(&nest(40), &mut g).expect("single far model imports");
        assert_eq!(g.count(), 1);

        let two = {
            let mut d = nest(1);
            d.models.push(model((1, 1, 1), &[(0, 0, 0, 1)]));
            d.scenes = vec![
                trn(1, &[], &[]),
                SceneNode::Group {
                    attributes: dict(&[]),
                    children: vec![2, 4],
                },
                trn(3, &[("_t", &format!("-{max} 0 0"))], &[]),
                shp(0),
                trn(5, &[("_t", &format!("{max} 0 0"))], &[]),
                shp(1),
            ];
            d
        };
        let err = place_models(&two, &mut VoxelGrid::default())
            .unwrap_err()
            .to_string();
        assert!(err.contains("coordinate range"), "got: {err}");
    }

    #[test]
    fn rotation_byte_decodes_to_a_signed_permutation() {
        assert_eq!(parse_rotation("4"), Some(Xform::IDENTITY.r));
        assert_eq!(
            parse_rotation("17"),
            Some([[0, -1, 0], [1, 0, 0], [0, 0, 1]])
        );
        assert_eq!(parse_rotation("3"), None);
        assert_eq!(parse_rotation("0"), None);
        assert_eq!(parse_translation("1 -2 3"), Some([1, -2, 3]));
        assert_eq!(parse_translation("1 2"), None);
    }

    #[test]
    fn nested_transforms_compose() {
        let rot = Xform {
            r: parse_rotation("17").unwrap(),
            t: [5, 0, 0],
        };
        let shift = Xform {
            r: Xform::IDENTITY.r,
            t: [1, 0, 0],
        };
        // Parent rotates the child's +x offset onto +y, then adds its own t.
        assert_eq!(rot.then(shift).t, [5, 1, 0]);
    }

    #[test]
    fn import_returns_used_colors_as_palette() {
        // Palette index order, deduped, only colors actually placed. Index 2 is
        // referenced twice (one dedup) and index 1 is unused → excluded.
        let path = tmp_path("palette");
        let palette = make_palette(&[(10, 10, 10), (20, 20, 20), (30, 30, 30)]);
        let data = DotVoxData {
            version: 150,
            index_map: (0u8..=255u8).collect(),
            models: vec![Model {
                size: Size {
                    x: 32,
                    y: 32,
                    z: 32,
                },
                voxels: vec![
                    Voxel {
                        x: 1,
                        y: 0,
                        z: 0,
                        i: 2,
                    },
                    Voxel {
                        x: 0,
                        y: 0,
                        z: 0,
                        i: 0,
                    },
                    Voxel {
                        x: 2,
                        y: 0,
                        z: 0,
                        i: 2,
                    },
                ],
            }],
            palette,
            materials: vec![],
            scenes: vec![],
            layers: vec![],
        };
        let mut f = std::fs::File::create(&path).unwrap();
        data.write_vox(&mut f).unwrap();
        let mut g = VoxelGrid::default();
        let colors = import(&path, &mut g).expect("import");
        // Ascending index order (0 then 2), deduped, index 1 absent.
        assert_eq!(colors, vec![[10, 10, 10, 255], [30, 30, 30, 255]]);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn export_reports_colors_past_255_and_keeps_the_most_used_exact() {
        let mut g = VoxelGrid::default();
        for i in 0..300u32 {
            let c = [(i % 256) as u8, (i / 256) as u8, 7, 255];
            g.set(IVec3::new((i % 16) as i32, 0, (i / 16) as i32), Some(c));
        }
        let popular = [250, 1, 7, 255];
        g.set(IVec3::new(0, 1, 0), Some(popular));
        g.set(IVec3::new(1, 1, 0), Some(popular));
        let (palette, map, merged) = build_palette(&g);
        assert_eq!(merged, 301 - VOX_MAX_COLORS);
        assert_eq!(palette.len(), VOX_MAX_COLORS);
        let p = palette[map[&popular] as usize];
        assert_eq!([p.r, p.g, p.b], [250, 1, 7]);

        let path = tmp_path("many-colors");
        assert_eq!(export(&path, &g).unwrap(), merged);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn export_under_the_cap_merges_nothing() {
        let mut g = VoxelGrid::default();
        g.set(IVec3::ZERO, Some([1, 2, 3, 255]));
        let path = tmp_path("few-colors");
        assert_eq!(export(&path, &g).unwrap(), 0);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn nearest_picks_closest_rgb() {
        let palette = vec![
            Color {
                r: 0,
                g: 0,
                b: 0,
                a: 255,
            },
            Color {
                r: 255,
                g: 0,
                b: 0,
                a: 255,
            },
            Color {
                r: 0,
                g: 255,
                b: 0,
                a: 255,
            },
        ];
        assert_eq!(nearest(&palette, [250, 10, 10, 255]), 1);
        assert_eq!(nearest(&palette, [5, 5, 5, 255]), 0);
    }
}
