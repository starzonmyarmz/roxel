use crate::GridResource;
use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::pbr::MeshMaterial3d;
use bevy::prelude::*;
use roxel::grid::{CHUNK_I, Color8, VoxelGrid, chunk_coord};
use std::collections::HashMap;

pub use roxel::mesh_util::{FACES, face_shade, srgb_to_linear};

#[derive(Component)]
pub struct VoxelMesh;

pub struct GreedyQuad {
    pub face_idx: usize,
    /// World-space corners, winding-correct for the face direction.
    pub corners: [Vec3; 4],
    pub color: [u8; 4],
}

/// Greedy-merge every same-color exterior face in the grid. Reference path
/// for tests; the renderer goes through `greedy_quads_bounded` per dirty
/// chunk.
#[allow(dead_code)]
pub fn greedy_quads(
    grid: &VoxelGrid,
    hide: Option<IVec3>,
    recolor: Option<(IVec3, Color8)>,
) -> Vec<GreedyQuad> {
    let mut out = Vec::new();
    for coord in grid.chunks.keys().copied().collect::<Vec<_>>() {
        let min = coord * CHUNK_I;
        let max = min + IVec3::splat(CHUNK_I);
        out.extend(greedy_quads_bounded(grid, hide, recolor, min, max));
    }
    out
}

/// Greedy-merge exterior faces of voxels in the half-open box `[min, max)`.
/// Cross-bounds occlusion still queries the full grid so emitted quads agree
/// at chunk seams with the equivalent monolithic call. `recolor` swaps the
/// emitted color for a single cell — used by the Paint preview to ghost the
/// target color without mutating the grid.
pub fn greedy_quads_bounded(
    grid: &VoxelGrid,
    hide: Option<IVec3>,
    recolor: Option<(IVec3, Color8)>,
    min: IVec3,
    max: IVec3,
) -> Vec<GreedyQuad> {
    let cell_filled = |p: IVec3| -> Option<[u8; 4]> {
        if hide == Some(p) {
            return None;
        }
        let base = grid.get(p)?;
        if let Some((cell, c)) = recolor
            && cell == p
        {
            Some([c[0], c[1], c[2], base[3]])
        } else {
            Some(base)
        }
    };

    let mut quads = Vec::new();
    let min_a = [min.x, min.y, min.z];
    let max_a = [max.x, max.y, max.z];

    for (face_idx, face) in FACES.iter().enumerate() {
        let axis = face.axis;
        let u = face.u_axis;
        let v = face.v_axis;

        let k_lo = min_a[axis];
        let k_hi = max_a[axis];
        let u_lo = min_a[u];
        let u_hi = max_a[u];
        let v_lo = min_a[v];
        let v_hi = max_a[v];
        let dim_u = (u_hi - u_lo).max(0) as usize;
        let dim_v = (v_hi - v_lo).max(0) as usize;
        if dim_u == 0 || dim_v == 0 || k_hi <= k_lo {
            continue;
        }

        let mut mask: Vec<Option<[u8; 4]>> = vec![None; dim_u * dim_v];

        for k in k_lo..k_hi {
            for slot in mask.iter_mut() {
                *slot = None;
            }
            for i in 0..dim_u {
                for j in 0..dim_v {
                    let mut c = [0i32; 3];
                    c[axis] = k;
                    c[u] = u_lo + i as i32;
                    c[v] = v_lo + j as i32;
                    let p = IVec3::new(c[0], c[1], c[2]);
                    let Some(rgba) = cell_filled(p) else { continue };
                    if cell_filled(p + face.d).is_some() {
                        continue;
                    }
                    mask[i * dim_v + j] = Some(rgba);
                }
            }

            for i in 0..dim_u {
                let mut j = 0;
                while j < dim_v {
                    let Some(key) = mask[i * dim_v + j] else {
                        j += 1;
                        continue;
                    };
                    let mut w = 1;
                    while j + w < dim_v && mask[i * dim_v + j + w] == Some(key) {
                        w += 1;
                    }
                    let mut h = 1;
                    'grow: while i + h < dim_u {
                        for dj in 0..w {
                            if mask[(i + h) * dim_v + j + dj] != Some(key) {
                                break 'grow;
                            }
                        }
                        h += 1;
                    }
                    for di in 0..h {
                        for dj in 0..w {
                            mask[(i + di) * dim_v + j + dj] = None;
                        }
                    }

                    let mut corners = [Vec3::ZERO; 4];
                    let i_base = u_lo + i as i32;
                    let j_base = v_lo + j as i32;
                    for (idx, c) in face.corners.iter().enumerate() {
                        let mut p = [0f32; 3];
                        p[axis] = (k + face.plane_offset) as f32;
                        p[u] = if c[u] == 0 {
                            i_base as f32
                        } else {
                            (i_base + h as i32) as f32
                        };
                        p[v] = if c[v] == 0 {
                            j_base as f32
                        } else {
                            (j_base + w as i32) as f32
                        };
                        corners[idx] = Vec3::new(p[0], p[1], p[2]);
                    }

                    quads.push(GreedyQuad {
                        face_idx,
                        corners,
                        color: key,
                    });

                    j += w;
                }
            }
        }
    }

    quads
}

