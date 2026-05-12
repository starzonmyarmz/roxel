use crate::grid::{GRID, VoxelGrid};
use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::mesh::{Indices, PrimitiveTopology};

#[derive(Component)]
pub struct VoxelMesh;

pub const FACES: [Face; 6] = [
    Face { normal: [1.0, 0.0, 0.0],  d: IVec3::new(1, 0, 0),   corners: [[1,0,0],[1,0,1],[1,1,1],[1,1,0]], axis: 0, u_axis: 1, v_axis: 2, plane_offset: 1 },
    Face { normal: [-1.0, 0.0, 0.0], d: IVec3::new(-1, 0, 0),  corners: [[0,0,0],[0,1,0],[0,1,1],[0,0,1]], axis: 0, u_axis: 1, v_axis: 2, plane_offset: 0 },
    Face { normal: [0.0, 1.0, 0.0],  d: IVec3::new(0, 1, 0),   corners: [[0,1,0],[1,1,0],[1,1,1],[0,1,1]], axis: 1, u_axis: 0, v_axis: 2, plane_offset: 1 },
    Face { normal: [0.0, -1.0, 0.0], d: IVec3::new(0, -1, 0),  corners: [[0,0,0],[0,0,1],[1,0,1],[1,0,0]], axis: 1, u_axis: 0, v_axis: 2, plane_offset: 0 },
    Face { normal: [0.0, 0.0, 1.0],  d: IVec3::new(0, 0, 1),   corners: [[0,0,1],[0,1,1],[1,1,1],[1,0,1]], axis: 2, u_axis: 0, v_axis: 1, plane_offset: 1 },
    Face { normal: [0.0, 0.0, -1.0], d: IVec3::new(0, 0, -1),  corners: [[0,0,0],[1,0,0],[1,1,0],[0,1,0]], axis: 2, u_axis: 0, v_axis: 1, plane_offset: 0 },
];

pub struct Face {
    pub normal: [f32; 3],
    pub d: IVec3,
    /// Unit-cube corners of this face, in winding-correct order. Indices into
    /// each `[i32; 3]` are 0/1; substitute the greedy quad's `(i, i+h)` along
    /// `u_axis` and `(j, j+w)` along `v_axis` to lift to world space.
    pub corners: [[i32; 3]; 4],
    /// World-space axis that is constant on this face (0=X, 1=Y, 2=Z).
    pub axis: usize,
    /// The two axes that vary across the face.
    pub u_axis: usize,
    pub v_axis: usize,
    /// Offset added to the slice index `k` to get the world-space plane on
    /// `axis` for this face. `0` for the negative face, `1` for the positive.
    pub plane_offset: i32,
}

pub fn face_shade(normal: [f32; 3]) -> f32 {
    if normal[1] > 0.5 { 1.0 }
    else if normal[1] < -0.5 { 0.45 }
    else if normal[0].abs() > 0.5 { 0.78 }
    else { 0.62 }
}

pub fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
}

pub fn linear_to_srgb(c: f32) -> f32 {
    if c <= 0.003_130_8 { c * 12.92 } else { 1.055 * c.powf(1.0 / 2.4) - 0.055 }
}

pub struct GreedyQuad {
    pub face_idx: usize,
    /// World-space corners, winding-correct for the face direction.
    pub corners: [Vec3; 4],
    pub color: [u8; 4],
}

