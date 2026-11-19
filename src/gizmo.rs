use bevy::camera::Viewport;
use bevy::camera::visibility::RenderLayers;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_egui::EguiContexts;
use bevy_panorbit_camera::PanOrbitCamera;
use std::f32::consts::{FRAC_PI_2, PI, TAU};

use crate::camera::{CameraPreset, TOP_PRESET_PITCH, preset_angles};
use crate::ui::UiVisible;

const GIZMO_LAYER: usize = 1;
const GIZMO_SIZE_PT: f32 = 100.0;
const GIZMO_MARGIN: f32 = 12.0;
const DRAG_SENSITIVITY: f32 = 0.012;
const FACE_HALF: f32 = 0.55;
const CLICK_SLOP: f32 = 4.0;

#[derive(Default, Reflect, GizmoConfigGroup)]
pub struct AxisGizmoGroup;

#[derive(Component)]
pub struct GizmoCamera;

#[derive(Component)]
pub struct GizmoFace {
    pub index: usize,
    pub base: Color,
}

/// The orientation cube's screen rect in logical window pixels (the space of
/// `Window::cursor_position`), not egui points, which differ once the UI
/// scale isn't 100%.
#[derive(Resource, Default)]
pub struct GizmoRect(pub Option<bevy::math::Rect>);

impl GizmoRect {
    pub fn contains_cursor(&self, windows: &Query<&Window, With<PrimaryWindow>>) -> bool {
        self.0
            .zip(windows.single().ok().and_then(Window::cursor_position))
            .is_some_and(|(rect, c)| rect.contains(c))
    }
}

#[derive(Resource, Default)]
pub struct GizmoDrag {
    pub active: bool,
    last_cursor: Vec2,
    press_cursor: Vec2,
}

#[derive(Resource, Default)]
pub struct GizmoHover {
    pub hovered_index: Option<usize>,
}

pub fn spawn_gizmo(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
) {
    commands.spawn((
        Camera3d::default(),
        Camera {
            order: 1,
            clear_color: ClearColorConfig::None,
            viewport: Some(Viewport {
                physical_position: UVec2::ZERO,
                physical_size: UVec2::splat(GIZMO_SIZE_PT as u32),
                depth: 0.0..1.0,
            }),
            ..default()
        },
        Projection::Orthographic(OrthographicProjection {
            scaling_mode: bevy::camera::ScalingMode::FixedVertical {
                viewport_height: 2.0,
            },
            ..OrthographicProjection::default_3d()
        }),
        Transform::from_translation(Vec3::Z * 5.0).looking_at(Vec3::ZERO, Vec3::Y),
        RenderLayers::layer(GIZMO_LAYER),
        GizmoCamera,
    ));

    spawn_etched_faces(commands, meshes, materials);
}

fn axis_colors() -> (Color, Color, Color, Color, Color, Color) {
    // X=red, Y=green, Z=blue. Negative axes dimmed.
    (
        Color::srgb(0.92, 0.30, 0.34),
        Color::srgb(0.55, 0.22, 0.24),
        Color::srgb(0.40, 0.84, 0.42),
        Color::srgb(0.25, 0.50, 0.27),
        Color::srgb(0.36, 0.58, 1.00),
        Color::srgb(0.22, 0.36, 0.62),
    )
}

fn cube_face_layout() -> [(Vec3, Quat); 6] {
    [
        (
            Vec3::X * FACE_HALF,
            Quat::from_axis_angle(Vec3::Y, FRAC_PI_2),
        ),
        (
            -Vec3::X * FACE_HALF,
            Quat::from_axis_angle(Vec3::Y, -FRAC_PI_2),
        ),
        (
            Vec3::Y * FACE_HALF,
            Quat::from_axis_angle(Vec3::X, -FRAC_PI_2),
        ),
        (
            -Vec3::Y * FACE_HALF,
            Quat::from_axis_angle(Vec3::X, FRAC_PI_2),
        ),
        (Vec3::Z * FACE_HALF, Quat::IDENTITY),
        (
            -Vec3::Z * FACE_HALF,
            Quat::from_axis_angle(Vec3::Y, std::f32::consts::PI),
        ),
    ]
}

