use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_egui::EguiContexts;
use bevy_panorbit_camera::PanOrbitCamera;

use crate::grid::VoxelGrid;
use crate::mesh::PreviewHide;
use crate::picking::{cursor_ray, pick};
use crate::tools::{CurrentColor, PointerState, Tool, ToolState};

#[derive(Component)]
pub struct BrushPreview;

#[derive(Resource)]
pub struct BrushPreviewMaterial(pub Handle<StandardMaterial>);

pub fn spawn_brush_preview(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
) {
    let mesh = meshes.add(Mesh::from(bevy::math::primitives::Cuboid::new(1.0, 1.0, 1.0)));
    let mat = materials.add(StandardMaterial {
        base_color: Color::srgba(1.0, 1.0, 1.0, 0.45),
        unlit: true,
        alpha_mode: AlphaMode::Blend,
        cull_mode: None,
        ..default()
    });
    commands.spawn((
        Mesh3d(mesh),
        MeshMaterial3d(mat.clone()),
        Transform::default(),
        Visibility::Hidden,
        BrushPreview,
    ));
    commands.insert_resource(BrushPreviewMaterial(mat));
}

pub fn brush_preview_system(
    mut contexts: EguiContexts,
    keys: Res<ButtonInput<KeyCode>>,
    cameras: Query<(&Camera, &GlobalTransform), With<PanOrbitCamera>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    grid: Res<VoxelGrid>,
    tool: Res<ToolState>,
    color: Res<CurrentColor>,
    pointer: Res<PointerState>,
    mat_handle: Res<BrushPreviewMaterial>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut hide: ResMut<PreviewHide>,
    mut q: Query<(&mut Transform, &mut Visibility), With<BrushPreview>>,
    gizmo_drag: Res<crate::gizmo::GizmoDrag>,
    gizmo_rect: Res<crate::gizmo::GizmoRect>,
) {
    let Ok((mut tf, mut vis)) = q.single_mut() else { return };

    let egui_wants_pointer = contexts
        .ctx_mut()
        .map(|c| c.is_pointer_over_area())
        .unwrap_or(false);

    let clear = |vis: &mut Visibility, hide: &mut PreviewHide| {
        *vis = Visibility::Hidden;
        hide.set(None);
    };

    if egui_wants_pointer || gizmo_drag.active || keys.pressed(KeyCode::Space) || pointer.stroking {
        clear(&mut vis, &mut hide);
        return;
    }
    if let (Some(rect), Ok(window)) = (gizmo_rect.0, windows.single())
        && let Some(c) = window.cursor_position()
            && rect.contains(c) {
                clear(&mut vis, &mut hide);
                return;
            }

    let Some((origin, dir)) = cursor_ray(&cameras, &windows) else {
        clear(&mut vis, &mut hide);
        return;
    };
    let Some(hit) = pick(&grid, origin, dir) else {
        clear(&mut vis, &mut hide);
        return;
    };

    match tool.current {
        Tool::Brush => {
            hide.set(None);
            let target = hit.cell + hit.normal;
            if !VoxelGrid::in_bounds(target) {
                *vis = Visibility::Hidden;
                return;
            }
            let c = color.0;
            let col = Color::srgba(
                c[0] as f32 / 255.0,
                c[1] as f32 / 255.0,
                c[2] as f32 / 255.0,
                0.45,
            );
            let pos = target.as_vec3() + Vec3::splat(0.5);
            *tf = Transform::from_translation(pos).with_scale(Vec3::splat(1.0));
            *vis = Visibility::Visible;
            if let Some(m) = materials.get_mut(&mat_handle.0) {
                m.base_color = col;
            }
        }
        Tool::Erase => {
            *vis = Visibility::Hidden;
            if hit.hit_voxel {
                hide.set(Some(hit.cell));
            } else {
                hide.set(None);
            }
        }
        _ => {
            clear(&mut vis, &mut hide);
        }
    }
}
