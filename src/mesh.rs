use crate::grid::{GRID, VoxelGrid};
use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::mesh::{Indices, PrimitiveTopology};

#[derive(Component)]
pub struct VoxelMesh;

pub const FACES: [Face; 6] = [
    Face { normal: [1.0, 0.0, 0.0],  d: IVec3::new(1, 0, 0),   corners: [[1,0,0],[1,0,1],[1,1,1],[1,1,0]] },
    Face { normal: [-1.0, 0.0, 0.0], d: IVec3::new(-1, 0, 0),  corners: [[0,0,0],[0,1,0],[0,1,1],[0,0,1]] },
    Face { normal: [0.0, 1.0, 0.0],  d: IVec3::new(0, 1, 0),   corners: [[0,1,0],[1,1,0],[1,1,1],[0,1,1]] },
    Face { normal: [0.0, -1.0, 0.0], d: IVec3::new(0, -1, 0),  corners: [[0,0,0],[0,0,1],[1,0,1],[1,0,0]] },
    Face { normal: [0.0, 0.0, 1.0],  d: IVec3::new(0, 0, 1),   corners: [[0,0,1],[0,1,1],[1,1,1],[1,0,1]] },
    Face { normal: [0.0, 0.0, -1.0], d: IVec3::new(0, 0, -1),  corners: [[0,0,0],[1,0,0],[1,1,0],[0,1,0]] },
];

pub struct Face {
    pub normal: [f32; 3],
    pub d: IVec3,
    pub corners: [[i32; 3]; 4],
}

fn face_shade(normal: [f32; 3]) -> f32 {
    if normal[1] > 0.5 { 1.0 }
    else if normal[1] < -0.5 { 0.45 }
    else if normal[0].abs() > 0.5 { 0.78 }
    else { 0.62 }
}

fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
}

pub fn build_mesh(grid: &VoxelGrid, hide: Option<IVec3>) -> Mesh {
    let mut positions: Vec<[f32; 3]> = Vec::new();
    let mut normals:   Vec<[f32; 3]> = Vec::new();
    let mut colors:    Vec<[f32; 4]> = Vec::new();
    let mut indices:   Vec<u32>      = Vec::new();

    let cell_filled = |p: IVec3| -> Option<[u8; 4]> {
        if hide == Some(p) {
            return None;
        }
        if !VoxelGrid::in_bounds(p) {
            return None;
        }
        grid.cells[p.x as usize][p.y as usize][p.z as usize]
    };

    for x in 0..GRID {
        for y in 0..GRID {
            for z in 0..GRID {
                let cell = IVec3::new(x as i32, y as i32, z as i32);
                let Some(rgba) = cell_filled(cell) else { continue; };
                let base_rgb = [
                    srgb_to_linear(rgba[0] as f32 / 255.0),
                    srgb_to_linear(rgba[1] as f32 / 255.0),
                    srgb_to_linear(rgba[2] as f32 / 255.0),
                ];
                let alpha = rgba[3] as f32 / 255.0;

                for f in &FACES {
                    let n = cell + f.d;
                    if cell_filled(n).is_some() {
                        continue;
                    }
                    let shade = face_shade(f.normal);
                    let col = [base_rgb[0] * shade, base_rgb[1] * shade, base_rgb[2] * shade, alpha];
                    let base = positions.len() as u32;
                    for c in &f.corners {
                        positions.push([
                            (cell.x + c[0]) as f32,
                            (cell.y + c[1]) as f32,
                            (cell.z + c[2]) as f32,
                        ]);
                        normals.push(f.normal);
                        colors.push(col);
                    }
                    indices.extend_from_slice(&[base, base + 2, base + 1, base, base + 3, base + 2]);
                }
            }
        }
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