fn face_color_for_index(idx: usize) -> Color {
    let (red, red_dim, green, green_dim, blue, blue_dim) = axis_colors();
    [red, red_dim, green, green_dim, blue, blue_dim][idx]
}

fn mute(c: Color, mix: f32) -> Color {
    // Pull each channel toward neutral grey by `mix`. Modern UIs prefer
    // restrained palettes — saturated primaries read crayon-y.
    let l = c.to_linear();
    let g = 0.30;
    Color::linear_rgba(
        l.red * (1.0 - mix) + g * mix,
        l.green * (1.0 - mix) + g * mix,
        l.blue * (1.0 - mix) + g * mix,
        l.alpha,
    )
}

/// Etched cube: muted axis colors on each face, canvas-colored "grout" lines
/// drawn between faces via gizmos. The grout tracks light/dark mode so the
/// gaps always match the viewport bg, making the cube read as a set of
/// floating tiles.
fn spawn_etched_faces(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
) {
    let size = FACE_HALF * 2.0;
    let quad = meshes.add(Rectangle::new(size, size));
    for (idx, (pos, rot)) in cube_face_layout().into_iter().enumerate() {
        let color = mute(face_color_for_index(idx), 0.45);
        let mat = materials.add(StandardMaterial {
            base_color: color,
            unlit: true,
            ..default()
        });
        commands.spawn((
            Mesh3d(quad.clone()),
            MeshMaterial3d(mat),
            Transform {
                translation: pos,
                rotation: rot,
                scale: Vec3::ONE,
            },
            RenderLayers::layer(GIZMO_LAYER),
            GizmoFace {
                index: idx,
                base: color,
            },
        ));
    }
}

/// Per-frame canvas-colored grout strokes along all 12 cube edges. Reads
/// light/dark mode from `Preferences` + `Theme` so the gaps stay matched to
/// the viewport bg even when the user toggles theme.
pub fn draw_gizmo_decorations_system(
    prefs: Res<crate::theme::Preferences>,
    theme: Res<crate::theme::Theme>,
    mut gizmos: Gizmos<AxisGizmoGroup>,
) {
    let h = FACE_HALF;
    let canvas_rgb = crate::theme::resolve_canvas_color(&prefs, &theme);
    let canvas = Color::srgb(
        canvas_rgb[0] as f32 / 255.0,
        canvas_rgb[1] as f32 / 255.0,
        canvas_rgb[2] as f32 / 255.0,
    );
    let corners = [
        Vec3::new(-h, -h, -h),
        Vec3::new(h, -h, -h),
        Vec3::new(h, h, -h),
        Vec3::new(-h, h, -h),
        Vec3::new(-h, -h, h),
        Vec3::new(h, -h, h),
        Vec3::new(h, h, h),
        Vec3::new(-h, h, h),
    ];
    let pairs = [
        (0, 1),
        (1, 2),
        (2, 3),
        (3, 0),
        (4, 5),
        (5, 6),
        (6, 7),
        (7, 4),
        (0, 4),
        (1, 5),
        (2, 6),
        (3, 7),
    ];
    for (a, b) in pairs {
        gizmos.line(corners[a], corners[b], canvas);
    }
}

pub fn configure_axis_gizmo(mut store: ResMut<GizmoConfigStore>) {
    let (config, _) = store.config_mut::<AxisGizmoGroup>();
    config.render_layers = RenderLayers::layer(GIZMO_LAYER);
    // Etched / Wireframe / Subdivided draw their edges as gizmo lines on top
    // of the cube faces. 3 px reads as a deliberate gap between tiles at the
    // gizmo's 100 px viewport size, not a hairline.
    config.line.width = 3.0;
    config.depth_bias = -1.0;
}

pub fn sync_gizmo_camera(
    primary: Query<&Transform, (With<PanOrbitCamera>, Without<GizmoCamera>)>,
    mut gizmo_cam: Query<&mut Transform, With<GizmoCamera>>,
) {
    let Ok(p) = primary.single() else {
        return;
    };
    let Ok(mut t) = gizmo_cam.single_mut() else {
        return;
    };
    t.rotation = p.rotation;
    t.translation = t.rotation * Vec3::Z * 5.0;
}

