use bevy::asset::RenderAssetUsages;
use bevy::ecs::system::SystemParam;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;

use std::collections::HashSet;

use crate::mesh::srgb_to_linear;
use crate::preview::{PreviewGizmos, accent_outline_color};
use crate::theme::Theme;
use crate::tools::{
    CurrentColor, ExtraColors, ShapeOptions, ShapeState, Tool, ToolState, color_pool, sample_color,
};
use roxel::grid::Color8;
use roxel::shapes::{ShapePrimitive, compute_shape_cells};

#[derive(Component)]
pub struct ShapePreview;

#[derive(Resource)]
pub struct ShapePreviewHandles {
    pub mesh: Handle<Mesh>,
    pub material: Handle<StandardMaterial>,
}

pub fn spawn_shape_preview(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
) {
    let mesh = meshes.add(empty_mesh());
    let material = materials.add(StandardMaterial {
        base_color: Color::srgba(1.0, 1.0, 1.0, 0.4),
        unlit: true,
        alpha_mode: AlphaMode::Blend,
        cull_mode: None,
        ..default()
    });
    commands.spawn((
        Mesh3d(mesh.clone()),
        MeshMaterial3d(material.clone()),
        Transform::default(),
        Visibility::Hidden,
        ShapePreview,
    ));
    commands.insert_resource(ShapePreviewHandles { mesh, material });
}

fn empty_mesh() -> Mesh {
    let mut m = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );
    m.insert_attribute(Mesh::ATTRIBUTE_POSITION, Vec::<[f32; 3]>::new());
    m.insert_attribute(Mesh::ATTRIBUTE_NORMAL, Vec::<[f32; 3]>::new());
    m.insert_indices(Indices::U32(vec![]));
    m
}

#[derive(SystemParam)]
pub struct ShapePreviewInputs<'w> {
    tool: Res<'w, ToolState>,
    options: Res<'w, ShapeOptions>,
    state: Res<'w, ShapeState>,
    color: Res<'w, CurrentColor>,
    extras: Res<'w, ExtraColors>,
    handles: Res<'w, ShapePreviewHandles>,
    flyby: Res<'w, crate::camera::FlybyState>,
    theme: Res<'w, Theme>,
    snapshot: Res<'w, crate::snapshot::SnapshotInProgress>,
}

#[derive(SystemParam)]
pub struct ShapePreviewAssets<'w> {
    meshes: ResMut<'w, Assets<Mesh>>,
    materials: ResMut<'w, Assets<StandardMaterial>>,
}

#[derive(PartialEq)]
struct PreviewKey {
    primitive: ShapePrimitive,
    c1: IVec3,
    c2: IVec3,
    axis: usize,
    thickness: i32,
    normal_sign: i32,
    pool: Vec<Color8>,
}

#[derive(Default)]
pub struct PreviewCache {
    key: Option<PreviewKey>,
    segments: Vec<(Vec3, Vec3)>,
}

pub fn shape_preview_system(
    inputs: ShapePreviewInputs,
    mut assets: ShapePreviewAssets,
    mut q: Query<&mut Visibility, With<ShapePreview>>,
    mut gizmos: Gizmos<PreviewGizmos>,
    mut cache: Local<PreviewCache>,
) {
    let Ok(mut vis) = q.single_mut() else { return };

    if inputs.flyby.active
        || inputs.snapshot.0
        || inputs.tool.current != Tool::Shape
        || inputs.state.drag.is_idle()
    {
        *vis = Visibility::Hidden;
        return;
    }

    let (Some(anchor), Some(c1), Some(c2)) = (
        inputs.state.drag.anchor,
        inputs.state.drag.corner1,
        inputs.state.drag.corner2,
    ) else {
        *vis = Visibility::Hidden;
        return;
    };

    let pool = color_pool(inputs.color.0, &inputs.extras.0);
    let key = PreviewKey {
        primitive: inputs.options.primitive,
        c1,
        c2,
        axis: anchor.axis,
        thickness: inputs.state.drag.thickness,
        normal_sign: inputs.state.drag.normal_sign,
        pool,
    };
    if cache.key.as_ref() != Some(&key) {
        let Some(mesh) = assets.meshes.get_mut(&inputs.handles.mesh) else {
            *vis = Visibility::Hidden;
            return;
        };
        let cells = compute_shape_cells(
            key.primitive,
            c1,
            c2,
            key.axis,
            key.thickness,
            key.normal_sign,
        );
        let multi = key.pool.len() > 1;
        let (positions, normals, indices) = build_cubes_mesh(&cells);
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
        if multi {
            mesh.insert_attribute(
                Mesh::ATTRIBUTE_COLOR,
                build_cube_vertex_colors(&cells, &key.pool),
            );
        } else {
            mesh.remove_attribute(Mesh::ATTRIBUTE_COLOR.id);
        }
        mesh.insert_indices(Indices::U32(indices));

        if let Some(m) = assets.materials.get_mut(&inputs.handles.material) {
            if multi {
                // White base lets vertex colors show through unmodified.
                m.base_color = Color::srgba(1.0, 1.0, 1.0, 0.4);
            } else {
                let c = inputs.color.0;
                m.base_color = Color::srgba(
                    c[0] as f32 / 255.0,
                    c[1] as f32 / 255.0,
                    c[2] as f32 / 255.0,
                    0.4,
                );
            }
        }

        cache.segments = silhouette_segments(&cells);
        cache.key = Some(key);
    }

    let color = accent_outline_color(&inputs.theme);
    for &(a, b) in &cache.segments {
        gizmos.line(a, b, color);
    }

    *vis = Visibility::Visible;
}

