use crate::grid::GRID;
use bevy::light::CascadeShadowConfigBuilder;
use bevy::prelude::*;

#[derive(Component)]
pub struct SunLight;

#[derive(Resource)]
pub struct LightControls {
    pub azimuth: f32,   // radians, around Y
    pub elevation: f32, // radians, above horizon
    pub intensity: f32,
}

impl Default for LightControls {
    fn default() -> Self {
        Self {
            azimuth: 0.6,
            elevation: 0.9,
            intensity: 10_000.0,
        }
    }
}

pub fn spawn_lights(commands: &mut Commands) {
    let center = Vec3::splat(GRID as f32 / 2.0);
    let cascade = CascadeShadowConfigBuilder {
        num_cascades: 2,
        maximum_distance: 400.0,
        ..default()
    }
    .build();
    commands.spawn((
        DirectionalLight {
            illuminance: 10_000.0,
            shadows_enabled: true,
            ..default()
        },
        Transform::from_translation(center + Vec3::new(100.0, 100.0, 100.0))
            .looking_at(center, Vec3::Y),
        cascade,
        SunLight,
    ));
}

pub fn update_light_system(
    controls: Res<LightControls>,
    mut q: Query<(&mut Transform, &mut DirectionalLight), With<SunLight>>,
) {
    let Ok((mut tf, mut light)) = q.single_mut() else { return; };
    let dir = Vec3::new(
        controls.elevation.cos() * controls.azimuth.cos(),
        controls.elevation.sin(),
        controls.elevation.cos() * controls.azimuth.sin(),
    );
    let center = Vec3::splat(GRID as f32 / 2.0);
    *tf = Transform::from_translation(center + dir * 200.0).looking_at(center, Vec3::Y);
    light.illuminance = controls.intensity;
}