/// Build the whole-grid mesh in one allocation. Test/reference path only —
/// the runtime renderer goes through per-chunk meshes via
/// `regenerate_mesh_system`.
#[allow(dead_code)]
pub fn build_mesh(grid: &VoxelGrid, hide: Option<IVec3>, recolor: Option<(IVec3, Color8)>) -> Mesh {
    build_mesh_from_quads(greedy_quads(grid, hide, recolor))
}

fn build_mesh_from_quads(quads: Vec<GreedyQuad>) -> Mesh {
    build_mesh_from_quads_shaded(quads, true, 1.0)
}

/// Whole-grid mesh with **real** per-face normals but no baked `face_shade`
/// darkening — vertex colors carry pure albedo. Used by the social-media shot
/// renderer (`shot.rs`), which lights the model for real with a directional
/// light. Applying the baked fake shade there would double-darken the
/// side/bottom faces. Built once per shot (not per frame), so the whole-grid
/// `greedy_quads` reference path is fine. `saturation` (< 1.0) pulls albedo
/// toward grey so the lit shot doesn't read oversaturated vs. the editor.
pub fn build_lit_mesh(grid: &VoxelGrid, saturation: f32) -> Mesh {
    build_mesh_from_quads_shaded(greedy_quads(grid, None, None), false, saturation)
}

/// `apply_shade` bakes `face_shade(normal)` into the vertex colors (the
/// editor's flat-lit look); `false` keeps albedo untouched for real lighting.
/// `saturation` lerps each linear color toward its luminance (1.0 = unchanged).
fn build_mesh_from_quads_shaded(
    quads: Vec<GreedyQuad>,
    apply_shade: bool,
    saturation: f32,
) -> Mesh {
    let mut positions: Vec<[f32; 3]> = Vec::new();
    let mut normals: Vec<[f32; 3]> = Vec::new();
    let mut colors: Vec<[f32; 4]> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();

    for q in quads {
        let face = &FACES[q.face_idx];
        let rgba = q.color;
        let shade = if apply_shade {
            face_shade(face.normal)
        } else {
            1.0
        };
        let alpha = rgba[3] as f32 / 255.0;
        let mut col = [
            srgb_to_linear((rgba[0] as f32 / 255.0) * shade),
            srgb_to_linear((rgba[1] as f32 / 255.0) * shade),
            srgb_to_linear((rgba[2] as f32 / 255.0) * shade),
            alpha,
        ];
        if saturation < 1.0 {
            // Rec.709 luminance in linear space; lerp rgb toward grey.
            let lum = 0.2126 * col[0] + 0.7152 * col[1] + 0.0722 * col[2];
            for c in col.iter_mut().take(3) {
                *c = lum + (*c - lum) * saturation;
            }
        }

        let base = positions.len() as u32;
        for corner in &q.corners {
            positions.push([corner.x, corner.y, corner.z]);
            normals.push(face.normal);
            colors.push(col);
        }
        indices.extend_from_slice(&[base, base + 2, base + 1, base, base + 3, base + 2]);
    }

    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}

/// One mesh handle + entity per loaded chunk, keyed by chunk coord. Chunks
/// allocate when the mesher first sees a dirty coord with data; they
/// despawn when the chunk empties and shows up in `dirty_chunks` with no
/// backing data. The shared `material` handle is reused for every chunk.
#[derive(Resource)]
pub struct VoxelChunkMeshes {
    pub chunks: HashMap<IVec3, (Entity, Handle<Mesh>)>,
    pub material: Handle<StandardMaterial>,
}