pub fn update_gizmo_viewport(
    mut contexts: EguiContexts,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut cameras: Query<&mut Camera, With<GizmoCamera>>,
    mut rect_res: ResMut<GizmoRect>,
    ui_visible: Res<UiVisible>,
    modal_active: Res<crate::ui::ModalActive>,
) -> Result {
    let ctx = contexts.ctx_mut()?;
    let Ok(window) = windows.single() else {
        return Ok(());
    };
    let Ok(mut cam) = cameras.single_mut() else {
        return Ok(());
    };

    // Hide the gizmo in focus mode — it's chrome, so it goes with the rest.
    // During flyby (+ visible UI) keep it visible so the user can still
    // orient themselves against the cube. Also hide it behind an open modal:
    // its camera composites after egui, so the modal scrim can't cover it.
    if !ui_visible.0 || modal_active.0 {
        cam.is_active = false;
        rect_res.0 = None;
        return Ok(());
    }
    cam.is_active = true;

    let rect = ctx.available_rect();
    let ppp = ctx.pixels_per_point();

    let x_pt = (rect.right() - GIZMO_SIZE_PT - GIZMO_MARGIN).max(0.0);
    let y_pt = (rect.top() + GIZMO_MARGIN).max(0.0);

    let size_phys = (GIZMO_SIZE_PT * ppp).round() as u32;
    let mut x = (x_pt * ppp).round() as u32;
    let mut y = (y_pt * ppp).round() as u32;

    let w = window.physical_width();
    let h = window.physical_height();
    if x + size_phys > w {
        x = w.saturating_sub(size_phys);
    }
    if y + size_phys > h {
        y = h.saturating_sub(size_phys);
    }

    cam.viewport = Some(Viewport {
        physical_position: UVec2::new(x, y),
        physical_size: UVec2::splat(size_phys.min(w).min(h)),
        depth: 0.0..1.0,
    });

    let to_logical = 1.0 / window.scale_factor();
    let min = UVec2::new(x, y).as_vec2() * to_logical;
    rect_res.0 = Some(bevy::math::Rect {
        min,
        max: min + Vec2::splat(size_phys as f32 * to_logical),
    });
    Ok(())
}

pub fn update_gizmo_hover(
    windows: Query<&Window, With<PrimaryWindow>>,
    rect_res: Res<GizmoRect>,
    gizmo_cam: Query<(&Camera, &GlobalTransform), With<GizmoCamera>>,
    mut hover: ResMut<GizmoHover>,
    faces: Query<(&GizmoFace, &MeshMaterial3d<StandardMaterial>)>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let picked: Option<usize> = (|| {
        let window = windows.single().ok()?;
        let cursor = window.cursor_position()?;
        let rect = rect_res.0?;
        if !rect.contains(cursor) {
            return None;
        }
        let (camera, tf) = gizmo_cam.single().ok()?;
        let ray = camera.viewport_to_world(tf, cursor).ok()?;
        pick_face(ray.origin, *ray.direction)
    })();

    if picked == hover.hovered_index {
        return;
    }
    hover.hovered_index = picked;

    for (face, handle) in faces.iter() {
        if let Some(mat) = materials.get_mut(&handle.0) {
            mat.base_color = if Some(face.index) == picked {
                brighten(face.base, 0.35)
            } else {
                face.base
            };
        }
    }
}

fn pick_face(origin: Vec3, dir: Vec3) -> Option<usize> {
    let h = FACE_HALF;
    // (index, normal direction, plane signed distance)
    let candidates: [(usize, Vec3); 6] = [
        (0, Vec3::X),
        (1, -Vec3::X),
        (2, Vec3::Y),
        (3, -Vec3::Y),
        (4, Vec3::Z),
        (5, -Vec3::Z),
    ];
    let mut best: Option<(f32, usize)> = None;
    for (idx, normal) in candidates {
        let denom = dir.dot(normal);
        // Only front-facing planes (ray heading into the face from outside).
        if denom >= -1e-4 {
            continue;
        }
        let t = (h - origin.dot(normal)) / denom;
        if t <= 0.0 {
            continue;
        }
        let p = origin + dir * t;
        let in_bounds = if normal.x.abs() > 0.5 {
            p.y.abs() <= h && p.z.abs() <= h
        } else if normal.y.abs() > 0.5 {
            p.x.abs() <= h && p.z.abs() <= h
        } else {
            p.x.abs() <= h && p.y.abs() <= h
        };
        if !in_bounds {
            continue;
        }
        if best.is_none_or(|(bt, _)| t < bt) {
            best = Some((t, idx));
        }
    }
    best.map(|(_, i)| i)
}

