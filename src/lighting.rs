use crate::grid::DEFAULT_SIZE;
use bevy::light::CascadeShadowConfigBuilder;
use bevy::prelude::*;

pub fn spawn_lights(commands: &mut Commands) {
    // Initial position from default size; light isn't recentered on resize
    // since shadow-less directional lighting is location-insensitive at the
    // scales we care about.
    let center = Vec3::splat(DEFAULT_SIZE as f32 / 2.0);
    let cascade = CascadeShadowConfigBuilder {
        num_cascades: 2,
        maximum_distance: 400.0,
        ..default()
    }
    .build();
    commands.spawn((
        DirectionalLight {
            illuminance: 1_500.0,
            shadows_enabled: false,
            ..default()
        },
        Transform::from_translation(center + Vec3::new(100.0, 140.0, 80.0))
            .looking_at(center, Vec3::Y),
        cascade,
    ));
}
