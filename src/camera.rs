use crate::grid::{GRID, VoxelGrid};
use bevy::prelude::*;
use bevy_egui::PrimaryEguiContext;
use bevy_panorbit_camera::PanOrbitCamera;

/// Default camera focus: center of the floor plane, not the cubic-grid centroid.
/// Centering on the floor makes the ground plane sit in the middle of the
/// viewport on launch instead of dropping into the lower half.
pub fn default_camera_focus() -> Vec3 {
    Vec3::new(GRID as f32 / 2.0, 0.0, GRID as f32 / 2.0)
}

/// Iso camera offset from focus: equal contribution per axis gives the
/// canonical isometric direction (azimuth 45°, elevation arctan(1/√2) ≈ 35.26°).
pub fn iso_camera_offset() -> Vec3 {
    Vec3::splat(80.0)
}

pub fn spawn_camera(commands: &mut Commands) {
    let focus = default_camera_focus();
    let offset = iso_camera_offset();
    commands.spawn((
        Camera3d::default(),
        Transform::from_translation(focus + offset).looking_at(focus, Vec3::Y),
        PrimaryEguiContext,
        PanOrbitCamera {
            focus,
            radius: Some(offset.length()),
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

/// Focus + orbit radius that frames every occupied voxel. Returns `None`
/// when the grid is empty so callers (UI zoom readout, F-fit) can fall
/// back to a default or hide the value.
pub fn fit_view(grid: &VoxelGrid) -> Option<(Vec3, f32)> {
    let (min, max) = voxel_bounds(grid)?;
    let centroid = (min.as_vec3() + max.as_vec3() + Vec3::ONE) * 0.5;
    let extent = (max - min).as_vec3().max_element() + 1.0;
    Some((centroid, (extent * 1.6).max(4.0)))
}

pub fn frame_view_system(
    keys: Res<ButtonInput<KeyCode>>,
    mut cameras: Query<(&mut PanOrbitCamera, &GlobalTransform, &Projection)>,
    grid: Res<VoxelGrid>,
    windows: Query<&bevy::window::Window, With<bevy::window::PrimaryWindow>>,
    viewport: Res<ViewportRect>,
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

    let (centroid, radius) = fit_view(&grid).unwrap_or_else(|| {
        let center = Vec3::splat(GRID as f32 / 2.0);
        (center, 120.0)
    });

    // Compute world-space offset that lands the centroid at the visible
    // viewport center (not the window center) once the camera is moved.
    let panel_offset = (|| -> Option<Vec3> {
        let win = windows.single().ok()?;
        let rect = viewport.0?;
        let (_, xform, projection) = cameras.iter().next()?;
        let Projection::Perspective(persp) = projection else {
            return None;
        };
        let win_center = Vec2::new(win.width() * 0.5, win.height() * 0.5);
        let avail_center = (rect.min + rect.max) * 0.5;
        let delta = win_center - avail_center;
        if delta.length_squared() < 1e-4 {
            return None;
        }
        let world_per_pixel = 2.0 * radius * (persp.fov * 0.5).tan() / win.height();
        let view_right = xform.right().as_vec3();
        let view_up = xform.up().as_vec3();
        Some(
            view_right * (delta.x * world_per_pixel)
                + view_up * (-delta.y * world_per_pixel),
        )
    })()
    .unwrap_or(Vec3::ZERO);

    let target_focus = centroid + panel_offset;
    for (mut cam, _, _) in &mut cameras {
        cam.target_focus = target_focus;
        cam.target_radius = radius;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iso_offset_is_equal_per_axis() {
        let o = iso_camera_offset();
        assert!((o.x - o.y).abs() < 1e-6);
        assert!((o.y - o.z).abs() < 1e-6);
        assert!(o.x > 0.0);
    }

    #[test]
    fn iso_offset_yields_iso_elevation_and_azimuth() {
        // azimuth from +X around +Y should be 45°, elevation 35.264°.
        let o = iso_camera_offset();
        let len = o.length();
        let elevation = (o.y / len).asin().to_degrees();
        let azimuth = o.z.atan2(o.x).to_degrees();
        assert!((elevation - 35.2643).abs() < 1e-3, "elevation={elevation}");
        assert!((azimuth - 45.0).abs() < 1e-3, "azimuth={azimuth}");
    }

    #[test]
    fn default_focus_sits_on_floor_centered_in_grid() {
        let f = default_camera_focus();
        assert_eq!(f.y, 0.0);
        assert_eq!(f.x, GRID as f32 / 2.0);
        assert_eq!(f.z, GRID as f32 / 2.0);
    }

    #[test]
    fn fit_view_empty_grid_returns_none() {
        let grid = VoxelGrid::default();
        assert!(fit_view(&grid).is_none());
    }

    #[test]
    fn fit_view_single_voxel_returns_centered_radius() {
        let mut grid = VoxelGrid::default();
        grid.set(IVec3::new(10, 10, 10), Some([255, 0, 0, 255]));
        let (focus, radius) = fit_view(&grid).expect("non-empty grid");
        assert!((focus.x - 10.5).abs() < 1e-5);
        assert!((focus.y - 10.5).abs() < 1e-5);
        assert!((focus.z - 10.5).abs() < 1e-5);
        // extent = 1, raw radius = 1.6, clamped to 4.0
        assert!((radius - 4.0).abs() < 1e-5);
    }
}

/// Logical-point rect (egui coordinates) describing the visible 3D viewport
/// area, i.e. window minus side/top/bottom panels. Populated each frame from
/// `ctx.available_rect()`; read by `frame_view_system` to compensate F's
/// centering for asymmetric UI panel layout.
#[derive(Resource, Default)]
pub struct ViewportRect(pub Option<bevy::math::Rect>);

pub fn update_viewport_rect(
    mut contexts: bevy_egui::EguiContexts,
    mut rect_res: ResMut<ViewportRect>,
) {
    if let Ok(ctx) = contexts.ctx_mut() {
        let r = ctx.available_rect();
        rect_res.0 = Some(bevy::math::Rect {
            min: Vec2::new(r.min.x, r.min.y),
            max: Vec2::new(r.max.x, r.max.y),
        });
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
