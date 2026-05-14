use crate::grid::{DEFAULT_SIZE, VoxelGrid};
use bevy::prelude::*;
use bevy_egui::PrimaryEguiContext;
use bevy_panorbit_camera::PanOrbitCamera;

/// Default camera focus: center of the floor plane, not the cubic-grid centroid.
/// Centering on the floor makes the ground plane sit in the middle of the
/// viewport on launch instead of dropping into the lower half.
pub fn default_camera_focus(size: usize) -> Vec3 {
    Vec3::new(size as f32 / 2.0, 0.0, size as f32 / 2.0)
}

/// Iso camera offset from focus: equal contribution per axis gives the
/// canonical isometric direction (azimuth 45°, elevation arctan(1/√2) ≈ 35.26°).
pub fn iso_camera_offset() -> Vec3 {
    Vec3::splat(80.0)
}

pub fn spawn_camera(commands: &mut Commands) {
    let focus = default_camera_focus(DEFAULT_SIZE);
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
    for x in 0..grid.size {
        for y in 0..grid.size {
            for z in 0..grid.size {
                if grid.cell(x, y, z).is_some() {
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

/// World-space offset that lands `centroid` at the visible viewport center
/// when added to `target_focus`. Pure ratio math: both rects must be in the
/// same units (egui points). Avoids the winit-logical-vs-egui-physical pixel
/// mismatch that caused the focus to fly off-screen at app launch.
pub fn panel_compensation_offset(
    screen_rect: bevy::math::Rect,
    avail_rect: bevy::math::Rect,
    radius: f32,
    fov: f32,
    view_right: Vec3,
    view_up: Vec3,
) -> Vec3 {
    let size = screen_rect.max - screen_rect.min;
    if size.y < 1e-4 {
        return Vec3::ZERO;
    }
    let screen_center = (screen_rect.min + screen_rect.max) * 0.5;
    let avail_center = (avail_rect.min + avail_rect.max) * 0.5;
    let delta = screen_center - avail_center;
    if delta.length_squared() < 1e-4 {
        return Vec3::ZERO;
    }
    let world_per_unit = 2.0 * radius * (fov * 0.5).tan() / size.y;
    view_right * (delta.x * world_per_unit) + view_up * (-delta.y * world_per_unit)
}

pub fn frame_view_system(
    keys: Res<ButtonInput<KeyCode>>,
    mut cameras: Query<(&mut PanOrbitCamera, &GlobalTransform, &Projection)>,
    grid: Res<VoxelGrid>,
    viewport: Res<ViewportRect>,
) {
    if !keys.just_pressed(KeyCode::Digit0) && !keys.just_pressed(KeyCode::Numpad0) {
        return;
    }
    let cmd = keys.pressed(KeyCode::SuperLeft)
        || keys.pressed(KeyCode::SuperRight)
        || keys.pressed(KeyCode::ControlLeft)
        || keys.pressed(KeyCode::ControlRight);
    if !cmd {
        return;
    }

    let (centroid, radius) = fit_view(&grid).unwrap_or_else(|| {
        let center = Vec3::splat(grid.size as f32 / 2.0);
        (center, grid.size as f32 * 1.875)
    });

    let panel_offset = (|| -> Option<Vec3> {
        let screen = viewport.screen?;
        let avail = viewport.avail?;
        let (_, xform, projection) = cameras.iter().next()?;
        let Projection::Perspective(persp) = projection else {
            return None;
        };
        Some(panel_compensation_offset(
            screen,
            avail,
            radius,
            persp.fov,
            xform.right().as_vec3(),
            xform.up().as_vec3(),
        ))
    })()
    .unwrap_or(Vec3::ZERO);

    let target_focus = centroid + panel_offset;
    for (mut cam, _, _) in &mut cameras {
        cam.target_focus = target_focus;
        cam.target_radius = radius;
    }
}

/// Pending camera recenter triggered on startup and on every project rebuild.
/// Consumed once `ViewportRect` is populated so panel widths don't have to be
/// guessed.
#[derive(Resource, Default)]
pub struct RecenterRequest {
    pub base_focus: Option<Vec3>,
}

pub fn apply_recenter_system(
    mut request: ResMut<RecenterRequest>,
    mut cameras: Query<(&mut PanOrbitCamera, &GlobalTransform, &Projection)>,
    viewport: Res<ViewportRect>,
) {
    let Some(base) = request.base_focus else {
        return;
    };
    let Some(screen) = viewport.screen else {
        return;
    };
    let Some(avail) = viewport.avail else {
        return;
    };
    let Some((mut cam, xform, projection)) = cameras.iter_mut().next() else {
        return;
    };
    let Projection::Perspective(persp) = projection else {
        return;
    };
    let offset = panel_compensation_offset(
        screen,
        avail,
        cam.target_radius,
        persp.fov,
        xform.right().as_vec3(),
        xform.up().as_vec3(),
    );
    cam.target_focus = base + offset;
    request.base_focus = None;
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
        let f = default_camera_focus(DEFAULT_SIZE);
        assert_eq!(f.y, 0.0);
        assert_eq!(f.x, DEFAULT_SIZE as f32 / 2.0);
        assert_eq!(f.z, DEFAULT_SIZE as f32 / 2.0);
    }

    #[test]
    fn fit_view_empty_grid_returns_none() {
        let grid = VoxelGrid::default();
        assert!(fit_view(&grid).is_none());
    }

    #[test]
    fn zoom_in_halves_radius() {
        assert!((apply_zoom(10.0, 0.5, 0.5, None) - 5.0).abs() < 1e-6);
    }

    #[test]
    fn zoom_out_doubles_radius() {
        assert!((apply_zoom(10.0, 2.0, 0.5, None) - 20.0).abs() < 1e-6);
    }

    #[test]
    fn zoom_clamps_to_lower_limit() {
        assert!((apply_zoom(0.6, 0.5, 0.5, None) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn zoom_clamps_to_upper_limit() {
        assert!((apply_zoom(60.0, 2.0, 0.5, Some(100.0)) - 100.0).abs() < 1e-6);
    }

    #[test]
    fn zoom_radius_limits_yield_1000_pct_at_lower_bound() {
        let mut grid = VoxelGrid::default();
        grid.set(IVec3::new(10, 10, 10), Some([255, 0, 0, 255]));
        let (lower, upper) = zoom_radius_limits(&grid);
        let (_, fit_radius) = fit_view(&grid).expect("non-empty grid");
        // At lower bound, displayed zoom% must equal MAX_ZOOM_PCT.
        let pct = (fit_radius / lower) * 100.0;
        assert!((pct - MAX_ZOOM_PCT).abs() < 1e-3, "pct={pct}");
        assert!(upper > fit_radius * 100.0, "upper={upper} fit={fit_radius}");
    }

    #[test]
    fn zoom_radius_limits_empty_grid_uses_fallback() {
        let grid = VoxelGrid::default();
        let (lower, upper) = zoom_radius_limits(&grid);
        assert!(lower > 0.0);
        assert!(upper > lower);
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

/// Egui-points rects describing the canvas. `avail` is the area not covered
/// by side/top/bottom panels; `screen` is the full window in the same units.
/// Both come from the same `egui::Context` call so they share coordinate space
/// — needed for `panel_compensation_offset` to compute correct ratios.
#[derive(Resource, Default)]
pub struct ViewportRect {
    pub avail: Option<bevy::math::Rect>,
    pub screen: Option<bevy::math::Rect>,
}

pub fn update_viewport_rect(
    mut contexts: bevy_egui::EguiContexts,
    mut rect_res: ResMut<ViewportRect>,
) {
    if let Ok(ctx) = contexts.ctx_mut() {
        let a = ctx.available_rect();
        let s = ctx.content_rect();
        rect_res.avail = Some(bevy::math::Rect {
            min: Vec2::new(a.min.x, a.min.y),
            max: Vec2::new(a.max.x, a.max.y),
        });
        rect_res.screen = Some(bevy::math::Rect {
            min: Vec2::new(s.min.x, s.min.y),
            max: Vec2::new(s.max.x, s.max.y),
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
        cam.target_radius = apply_zoom(
            cam.target_radius,
            factor,
            cam.zoom_lower_limit,
            cam.zoom_upper_limit,
        );
    }
}

/// Apply a zoom factor to `radius`, clamped to `[lower_limit, upper_limit]`.
/// Pure helper so the key/click systems share identical math and tests don't
/// need a Bevy app. `upper_limit = None` leaves the upper end unconstrained.
pub fn apply_zoom(radius: f32, factor: f32, lower_limit: f32, upper_limit: Option<f32>) -> f32 {
    let r = (radius * factor).max(lower_limit);
    match upper_limit {
        Some(u) => r.min(u),
        None => r,
    }
}

/// Dynamic zoom radius limits derived from the current grid. Maps a fixed
/// percentage range (zoom ∈ [`MIN_ZOOM_PCT`, `MAX_ZOOM_PCT`]) onto a radius
/// range relative to `fit_view`'s radius. Empty grids fall back to the default
/// fit estimate so we still produce sensible limits before the user paints.
pub fn zoom_radius_limits(grid: &VoxelGrid) -> (f32, f32) {
    let fit = fit_view(grid)
        .map(|(_, r)| r)
        .unwrap_or_else(|| grid.size as f32 * 1.875);
    // 1000% = zoomed in 10×, so radius = fit / 10.
    // 0% can't be exact (infinity); pick a large multiple that rounds to 0%.
    let lower = (fit / (MAX_ZOOM_PCT / 100.0)).max(0.01);
    let upper = fit * 1000.0;
    (lower, upper)
}

/// Public min/max for the zoom percentage readout. Both UI and clamp logic
/// agree on the same range so a value can't be both displayable and unreachable.
pub const MIN_ZOOM_PCT: f32 = 0.0;
pub const MAX_ZOOM_PCT: f32 = 1000.0;

pub fn update_zoom_limits_system(grid: Res<VoxelGrid>, mut cameras: Query<&mut PanOrbitCamera>) {
    let (lower, upper) = zoom_radius_limits(&grid);
    for mut cam in &mut cameras {
        cam.zoom_lower_limit = lower;
        cam.zoom_upper_limit = Some(upper);
        if cam.target_radius < lower {
            cam.target_radius = lower;
        }
        if cam.target_radius > upper {
            cam.target_radius = upper;
        }
    }
}

pub fn zoom_key_system(
    keys: Res<ButtonInput<KeyCode>>,
    mut contexts: bevy_egui::EguiContexts,
    mut cameras: Query<&mut PanOrbitCamera>,
) {
    let egui_wants_keyboard = contexts
        .ctx_mut()
        .map(|c| c.wants_keyboard_input())
        .unwrap_or(false);
    if egui_wants_keyboard {
        return;
    }
    let cmd = keys.pressed(KeyCode::SuperLeft)
        || keys.pressed(KeyCode::SuperRight)
        || keys.pressed(KeyCode::ControlLeft)
        || keys.pressed(KeyCode::ControlRight);
    if !cmd {
        return;
    }
    let zoom_in = keys.just_pressed(KeyCode::Equal) || keys.just_pressed(KeyCode::NumpadAdd);
    let zoom_out = keys.just_pressed(KeyCode::Minus) || keys.just_pressed(KeyCode::NumpadSubtract);
    if !zoom_in && !zoom_out {
        return;
    }
    let factor = if zoom_in { 0.5 } else { 2.0 };
    for mut cam in &mut cameras {
        cam.target_radius = apply_zoom(
            cam.target_radius,
            factor,
            cam.zoom_lower_limit,
            cam.zoom_upper_limit,
        );
    }
}
