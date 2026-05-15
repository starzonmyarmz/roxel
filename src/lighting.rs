use bevy::light::CascadeShadowConfigBuilder;
use bevy::prelude::*;

pub fn spawn_lights(commands: &mut Commands) {
    // Directional light is location-insensitive at the scales we care about,
    // so anchor it at origin. The open-world grid has no centroid to track.
    let center = Vec3::ZERO;
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