fn silhouette_segments(cells: &[IVec3]) -> Vec<(Vec3, Vec3)> {
    const OFFSETS: [(usize, f32, IVec3); 6] = [
        (0, 1.0, IVec3::X),
        (0, 0.0, IVec3::NEG_X),
        (1, 1.0, IVec3::Y),
        (1, 0.0, IVec3::NEG_Y),
        (2, 1.0, IVec3::Z),
        (2, 0.0, IVec3::NEG_Z),
    ];
    const UV: [[f32; 2]; 4] = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
    let set: HashSet<IVec3> = cells.iter().copied().collect();
    let mut out = Vec::new();
    for &cell in cells {
        let p = cell.as_vec3();
        for &(axis, face_coord, normal) in &OFFSETS {
            if set.contains(&(cell + normal)) {
                continue;
            }
            let u_axis = (axis + 1) % 3;
            let v_axis = (axis + 2) % 3;
            let corners = UV.map(|uv| {
                let mut a = [0.0f32; 3];
                a[axis] = face_coord;
                a[u_axis] = uv[0];
                a[v_axis] = uv[1];
                p + Vec3::from_array(a)
            });
            let mut u_unit = IVec3::ZERO;
            u_unit[u_axis] = 1;
            let mut v_unit = IVec3::ZERO;
            v_unit[v_axis] = 1;
            let edges: [(usize, usize, IVec3); 4] = [
                (0, 1, -v_unit),
                (1, 2, u_unit),
                (2, 3, v_unit),
                (3, 0, -u_unit),
            ];
            for (a, b, tangent) in edges {
                let neighbor = cell + tangent;
                let neighbor_face_exposed =
                    set.contains(&neighbor) && !set.contains(&(neighbor + normal));
                if neighbor_face_exposed {
                    continue;
                }
                out.push((corners[a], corners[b]));
            }
        }
    }
    out
}

/// Per-vertex RGBA colors (24 verts/cell, six faces × four corners) sampled
/// deterministically from `pool` so the dithered preview matches what
/// `shape_commit` will write. Linear-space — Bevy expects linear vertex
/// colors (sRGB tonemapped at the final blit, see mesh::srgb_to_linear).
pub(crate) fn build_cube_vertex_colors(
    cells: &[IVec3],
    pool: &[roxel::grid::Color8],
) -> Vec<[f32; 4]> {
    let mut out = Vec::with_capacity(cells.len() * 24);
    for c in cells {
        let s = sample_color(*c, pool);
        let lin = [
            srgb_to_linear(s[0] as f32 / 255.0),
            srgb_to_linear(s[1] as f32 / 255.0),
            srgb_to_linear(s[2] as f32 / 255.0),
            1.0,
        ];
        for _ in 0..24 {
            out.push(lin);
        }
    }
    out
}

pub(crate) fn build_cubes_mesh(cells: &[IVec3]) -> (Vec<[f32; 3]>, Vec<[f32; 3]>, Vec<u32>) {
    let mut pos = Vec::with_capacity(cells.len() * 24);
    let mut nor = Vec::with_capacity(cells.len() * 24);
    let mut idx = Vec::with_capacity(cells.len() * 36);
    const FACES: [([f32; 3], [[f32; 3]; 4]); 6] = [
        (
            [1.0, 0.0, 0.0],
            [
                [1.0, 0.0, 0.0],
                [1.0, 0.0, 1.0],
                [1.0, 1.0, 1.0],
                [1.0, 1.0, 0.0],
            ],
        ),
        (
            [-1.0, 0.0, 0.0],
            [
                [0.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [0.0, 1.0, 1.0],
                [0.0, 0.0, 1.0],
            ],
        ),
        (
            [0.0, 1.0, 0.0],
            [
                [0.0, 1.0, 0.0],
                [1.0, 1.0, 0.0],
                [1.0, 1.0, 1.0],
                [0.0, 1.0, 1.0],
            ],
        ),
        (
            [0.0, -1.0, 0.0],
            [
                [0.0, 0.0, 0.0],
                [0.0, 0.0, 1.0],
                [1.0, 0.0, 1.0],
                [1.0, 0.0, 0.0],
            ],
        ),
        (
            [0.0, 0.0, 1.0],
            [
                [0.0, 0.0, 1.0],
                [0.0, 1.0, 1.0],
                [1.0, 1.0, 1.0],
                [1.0, 0.0, 1.0],
            ],
        ),
        (
            [0.0, 0.0, -1.0],
            [
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [1.0, 1.0, 0.0],
                [0.0, 1.0, 0.0],
            ],
        ),
    ];
    for c in cells {
        let base = [c.x as f32, c.y as f32, c.z as f32];
        for (n, corners) in FACES.iter() {
            let start = pos.len() as u32;
            for corner in corners {
                pos.push([
                    base[0] + corner[0],
                    base[1] + corner[1],
                    base[2] + corner[2],
                ]);
                nor.push(*n);
            }
            idx.push(start);
            idx.push(start + 1);
            idx.push(start + 2);
            idx.push(start);
            idx.push(start + 2);
            idx.push(start + 3);
        }
    }
    (pos, nor, idx)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn silhouette_segments_outline_every_edge_of_a_lone_cube() {
        assert_eq!(silhouette_segments(&[IVec3::ZERO]).len(), 24);
    }

    #[test]
    fn silhouette_segments_skip_the_seam_between_coplanar_faces() {
        let segments = silhouette_segments(&[IVec3::ZERO, IVec3::X]);
        assert_eq!(segments.len(), 32);
        let seam = |&(a, b): &(Vec3, Vec3)| a.x == 1.0 && b.x == 1.0;
        assert!(!segments.iter().any(seam));
    }
}