/// Walk each of the six face directions, build a per-slice (color, visibility)
/// mask of exterior faces, and greedy-merge adjacent same-color cells into the
/// largest axis-aligned rectangles. Shared between the renderer and the SVG
/// exporter so they emit identical surface geometry.
pub fn greedy_quads(grid: &VoxelGrid, hide: Option<IVec3>) -> Vec<GreedyQuad> {
    let cell_filled = |p: IVec3| -> Option<[u8; 4]> {
        if hide == Some(p) {
            return None;
        }
        if !VoxelGrid::in_bounds(p) {
            return None;
        }
        grid.cells[p.x as usize][p.y as usize][p.z as usize]
    };

    let mut quads = Vec::new();
    // Stack-allocated mask; ~32 KB at GRID=64.
    let mut mask: [[Option<[u8; 4]>; GRID]; GRID] = [[None; GRID]; GRID];

    for (face_idx, face) in FACES.iter().enumerate() {
        let axis = face.axis;
        let u = face.u_axis;
        let v = face.v_axis;

        for k in 0..GRID {
            for row in mask.iter_mut() {
                for slot in row.iter_mut() {
                    *slot = None;
                }
            }
            for i in 0..GRID {
                for j in 0..GRID {
                    let mut c = [0i32; 3];
                    c[axis] = k as i32;
                    c[u] = i as i32;
                    c[v] = j as i32;
                    let p = IVec3::new(c[0], c[1], c[2]);
                    let Some(rgba) = cell_filled(p) else { continue };
                    if cell_filled(p + face.d).is_some() {
                        continue;
                    }
                    mask[i][j] = Some(rgba);
                }
            }

            for i in 0..GRID {
                let mut j = 0;
                while j < GRID {
                    let Some(key) = mask[i][j] else {
                        j += 1;
                        continue;
                    };
                    // Grow along v (inner) first, then along u (outer rows).
                    let mut w = 1;
                    while j + w < GRID && mask[i][j + w] == Some(key) {
                        w += 1;
                    }
                    let mut h = 1;
                    'grow: while i + h < GRID {
                        for dj in 0..w {
                            if mask[i + h][j + dj] != Some(key) {
                                break 'grow;
                            }
                        }
                        h += 1;
                    }
                    for di in 0..h {
                        for dj in 0..w {
                            mask[i + di][j + dj] = None;
                        }
                    }

                    // Lift unit-cube corners into the merged quad's world rect.
                    let mut corners = [Vec3::ZERO; 4];
                    for (idx, c) in face.corners.iter().enumerate() {
                        let mut p = [0f32; 3];
                        p[axis] = (k as i32 + face.plane_offset) as f32;
                        p[u] = if c[u] == 0 { i as f32 } else { (i + h) as f32 };
                        p[v] = if c[v] == 0 { j as f32 } else { (j + w) as f32 };
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

pub fn build_mesh(grid: &VoxelGrid, hide: Option<IVec3>) -> Mesh {
    let mut positions: Vec<[f32; 3]> = Vec::new();
    let mut normals:   Vec<[f32; 3]> = Vec::new();
    let mut colors:    Vec<[f32; 4]> = Vec::new();
    let mut indices:   Vec<u32>      = Vec::new();

    for q in greedy_quads(grid, hide) {
        let face = &FACES[q.face_idx];
        let rgba = q.color;
        let base_rgb = [
            srgb_to_linear(rgba[0] as f32 / 255.0),
            srgb_to_linear(rgba[1] as f32 / 255.0),
            srgb_to_linear(rgba[2] as f32 / 255.0),
        ];
        let alpha = rgba[3] as f32 / 255.0;
        let shade = face_shade(face.normal);
        let col = [base_rgb[0] * shade, base_rgb[1] * shade, base_rgb[2] * shade, alpha];

        let base = positions.len() as u32;
        for corner in &q.corners {
            positions.push([corner.x, corner.y, corner.z]);
            normals.push(face.normal);
            colors.push(col);
        }
        indices.extend_from_slice(&[base, base + 2, base + 1, base, base + 3, base + 2]);
    }

    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}

#[derive(Resource)]
pub struct VoxelMeshHandle(pub Handle<Mesh>);

#[derive(Resource, Default)]
pub struct PreviewHide {
    pub cell: Option<IVec3>,
    last: Option<IVec3>,
}

impl PreviewHide {
    pub fn set(&mut self, c: Option<IVec3>) {
        self.cell = c;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn srgb_linear_roundtrip_endpoints_and_midpoint() {
        for x in [0.0_f32, 0.04, 0.04045, 0.2, 0.5, 0.9, 1.0] {
            let r = linear_to_srgb(srgb_to_linear(x));
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
    fn greedy_quads_single_voxel_emits_six_faces() {
        let mut g = VoxelGrid::default();
        g.set(IVec3::new(5, 5, 5), Some([1, 1, 1, 255]));
        let quads = greedy_quads(&g, None);
        assert_eq!(quads.len(), 6);
    }

    #[test]
    fn greedy_quads_two_adjacent_voxels_share_face() {
        let mut g = VoxelGrid::default();
        g.set(IVec3::new(0, 0, 0), Some([1, 1, 1, 255]));
        g.set(IVec3::new(1, 0, 0), Some([1, 1, 1, 255]));
        // Adjacency hides the shared faces; the four lateral faces of each cell
        // merge along X into single quads — same result as a 2x1x1 strip: 6 quads.
        let quads = greedy_quads(&g, None);
        assert_eq!(quads.len(), 6);
    }

    #[test]
    fn greedy_quads_merges_same_color_row_into_one_quad_per_face() {
        let mut g = VoxelGrid::default();
        for x in 0..4 {
            g.set(IVec3::new(x, 0, 0), Some([1, 1, 1, 255]));
        }
        // 2 end caps (+X, -X) + top/bottom/+Z/-Z each merged to one quad = 6.
        let quads = greedy_quads(&g, None);
        assert_eq!(quads.len(), 6);
    }

    #[test]
    fn greedy_quads_different_colors_do_not_merge() {
        let mut g = VoxelGrid::default();
        g.set(IVec3::new(0, 0, 0), Some([1, 0, 0, 255]));
        g.set(IVec3::new(1, 0, 0), Some([2, 0, 0, 255]));
        // Same as adjacent, but top/bottom/±Z faces can't merge across the color
        // boundary — 4 faces * 2 cells + 2 end caps = 10.
        let quads = greedy_quads(&g, None);
        assert_eq!(quads.len(), 10);
    }

    #[test]
    fn greedy_quads_hide_skips_targeted_cell() {
        let mut g = VoxelGrid::default();
        g.set(IVec3::new(5, 5, 5), Some([1, 1, 1, 255]));
        let quads = greedy_quads(&g, Some(IVec3::new(5, 5, 5)));
        assert_eq!(quads.len(), 0);
    }

    #[test]
    fn build_mesh_empty_grid_has_no_geometry() {
        let g = VoxelGrid::default();
        let m = build_mesh(&g, None);
        let pos = m.attribute(Mesh::ATTRIBUTE_POSITION).expect("positions");
        assert_eq!(pos.len(), 0);
    }

    #[test]
    fn build_mesh_single_voxel_has_24_vertices_and_36_indices() {
        let mut g = VoxelGrid::default();
        g.set(IVec3::new(0, 0, 0), Some([1, 1, 1, 255]));
        let m = build_mesh(&g, None);
        let pos = m.attribute(Mesh::ATTRIBUTE_POSITION).expect("positions");
        assert_eq!(pos.len(), 24);
        let idx = m.indices().expect("indices");
        assert_eq!(idx.len(), 36);
    }
}

pub fn regenerate_mesh_system(
    mut grid: ResMut<VoxelGrid>,
    mut hide: ResMut<PreviewHide>,
    handle: Res<VoxelMeshHandle>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    let hide_changed = hide.cell != hide.last;
    if !grid.dirty && !hide_changed {
        return;
    }
    let new_mesh = build_mesh(&grid, hide.cell);
    if let Some(m) = meshes.get_mut(&handle.0) {
        *m = new_mesh;
    }
    grid.dirty = false;
    hide.last = hide.cell;
}
