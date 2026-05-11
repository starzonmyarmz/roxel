use crate::grid::GRID;
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
            button_pan: MouseButton::Right,
            modifier_pan: Some(KeyCode::ShiftLeft),
            ..default()
        },
    ));
}
