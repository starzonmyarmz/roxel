use crate::grid::VoxelGrid;
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::prelude::*;
use bevy_egui::PrimaryEguiContext;
use bevy_panorbit_camera::PanOrbitCamera;
use std::f32::consts::{FRAC_PI_2, FRAC_PI_4, PI, TAU};

/// Default orbit radius used when the world is empty (no occupied cells to
/// frame) and as the initial spawn radius. Picked to comfortably show the
/// origin gizmo plus a few chunks of floor around it.
pub const EMPTY_WORLD_RADIUS: f32 = 32.0;

/// Default camera focus in the open world: the world origin. The floor
/// follows the focus, so the floor stays under the camera even before any
/// voxels are painted.
pub fn default_camera_focus() -> Vec3 {
    Vec3::ZERO
}

/// Iso camera offset from focus. Length matches `EMPTY_WORLD_RADIUS` so the
/// orbit radius at spawn agrees with the empty-world fallback used by
/// `frame_view_system` — otherwise pressing Cmd+0 on a fresh scene would
/// snap to a different distance than the initial view.
pub fn iso_camera_offset() -> Vec3 {
    Vec3::splat(EMPTY_WORLD_RADIUS / 3f32.sqrt())
}

pub fn spawn_camera(commands: &mut Commands) {
    let focus = default_camera_focus();
    let offset = iso_camera_offset();
    commands.spawn((
        Camera3d::default(),
        // Force tonemapping off so voxel colors render as the exact sRGB values
        // the user picked in the palette. Voxel materials are `unlit`, so the
        // default tonemap curve only desaturates/darkens output vs. the egui
        // swatch with no benefit. Matches the snapshot camera.
        Tonemapping::None,
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

/// Focus + orbit radius that frames every occupied voxel. Returns `None`
/// when the grid is empty so callers (UI, Cmd+0 frame, zoom limits) can
/// fall back to the empty-world default.
pub fn fit_view(grid: &VoxelGrid) -> Option<(Vec3, f32)> {
    let (min, max) = grid.bounding_box()?;
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
    mut pending: ResMut<PendingFrameView>,
    mut cameras: Query<(&mut PanOrbitCamera, &GlobalTransform, &Projection)>,
    grid: Res<VoxelGrid>,
    viewport: Res<ViewportRect>,
) {
    let key_trigger = (keys.just_pressed(KeyCode::Digit0) || keys.just_pressed(KeyCode::Numpad0))
        && (keys.pressed(KeyCode::SuperLeft)
            || keys.pressed(KeyCode::SuperRight)
            || keys.pressed(KeyCode::ControlLeft)
            || keys.pressed(KeyCode::ControlRight));
    let menu_trigger = std::mem::take(&mut pending.0);
    if !key_trigger && !menu_trigger {
        return;
    }

    let (centroid, radius) =
        fit_view(&grid).unwrap_or((default_camera_focus(), EMPTY_WORLD_RADIUS));

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

/// Auto-orbit "drone" camera. While `active`, `flyby_system` rewrites the
/// PanOrbitCamera target_* fields every frame from a parametric path, so user
/// RMB-drag is silently absorbed — only Esc or another palette toggle stops
/// it. Painting and ghost previews are gated separately (see `tools.rs` and
/// `preview.rs`).
#[derive(Resource, Default)]
pub struct FlybyState {
    pub active: bool,
    pub t: f32,
}

pub const FLYBY_YAW_SPEED: f32 = 0.30;
pub const FLYBY_PITCH_MID: f32 = 0.52;
pub const FLYBY_PITCH_AMP: f32 = 0.26;
pub const FLYBY_PITCH_FREQ: f32 = 0.07;
pub const FLYBY_RADIUS_AMP: f32 = 0.25;
pub const FLYBY_RADIUS_FREQ: f32 = 0.10;
const FLYBY_PITCH_MIN: f32 = 0.05;
const FLYBY_PITCH_MAX: f32 = FRAC_PI_2 - 0.05;

pub fn flyby_yaw(t: f32, start_yaw: f32) -> f32 {
    start_yaw + t * FLYBY_YAW_SPEED
}

pub fn flyby_pitch(t: f32) -> f32 {
    let raw = FLYBY_PITCH_MID + FLYBY_PITCH_AMP * (t * FLYBY_PITCH_FREQ * TAU).sin();
    raw.clamp(FLYBY_PITCH_MIN, FLYBY_PITCH_MAX)
}

pub fn flyby_radius(t: f32, base: f32) -> f32 {
    // +π/2 phase so sin starts at 1 → at t=0 the breath sits at +amp above
    // base. We use that engagement frame to anchor base_radius from
    // `fit_view`, then the curve sweeps base*0.75 .. base*1.25 from there.
    base * (1.0 + FLYBY_RADIUS_AMP * (t * FLYBY_RADIUS_FREQ * TAU + FRAC_PI_2).sin())
}

pub fn flyby_system(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    grid: Res<VoxelGrid>,
    mut state: ResMut<FlybyState>,
    mut cameras: Query<&mut PanOrbitCamera>,
    mut start_yaw: Local<f32>,
    mut base_radius: Local<f32>,
) {
    if state.active && keys.just_pressed(KeyCode::Escape) {
        state.active = false;
    }
    if !state.active {
        state.t = 0.0;
        return;
    }

    let Some(mut cam) = cameras.iter_mut().next() else {
        return;
    };

    if state.t == 0.0 {
        *start_yaw = cam.target_yaw;
        let (focus, fit) = fit_view(&grid).unwrap_or((default_camera_focus(), EMPTY_WORLD_RADIUS));
        *base_radius = fit;
        cam.target_focus = focus;
    }

    state.t += time.delta_secs();

    let upper = cam.zoom_upper_limit.unwrap_or(f32::MAX);
    cam.target_yaw = flyby_yaw(state.t, *start_yaw);
    cam.target_pitch = flyby_pitch(state.t);
    cam.target_radius = flyby_radius(state.t, *base_radius).clamp(cam.zoom_lower_limit, upper);
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
        let o = iso_camera_offset();
        let len = o.length();
        let elevation = (o.y / len).asin().to_degrees();
        let azimuth = o.z.atan2(o.x).to_degrees();
        assert!((elevation - 35.2643).abs() < 1e-3, "elevation={elevation}");
        assert!((azimuth - 45.0).abs() < 1e-3, "azimuth={azimuth}");
    }

    #[test]
    fn iso_offset_length_matches_empty_world_radius() {
        assert!((iso_camera_offset().length() - EMPTY_WORLD_RADIUS).abs() < 1e-4);
    }

    #[test]
    fn default_focus_is_world_origin() {
        assert_eq!(default_camera_focus(), Vec3::ZERO);
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
    fn zoom_step_in_and_out_are_reciprocal() {
        // √2 in and out cancel exactly so toggling zoom in/out returns the
        // user to the same radius without drift.
        let product = ZOOM_STEP_IN * ZOOM_STEP_OUT;
        assert!((product - 1.0).abs() < 1e-6, "product={product}");
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
    fn zoom_radius_lower_bound_is_fixed() {
        let mut grid = VoxelGrid::default();
        grid.set(IVec3::new(10, 10, 10), Some([255, 0, 0, 255]));
        let (lower, _) = zoom_radius_limits(&grid);
        assert!((lower - ZOOM_LOWER_LIMIT).abs() < 1e-6);
    }

    #[test]
    fn zoom_radius_upper_scales_with_cluster_above_floor() {
        let mut grid = VoxelGrid::default();
        grid.set(IVec3::new(10, 10, 10), Some([255, 0, 0, 255]));
        let (_, upper) = zoom_radius_limits(&grid);
        let (_, fit_radius) = fit_view(&grid).expect("non-empty grid");
        let expected = (fit_radius * ZOOM_OUT_MULTIPLIER).max(ZOOM_OUT_FLOOR);
        assert!((upper - expected).abs() < 1e-3, "upper={upper}");
    }

    #[test]
    fn zoom_radius_limits_caps_at_two_times_cluster_size() {
        // Big cluster — cap should hit 2× fit_radius, above the empty-scene floor.
        let mut grid = VoxelGrid::default();
        for x in 0..40 {
            grid.set(IVec3::new(x, 0, 0), Some([255, 0, 0, 255]));
        }
        let (_, upper) = zoom_radius_limits(&grid);
        let (_, fit_radius) = fit_view(&grid).expect("non-empty grid");
        assert!(fit_radius * ZOOM_OUT_MULTIPLIER > ZOOM_OUT_FLOOR);
        assert!((upper - fit_radius * ZOOM_OUT_MULTIPLIER).abs() < 1e-3);
    }

    #[test]
    fn zoom_radius_limits_empty_grid_uses_floor() {
        let grid = VoxelGrid::default();
        let (lower, upper) = zoom_radius_limits(&grid);
        assert!((lower - ZOOM_LOWER_LIMIT).abs() < 1e-6);
        assert!(upper > lower);
        // Empty scene: fit fallback = EMPTY_WORLD_RADIUS (32). 2× = 64, equals
        // ZOOM_OUT_FLOOR, so upper lands exactly on the floor.
        assert!((upper - ZOOM_OUT_FLOOR).abs() < 1e-3);
    }

    #[test]
    fn fit_view_single_voxel_returns_centered_radius() {
        let mut grid = VoxelGrid::default();
        grid.set(IVec3::new(10, 10, 10), Some([255, 0, 0, 255]));
        let (focus, radius) = fit_view(&grid).expect("non-empty grid");
        assert!((focus.x - 10.5).abs() < 1e-5);
        assert!((focus.y - 10.5).abs() < 1e-5);
        assert!((focus.z - 10.5).abs() < 1e-5);
        assert!((radius - 4.0).abs() < 1e-5);
    }

    #[test]
    fn flyby_yaw_advances_linearly_with_t() {
        let a = flyby_yaw(0.0, 0.0);
        let b = flyby_yaw(1.0, 0.0);
        let c = flyby_yaw(2.0, 0.0);
        assert!(b > a);
        assert!((b - a) > 0.0);
        assert!(((c - b) - (b - a)).abs() < 1e-5);
    }

    #[test]
    fn flyby_yaw_preserves_start_offset() {
        assert!((flyby_yaw(0.0, 1.234) - 1.234).abs() < 1e-6);
    }

    #[test]
    fn flyby_pitch_within_safe_bounds() {
        for i in 0..400 {
            let t = i as f32 * 0.25;
            let p = flyby_pitch(t);
            assert!(
                (FLYBY_PITCH_MIN..=FLYBY_PITCH_MAX).contains(&p),
                "t={t} p={p}"
            );
        }
    }

    #[test]
    fn flyby_pitch_average_near_mid() {
        // Sample one full period and confirm the mean lands near the midpoint.
        let period = 1.0 / FLYBY_PITCH_FREQ;
        let n = 1024usize;
        let mut sum = 0.0_f64;
        for i in 0..n {
            let t = (i as f32 / n as f32) * period;
            sum += flyby_pitch(t) as f64;
        }
        let mean = (sum / n as f64) as f32;
        assert!((mean - FLYBY_PITCH_MID).abs() < 0.01, "mean={mean}");
    }

    #[test]
    fn flyby_radius_breath_bounded() {
        let base = 32.0;
        for i in 0..400 {
            let t = i as f32 * 0.25;
            let r = flyby_radius(t, base);
            assert!(
                r >= base * (1.0 - FLYBY_RADIUS_AMP) - 1e-4
                    && r <= base * (1.0 + FLYBY_RADIUS_AMP) + 1e-4,
                "t={t} r={r}"
            );
        }
    }

    #[test]
    fn flyby_radius_starts_at_peak() {
        // Phase chosen so sin = 1 at t=0 → radius starts at base*(1+amp).
        // This means engagement reads base from fit_view and then immediately
        // pulls slightly outward, never inward past the lower zoom limit.
        let base = 32.0;
        let r = flyby_radius(0.0, base);
        let expected = base * (1.0 + FLYBY_RADIUS_AMP);
        assert!((r - expected).abs() < 1e-4, "r={r} expected={expected}");
    }

    #[test]
    fn flyby_state_default_inactive() {
        let s = FlybyState::default();
        assert!(!s.active);
        assert_eq!(s.t, 0.0);
    }

    #[test]
    fn preset_front_looks_down_negative_z() {
        let dir = preset_direction(CameraPreset::Front);
        assert!((dir - Vec3::Z).length() < 1e-5, "dir={dir:?}");
    }

    #[test]
    fn preset_top_looks_straight_down() {
        // Top preset positions the camera above focus → direction.y ≈ 1.
        // x/z stay near zero within the pole-epsilon (cos(pitch) ≈ 0.05).
        let dir = preset_direction(CameraPreset::Top);
        assert!(dir.y > 0.99, "y={}", dir.y);
        assert!(dir.x.abs() < 0.06 && dir.z.abs() < 0.06, "dir={dir:?}");
    }

    #[test]
    fn preset_iso_matches_spawn_direction() {
        let dir = preset_direction(CameraPreset::Iso);
        let expected = Vec3::ONE.normalize();
        assert!((dir - expected).length() < 1e-5, "dir={dir:?}");
    }

    #[test]
    fn preset_back_is_front_yaw_plus_pi() {
        let (yaw_front, _) = preset_angles(CameraPreset::Front);
        let (yaw_back, _) = preset_angles(CameraPreset::Back);
        assert!((yaw_back - yaw_front - PI).abs() < 1e-6);
    }

    #[test]
    fn preset_left_is_right_yaw_negated() {
        let (yaw_right, _) = preset_angles(CameraPreset::Right);
        let (yaw_left, _) = preset_angles(CameraPreset::Left);
        assert!((yaw_left + yaw_right).abs() < 1e-6);
    }

    #[test]
    fn preset_right_looks_down_negative_x() {
        let dir = preset_direction(CameraPreset::Right);
        assert!((dir - Vec3::X).length() < 1e-5, "dir={dir:?}");
    }

    #[test]
    fn preset_back_looks_down_positive_z() {
        let dir = preset_direction(CameraPreset::Back);
        assert!((dir - (-Vec3::Z)).length() < 1e-5, "dir={dir:?}");
    }

    #[test]
    fn pending_view_preset_default_empty() {
        assert!(PendingViewPreset::default().0.is_none());
    }

    #[test]
    fn fit_view_handles_negative_coords() {
        let mut grid = VoxelGrid::default();
        grid.set(IVec3::new(-10, 0, -5), Some([1, 1, 1, 255]));
        grid.set(IVec3::new(20, 5, 25), Some([1, 1, 1, 255]));
        let (focus, _radius) = fit_view(&grid).expect("non-empty grid");
        // Centroid is the geometric center of the AABB, including the +1
        // voxel extent.
        assert!((focus.x - 5.5).abs() < 1e-4, "x={}", focus.x);
        assert!((focus.z - 10.5).abs() < 1e-4, "z={}", focus.z);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CameraPreset {
    Front,
    Back,
    Right,
    Left,
    Top,
    Iso,
}

impl CameraPreset {
    pub fn label(self) -> &'static str {
        match self {
            CameraPreset::Front => "Front",
            CameraPreset::Back => "Back",
            CameraPreset::Right => "Right",
            CameraPreset::Left => "Left",
            CameraPreset::Top => "Top",
            CameraPreset::Iso => "Isometric",
        }
    }
}

/// Pitch ceiling for Top preset — must stay just under the crate's
/// `pitch_upper_limit` (set in `spawn_camera`) to avoid the gimbal-lock pole.
pub const TOP_PRESET_PITCH: f32 = FRAC_PI_2 - 0.05;

/// (yaw, pitch) targets for each preset. Convention matches
/// `bevy_panorbit_camera`: offset from focus =
/// `(sin(yaw)·cos(pitch), sin(pitch), cos(yaw)·cos(pitch)) · radius`.
pub fn preset_angles(preset: CameraPreset) -> (f32, f32) {
    match preset {
        CameraPreset::Front => (0.0, 0.0),
        CameraPreset::Back => (PI, 0.0),
        CameraPreset::Right => (FRAC_PI_2, 0.0),
        CameraPreset::Left => (-FRAC_PI_2, 0.0),
        CameraPreset::Top => (0.0, TOP_PRESET_PITCH),
        CameraPreset::Iso => (FRAC_PI_4, (1.0_f32 / 3.0_f32.sqrt()).asin()),
    }
}

/// Test-only helper: converts (yaw, pitch) into a unit direction vector
/// from focus to camera position. Runtime callers write `target_yaw` /
/// `target_pitch` directly and let `bevy_panorbit_camera` recompute position.
#[cfg(test)]
pub fn preset_direction(preset: CameraPreset) -> Vec3 {
    let (yaw, pitch) = preset_angles(preset);
    Vec3::new(
        yaw.sin() * pitch.cos(),
        pitch.sin(),
        yaw.cos() * pitch.cos(),
    )
}

/// One-shot view-preset request. Written by the keybind system and the
/// command palette / native menu; consumed by `apply_pending_view_preset_system`.
#[derive(Resource, Default)]
pub struct PendingViewPreset(pub Option<CameraPreset>);

/// One-shot Frame-View request. Written by the native menu click handler
/// (the keyboard accelerator already calls `frame_view_system` directly).
#[derive(Resource, Default)]
pub struct PendingFrameView(pub bool);

pub fn apply_pending_view_preset_system(
    mut pending: ResMut<PendingViewPreset>,
    mut flyby: ResMut<FlybyState>,
    mut cameras: Query<&mut PanOrbitCamera>,
    grid: Res<VoxelGrid>,
) {
    let Some(preset) = pending.0.take() else {
        return;
    };
    flyby.active = false;

    let (centroid, radius) =
        fit_view(&grid).unwrap_or((default_camera_focus(), EMPTY_WORLD_RADIUS));
    let (yaw, pitch) = preset_angles(preset);

    for mut cam in &mut cameras {
        cam.target_focus = centroid;
        cam.target_radius = radius;
        cam.target_yaw = yaw;
        cam.target_pitch = pitch;
    }
}

pub fn camera_preset_keys_system(
    keys: Res<ButtonInput<KeyCode>>,
    mut contexts: bevy_egui::EguiContexts,
    mut pending: ResMut<PendingViewPreset>,
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
    let shift = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);

    let preset = if keys.just_pressed(KeyCode::Digit1) || keys.just_pressed(KeyCode::Numpad1) {
        Some(if shift {
            CameraPreset::Back
        } else {
            CameraPreset::Front
        })
    } else if keys.just_pressed(KeyCode::Digit3) || keys.just_pressed(KeyCode::Numpad3) {
        Some(if shift {
            CameraPreset::Left
        } else {
            CameraPreset::Right
        })
    } else if keys.just_pressed(KeyCode::Digit5) || keys.just_pressed(KeyCode::Numpad5) {
        Some(CameraPreset::Iso)
    } else if keys.just_pressed(KeyCode::Digit7) || keys.just_pressed(KeyCode::Numpad7) {
        Some(CameraPreset::Top)
    } else {
        None
    };

    if let Some(p) = preset {
        pending.0 = Some(p);
    }
}

/// Egui-points rects describing the canvas. `avail` is the area not covered
/// by side/top/bottom panels; `screen` is the full window in the same units.
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
    cam_query: Query<(&Camera, &GlobalTransform), With<PanOrbitCamera>>,
    windows: Query<&Window, With<bevy::window::PrimaryWindow>>,
    grid: Res<crate::grid::VoxelGrid>,
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
    let factor = if alt { ZOOM_STEP_OUT } else { ZOOM_STEP_IN };

    // Recenter focus to whatever's under the cursor so the zoom converges
    // on the user's point of interest rather than orbiting around an old
    // anchor point.
    let new_focus = (|| -> Option<Vec3> {
        let (origin, dir) = crate::picking::cursor_ray(&cam_query, &windows)?;
        let hit = crate::picking::pick(&grid, origin, dir)?;
        Some(hit.cell.as_vec3() + Vec3::splat(0.5))
    })();

    for mut cam in &mut cameras {
        if let Some(focus) = new_focus {
            cam.target_focus = focus;
        }
        cam.target_radius = apply_zoom(
            cam.target_radius,
            factor,
            cam.zoom_lower_limit,
            cam.zoom_upper_limit,
        );
    }
}

/// Click/keyboard zoom step. √2 multiplier per click — gentler than the
/// previous ×2 doubling so large scenes feel less jarring while still
/// covering useful range in a few clicks.
pub const ZOOM_STEP_IN: f32 = std::f32::consts::FRAC_1_SQRT_2;
pub const ZOOM_STEP_OUT: f32 = std::f32::consts::SQRT_2;

/// Apply a zoom factor to `radius`, clamped to `[lower_limit, upper_limit]`.
pub fn apply_zoom(radius: f32, factor: f32, lower_limit: f32, upper_limit: Option<f32>) -> f32 {
    let r = (radius * factor).max(lower_limit);
    match upper_limit {
        Some(u) => r.min(u),
        None => r,
    }
}

/// Dynamic zoom radius limits derived from the current grid. Lower bound
/// is fixed; upper bound scales with `fit_view` clamped to a floor so empty
/// scenes still feel orbit-able.
pub fn zoom_radius_limits(grid: &VoxelGrid) -> (f32, f32) {
    let fit = fit_view(grid).map(|(_, r)| r).unwrap_or(EMPTY_WORLD_RADIUS);
    // Fixed lower bound so the user can always zoom in to individual voxels
    // regardless of scene size. Scaling this with cluster fit_radius made
    // large scenes feel locked out of close-up inspection.
    let lower = ZOOM_LOWER_LIMIT;
    // Upper cap = 2× fit_radius with a fixed floor so empty/tiny scenes
    // still allow comfortable orbit-out, but big scenes don't let you fly
    // off into a void of empty space.
    let upper = (fit * ZOOM_OUT_MULTIPLIER).max(ZOOM_OUT_FLOOR);
    (lower, upper)
}

/// Smallest orbit radius the user can zoom to. 8 voxels = camera sits close
/// enough to inspect a single voxel face without snapping inside it.
pub const ZOOM_LOWER_LIMIT: f32 = 8.0;

/// How far past `fit_view` radius the user can orbit out. 2× keeps the
/// cluster filling most of the view; further out felt like infinite empty
/// space and made it easy to lose the model.
pub const ZOOM_OUT_MULTIPLIER: f32 = 2.0;

/// Floor for the upper zoom cap when the cluster is tiny / empty. Keeps
/// the spawn-camera radius reachable on a fresh project.
pub const ZOOM_OUT_FLOOR: f32 = 64.0;

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
    let factor = if zoom_in { ZOOM_STEP_IN } else { ZOOM_STEP_OUT };
    for mut cam in &mut cameras {
        cam.target_radius = apply_zoom(
            cam.target_radius,
            factor,
            cam.zoom_lower_limit,
            cam.zoom_upper_limit,
        );
    }
}
