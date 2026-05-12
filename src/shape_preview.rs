use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;

use crate::shapes::{ShapePrimitive, ellipse_cells, extrude, line2d_cells, rect_cells};
use crate::tools::{CurrentColor, ShapeOptions, ShapeState, Tool, ToolState};

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

pub fn shape_preview_system(
    tool: Res<ToolState>,
    options: Res<ShapeOptions>,
    state: Res<ShapeState>,
    color: Res<CurrentColor>,
    handles: Res<ShapePreviewHandles>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut q: Query<&mut Visibility, With<ShapePreview>>,
) {
    let Ok(mut vis) = q.single_mut() else { return };

    if tool.current != Tool::Shape || state.phase.is_none() {
        *vis = Visibility::Hidden;
        return;
    }

    let (Some(anchor), Some(c1), Some(c2)) = (state.anchor, state.corner1, state.corner2) else {
        *vis = Visibility::Hidden;
        return;
    };

    let base = match options.primitive {
        ShapePrimitive::Rectangle => rect_cells(c1, c2, anchor.axis, options.filled),
        ShapePrimitive::Ellipse => ellipse_cells(c1, c2, anchor.axis, options.filled),
        ShapePrimitive::Line => line2d_cells(c1, c2, anchor.axis),
    };
    let sign = if state.normal_sign == 0 { 1 } else { state.normal_sign };
    let cells = extrude(&base, anchor.axis, state.thickness.max(1), sign);

    let Some(mesh) = meshes.get_mut(&handles.mesh) else {
        *vis = Visibility::Hidden;
        return;
    };
    let (positions, normals, indices) = build_cubes_mesh(&cells);
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_indices(Indices::U32(indices));

    if let Some(m) = materials.get_mut(&handles.material) {
        let c = color.0;
        m.base_color = Color::srgba(
            c[0] as f32 / 255.0,
            c[1] as f32 / 255.0,
            c[2] as f32 / 255.0,
            0.4,
        );
    }

    *vis = Visibility::Visible;
}

fn build_cubes_mesh(cells: &[IVec3]) -> (Vec<[f32; 3]>, Vec<[f32; 3]>, Vec<u32>) {
    let mut pos = Vec::with_capacity(cells.len() * 24);
    let mut nor = Vec::with_capacity(cells.len() * 24);
    let mut idx = Vec::with_capacity(cells.len() * 36);
    const FACES: [([f32; 3], [[f32; 3]; 4]); 6] = [
        ([1.0, 0.0, 0.0],  [[1.0,0.0,0.0],[1.0,0.0,1.0],[1.0,1.0,1.0],[1.0,1.0,0.0]]),
        ([-1.0, 0.0, 0.0], [[0.0,0.0,0.0],[0.0,1.0,0.0],[0.0,1.0,1.0],[0.0,0.0,1.0]]),
        ([0.0, 1.0, 0.0],  [[0.0,1.0,0.0],[1.0,1.0,0.0],[1.0,1.0,1.0],[0.0,1.0,1.0]]),
        ([0.0, -1.0, 0.0], [[0.0,0.0,0.0],[0.0,0.0,1.0],[1.0,0.0,1.0],[1.0,0.0,0.0]]),
        ([0.0, 0.0, 1.0],  [[0.0,0.0,1.0],[0.0,1.0,1.0],[1.0,1.0,1.0],[1.0,0.0,1.0]]),
        ([0.0, 0.0, -1.0], [[0.0,0.0,0.0],[1.0,0.0,0.0],[1.0,1.0,0.0],[0.0,1.0,0.0]]),
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