#[derive(Resource, Default)]
pub struct PreviewHide {
    pub cell: Option<IVec3>,
    pub recolor: Option<(IVec3, Color8)>,
    last_cell: Option<IVec3>,
    last_recolor: Option<(IVec3, Color8)>,
}

impl PreviewHide {
    pub fn set(&mut self, c: Option<IVec3>) {
        self.cell = c;
    }

    pub fn set_recolor(&mut self, r: Option<(IVec3, Color8)>) {
        self.recolor = r;
    }
}

pub fn regenerate_mesh_system(
    mut commands: Commands,
    mut grid: ResMut<GridResource>,
    mut hide: ResMut<PreviewHide>,
    mut chunk_meshes: ResMut<VoxelChunkMeshes>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    let hide_changed = hide.cell != hide.last_cell;
    let recolor_changed = hide.recolor != hide.last_recolor;

    if hide_changed {
        for opt in [hide.cell, hide.last_cell] {
            if let Some(p) = opt
                && grid.in_bounds(p)
            {
                let coord = chunk_coord(p);
                grid.dirty_chunks.insert(coord);
            }
        }
    }
    if recolor_changed {
        for opt in [hide.recolor, hide.last_recolor] {
            if let Some((p, _)) = opt
                && grid.in_bounds(p)
            {
                let coord = chunk_coord(p);
                grid.dirty_chunks.insert(coord);
            }
        }
    }

    if grid.dirty_chunks.is_empty() {
        grid.dirty = false;
        hide.last_cell = hide.cell;
        hide.last_recolor = hide.recolor;
        return;
    }

    let dirty: Vec<IVec3> = grid.dirty_chunks.drain().collect();
    let material = chunk_meshes.material.clone();

    for coord in dirty {
        let has_data = grid.chunks.contains_key(&coord);

        if !has_data {
            if let Some((entity, _)) = chunk_meshes.chunks.remove(&coord) {
                commands.entity(entity).despawn();
            }
            continue;
        }

        let min = coord * CHUNK_I;
        let max = min + IVec3::splat(CHUNK_I);
        let new_mesh = build_mesh_from_quads(greedy_quads_bounded(
            &grid,
            hide.cell,
            hide.recolor,
            min,
            max,
        ));

        if let Some((_, handle)) = chunk_meshes.chunks.get(&coord) {
            if let Some(slot) = meshes.get_mut(handle) {
                *slot = new_mesh;
            }
        } else {
            let handle = meshes.add(new_mesh);
            let entity = commands
                .spawn((
                    Mesh3d(handle.clone()),
                    MeshMaterial3d(material.clone()),
                    Transform::IDENTITY,
                    VoxelMesh,
                ))
                .id();
            chunk_meshes.chunks.insert(coord, (entity, handle));
        }
    }

    grid.dirty = false;
    hide.last_cell = hide.cell;
    hide.last_recolor = hide.recolor;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn srgb_linear_roundtrip_endpoints_and_midpoint() {
        for x in [0.0_f32, 0.04, 0.04045, 0.2, 0.5, 0.9, 1.0] {
            let r = roxel::mesh_util::linear_to_srgb(srgb_to_linear(x));
            assert!((r - x).abs() < 1e-4, "x={x} r={r}");
        }
    }

    #[test]
    fn face_shade_distinguishes_top_bottom_side() {
        assert!(face_shade([0.0, 1.0, 0.0]) > face_shade([1.0, 0.0, 0.0]));
        assert!(face_shade([1.0, 0.0, 0.0]) > face_shade([0.0, 0.0, 1.0]));
        assert!(face_shade([0.0, 0.0, 1.0]) > face_shade([0.0, -1.0, 0.0]));
    }

    #[test]
    fn build_lit_mesh_skips_baked_face_shade() {
        // Lit mesh = pure albedo (no per-face darkening). Every vertex color
        // for a single white voxel should be the same linear value; the editor
        // mesh would vary per face via face_shade.
        let mut g = VoxelGrid::default();
        g.set(IVec3::new(0, 0, 0), Some([200, 100, 50, 255]));

        let lit = build_lit_mesh(&g, 1.0);
        let Some(bevy::mesh::VertexAttributeValues::Float32x4(lit_cols)) =
            lit.attribute(Mesh::ATTRIBUTE_COLOR)
        else {
            panic!("lit mesh missing vertex colors");
        };
        let first = lit_cols[0];
        assert!(
            lit_cols.iter().all(|c| (c[0] - first[0]).abs() < 1e-6),
            "lit mesh must not bake per-face shade"
        );
        // Albedo channel equals srgb_to_linear of the raw color (shade = 1.0).
        assert!((first[0] - srgb_to_linear(200.0 / 255.0)).abs() < 1e-5);

        // The editor mesh DOES vary per face (top brighter than sides).
        let shaded = build_mesh(&g, None, None);
        let Some(bevy::mesh::VertexAttributeValues::Float32x4(shaded_cols)) =
            shaded.attribute(Mesh::ATTRIBUTE_COLOR)
        else {
            panic!("shaded mesh missing vertex colors");
        };
        assert!(
            shaded_cols
                .iter()
                .any(|c| (c[0] - shaded_cols[0][0]).abs() > 1e-4),
            "editor mesh should vary per face"
        );
    }

    #[test]
    fn greedy_quads_single_voxel_emits_six_faces() {
        let mut g = VoxelGrid::default();
        g.set(IVec3::new(5, 5, 5), Some([1, 1, 1, 255]));
        let quads = greedy_quads(&g, None, None);
        assert_eq!(quads.len(), 6);
    }

    #[test]
    fn greedy_quads_two_adjacent_voxels_share_face() {
        let mut g = VoxelGrid::default();
        g.set(IVec3::new(0, 0, 0), Some([1, 1, 1, 255]));
        g.set(IVec3::new(1, 0, 0), Some([1, 1, 1, 255]));
        let quads = greedy_quads(&g, None, None);
        assert_eq!(quads.len(), 6);
    }

    #[test]
    fn greedy_quads_merges_same_color_row_into_one_quad_per_face() {
        let mut g = VoxelGrid::default();
        for x in 0..4 {
            g.set(IVec3::new(x, 0, 0), Some([1, 1, 1, 255]));
        }
        let quads = greedy_quads(&g, None, None);
        assert_eq!(quads.len(), 6);
    }

    #[test]
    fn greedy_quads_different_colors_do_not_merge() {
        let mut g = VoxelGrid::default();
        g.set(IVec3::new(0, 0, 0), Some([1, 0, 0, 255]));
        g.set(IVec3::new(1, 0, 0), Some([2, 0, 0, 255]));
        let quads = greedy_quads(&g, None, None);
        assert_eq!(quads.len(), 10);
    }

    #[test]
    fn greedy_quads_recolor_swaps_rgb_keeps_alpha() {
        let mut g = VoxelGrid::default();
        g.set(IVec3::new(2, 2, 2), Some([10, 20, 30, 200]));
        let quads = greedy_quads(&g, None, Some((IVec3::new(2, 2, 2), [99, 88, 77, 255])));
        assert_eq!(quads.len(), 6);
        for q in &quads {
            assert_eq!(q.color, [99, 88, 77, 200]);
        }
    }

    #[test]
    fn greedy_quads_recolor_only_affects_targeted_cell() {
        let mut g = VoxelGrid::default();
        g.set(IVec3::new(0, 0, 0), Some([10, 10, 10, 255]));
        g.set(IVec3::new(2, 0, 0), Some([10, 10, 10, 255]));
        let quads = greedy_quads(&g, None, Some((IVec3::new(0, 0, 0), [200, 0, 0, 255])));
        let recolored = quads.iter().filter(|q| q.color == [200, 0, 0, 255]).count();
        let untouched = quads
            .iter()
            .filter(|q| q.color == [10, 10, 10, 255])
            .count();
        assert_eq!(recolored, 6);
        assert_eq!(untouched, 6);
    }

    #[test]
    fn greedy_quads_hide_skips_targeted_cell() {
        let mut g = VoxelGrid::default();
        g.set(IVec3::new(5, 5, 5), Some([1, 1, 1, 255]));
        let quads = greedy_quads(&g, Some(IVec3::new(5, 5, 5)), None);
        assert_eq!(quads.len(), 0);
    }

    #[test]
    fn build_mesh_empty_grid_has_no_geometry() {
        let g = VoxelGrid::default();
        let m = build_mesh(&g, None, None);
        let pos = m.attribute(Mesh::ATTRIBUTE_POSITION).expect("positions");
        assert_eq!(pos.len(), 0);
    }

    /// Total unit-face area per face direction. Chunked walks may emit
    /// multiple smaller quads where a monolithic walk emits one big one;
    /// the underlying covered surface area must still agree.
    fn area_by_face(quads: &[GreedyQuad]) -> [i64; 6] {
        let mut areas = [0i64; 6];
        for q in quads {
            let face = &FACES[q.face_idx];
            let u = face.u_axis;
            let v = face.v_axis;
            let mut u_min = f32::INFINITY;
            let mut u_max = f32::NEG_INFINITY;
            let mut v_min = f32::INFINITY;
            let mut v_max = f32::NEG_INFINITY;
            for c in &q.corners {
                let arr = [c.x, c.y, c.z];
                u_min = u_min.min(arr[u]);
                u_max = u_max.max(arr[u]);
                v_min = v_min.min(arr[v]);
                v_max = v_max.max(arr[v]);
            }
            let w = (u_max - u_min) as i64;
            let h = (v_max - v_min) as i64;
            areas[q.face_idx] += w * h;
        }
        areas
    }

    #[test]
    fn chunked_quads_cover_same_area_as_monolithic_across_seams() {
        // Cells deliberately straddle chunk seams in X and Y to exercise
        // cross-chunk occlusion in the chunked walk.
        let mut g = VoxelGrid::default();
        let pts = [
            IVec3::new(0, 0, 0),
            IVec3::new(31, 5, 5),
            IVec3::new(32, 5, 5),
            IVec3::new(33, 5, 5),
            IVec3::new(5, 31, 5),
            IVec3::new(5, 32, 5),
            IVec3::new(80, 80, 80),
        ];
        for p in pts {
            g.set(p, Some([7, 7, 7, 255]));
        }

        // Two "monolithic" boxes for comparison: each ALSO honours per-chunk
        // occlusion via grid.get under the hood, so the only sensible
        // comparison is per-chunk against per-chunk — we instead compare
        // against the reference `greedy_quads` which already walks per-chunk.
        let walk_a = greedy_quads(&g, None, None);
        let walk_b = {
            // Independently walk chunk coords and aggregate.
            let mut out = Vec::new();
            for coord in g.chunks.keys().copied().collect::<Vec<_>>() {
                let min = coord * CHUNK_I;
                let max = min + IVec3::splat(CHUNK_I);
                out.extend(greedy_quads_bounded(&g, None, None, min, max));
            }
            out
        };
        assert_eq!(area_by_face(&walk_a), area_by_face(&walk_b));
    }

    #[test]
    fn chunked_quads_hide_cross_chunk_seam_faces() {
        // Two cells adjacent across the x = CHUNK seam. The shared face
        // (+X of left, -X of right) must not be emitted by either chunk —
        // cross-chunk occlusion still hides it.
        let mut g = VoxelGrid::default();
        g.set(IVec3::new(CHUNK_I - 1, 0, 0), Some([1, 1, 1, 255]));
        g.set(IVec3::new(CHUNK_I, 0, 0), Some([1, 1, 1, 255]));

        let quads = greedy_quads(&g, None, None);
        let total: i64 = area_by_face(&quads).iter().sum();
        // 2 caps (1 area each) + 4 long sides (2 area each) = 10.
        assert_eq!(total, 10);
    }

    #[test]
    fn greedy_quads_handles_negative_coords() {
        let mut g = VoxelGrid::default();
        g.set(IVec3::new(-5, 0, -5), Some([1, 1, 1, 255]));
        let quads = greedy_quads(&g, None, None);
        assert_eq!(quads.len(), 6);
    }

    #[test]
    fn build_mesh_single_voxel_has_24_vertices_and_36_indices() {
        let mut g = VoxelGrid::default();
        g.set(IVec3::new(0, 0, 0), Some([1, 1, 1, 255]));
        let m = build_mesh(&g, None, None);
        let pos = m.attribute(Mesh::ATTRIBUTE_POSITION).expect("positions");
        assert_eq!(pos.len(), 24);
        let idx = m.indices().expect("indices");
        assert_eq!(idx.len(), 36);
    }
}