fn brighten(c: Color, amount: f32) -> Color {
    let l = c.to_linear();
    let mix = |a: f32| a + (1.0 - a) * amount;
    Color::linear_rgba(mix(l.red), mix(l.green), mix(l.blue), l.alpha)
}

fn face_angles(index: usize) -> Option<(f32, f32)> {
    let preset = match index {
        0 => CameraPreset::Right,
        1 => CameraPreset::Left,
        2 => CameraPreset::Top,
        3 => return Some((0.0, -TOP_PRESET_PITCH)),
        4 => CameraPreset::Front,
        5 => CameraPreset::Back,
        _ => return None,
    };
    Some(preset_angles(preset))
}

fn nearest_turn(current: f32, target: f32) -> f32 {
    current + (target - current + PI).rem_euclid(TAU) - PI
}

pub fn gizmo_drag_system(
    mouse: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    gizmo_rect: Res<GizmoRect>,
    hover: Res<GizmoHover>,
    mut drag: ResMut<GizmoDrag>,
    mut flyby: ResMut<crate::camera::FlybyState>,
    mut cameras: Query<&mut PanOrbitCamera>,
) {
    let Ok(window) = windows.single() else {
        return;
    };
    let Some(cursor) = window.cursor_position() else {
        if mouse.just_released(MouseButton::Left) {
            drag.active = false;
        }
        return;
    };

    if mouse.just_released(MouseButton::Left) && drag.active {
        drag.active = false;
        if cursor.distance(drag.press_cursor) < CLICK_SLOP
            && let Some((yaw, pitch)) = hover.hovered_index.and_then(face_angles)
            && let Ok(mut cam) = cameras.single_mut()
        {
            flyby.active = false;
            cam.target_yaw = nearest_turn(cam.target_yaw, yaw);
            cam.target_pitch = pitch;
        }
        return;
    }

    if mouse.just_pressed(MouseButton::Left)
        && let Some(rect) = gizmo_rect.0
        && rect.contains(cursor)
    {
        drag.active = true;
        drag.last_cursor = cursor;
        drag.press_cursor = cursor;
    }

    if !drag.active || !mouse.pressed(MouseButton::Left) {
        return;
    }

    let delta = cursor - drag.last_cursor;
    drag.last_cursor = cursor;
    if delta == Vec2::ZERO {
        return;
    }
    if let Ok(mut cam) = cameras.single_mut() {
        cam.target_yaw -= delta.x * DRAG_SENSITIVITY;
        cam.target_pitch += delta.y * DRAG_SENSITIVITY;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn offset(yaw: f32, pitch: f32) -> Vec3 {
        Vec3::new(
            yaw.sin() * pitch.cos(),
            pitch.sin(),
            yaw.cos() * pitch.cos(),
        )
    }

    #[test]
    fn each_face_puts_the_camera_on_that_side() {
        let normals = [Vec3::X, -Vec3::X, Vec3::Y, -Vec3::Y, Vec3::Z, -Vec3::Z];
        for (i, n) in normals.into_iter().enumerate() {
            let (yaw, pitch) = face_angles(i).expect("face");
            assert!(offset(yaw, pitch).dot(n) > 0.99, "face {i}");
        }
        assert_eq!(face_angles(6), None);
    }

    #[test]
    fn nearest_turn_takes_the_short_way_round() {
        assert!((nearest_turn(0.0, FRAC_PI_2) - FRAC_PI_2).abs() < 1e-5);
        assert!((nearest_turn(4.0 * TAU, FRAC_PI_2) - (4.0 * TAU + FRAC_PI_2)).abs() < 1e-4);
        assert!((nearest_turn(PI - 0.1, -PI + 0.1) - (PI + 0.1)).abs() < 1e-5);
    }
}
