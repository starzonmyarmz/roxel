use crate::grid::{GRID, VoxelGrid};
use bevy::prelude::*;
use bevy_egui::PrimaryEguiContext;
use bevy_panorbit_camera::PanOrbitCamera;

pub fn spawn_camera(commands: &mut Commands) {
    let center = Vec3::splat(GRID as f32 / 2.0);
    commands.spawn((
        Camera3d::default(),
        Transform::from_translation(center + Vec3::new(80.0, 80.0, 80.0))
            .looking_at(center, Vec3::Y),
        PrimaryEguiContext,
        PanOrbitCamera {
            focus: center,
            radius: Some(120.0),
            yaw_upper_limit: None,
            yaw_lower_limit: None,
            pitch_upper_limit: Some(std::f32::consts::FRAC_PI_2 - 0.05),
            pitch_lower_limit: Some(-std::f32::consts::FRAC_PI_2 + 0.05),
            button_orbit: MouseButton::Right,
            button_pan: MouseButton::Left,
            modifier_pan: Some(KeyCode::Space),
            pan_sensitivity: 1.0,
            zoom_lower_limit: 0.5,
            ..default()
        },
    ));
}

fn voxel_bounds(grid: &VoxelGrid) -> Option<(IVec3, IVec3)> {
    let mut min = IVec3::splat(i32::MAX);
    let mut max = IVec3::splat(i32::MIN);
    let mut any = false;
    for x in 0..GRID {
        for y in 0..GRID {
            for z in 0..GRID {
                if grid.cells[x][y][z].is_some() {
                    any = true;
                    let p = IVec3::new(x as i32, y as i32, z as i32);
                    min = min.min(p);
                    max = max.max(p);
                }
            }
        }
    }
    if any { Some((min, max)) } else { None }
}

pub fn frame_view_system(
    keys: Res<ButtonInput<KeyCode>>,
    mut cameras: Query<&mut PanOrbitCamera>,
    grid: Res<VoxelGrid>,
) {
    if !keys.just_pressed(KeyCode::KeyF) {
        return;
    }
    let modded = keys.pressed(KeyCode::SuperLeft)
        || keys.pressed(KeyCode::SuperRight)
        || keys.pressed(KeyCode::ControlLeft)
        || keys.pressed(KeyCode::ControlRight);
    if modded {
        return;
    }

    let (centroid, radius) = if let Some((min, max)) = voxel_bounds(&grid) {
        let centroid = (min.as_vec3() + max.as_vec3() + Vec3::ONE) * 0.5;
        let extent = (max - min).as_vec3().max_element() + 1.0;
        (centroid, (extent * 1.6).max(4.0))
    } else {
        let center = Vec3::splat(GRID as f32 / 2.0);
        (center, 120.0)
    };

    for mut cam in &mut cameras {
        cam.target_focus = centroid;
        cam.target_radius = radius;
    }
}

pub fn zoom_click_system(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut contexts: bevy_egui::EguiContexts,
    mut cameras: Query<&mut PanOrbitCamera>,
) {
    if !keys.pressed(KeyCode::KeyZ) || !mouse.just_pressed(MouseButton::Left) {
        return;
    }
    let egui_wants_pointer = contexts
        .ctx_mut()
        .map(|c| c.is_pointer_over_area() || c.wants_pointer_input())
        .unwrap_or(false);
    if egui_wants_pointer {
        return;
    }
    let alt = keys.pressed(KeyCode::AltLeft) || keys.pressed(KeyCode::AltRight);
    let factor = if alt { 2.0 } else { 0.5 };
    for mut cam in &mut cameras {
        cam.target_radius = (cam.target_radius * factor).max(cam.zoom_lower_limit);
    }
}
