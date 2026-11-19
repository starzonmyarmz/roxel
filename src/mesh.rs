use crate::GridResource;
use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::pbr::MeshMaterial3d;
use bevy::prelude::*;
use roxel::grid::{Color8, VoxelGrid, chunk_coord};
use std::collections::HashMap;

pub use roxel::mesh_util::{FACES, face_shade, srgb_to_linear};
use roxel::mesh_util::{GreedyQuad, greedy_quads, greedy_quads_chunk};

#[derive(Component)]
pub struct VoxelMesh;

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
                grid.mark_cell_dirty(p);
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

        let new_mesh =
            build_mesh_from_quads(greedy_quads_chunk(&grid, coord, hide.cell, hide.recolor));

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
    fn build_mesh_empty_grid_has_no_geometry() {
        let g = VoxelGrid::default();
        let m = build_mesh(&g, None, None);
        let pos = m.attribute(Mesh::ATTRIBUTE_POSITION).expect("positions");
        assert_eq!(pos.len(), 0);
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
