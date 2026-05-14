use bevy::camera::Viewport;
use bevy::camera::visibility::RenderLayers;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_egui::EguiContexts;
use bevy_panorbit_camera::PanOrbitCamera;
use std::f32::consts::FRAC_PI_2;

const GIZMO_LAYER: usize = 1;
const GIZMO_SIZE_PT: f32 = 100.0;
const GIZMO_MARGIN: f32 = 12.0;
const DRAG_SENSITIVITY: f32 = 0.012;
const FACE_HALF: f32 = 0.55;

#[derive(Default, Reflect, GizmoConfigGroup)]
pub struct AxisGizmoGroup;

#[derive(Component)]
pub struct GizmoCamera;

#[derive(Component)]
pub struct GizmoFace {
    pub index: usize,
    pub base: Color,
}

#[derive(Resource, Default)]
pub struct GizmoRect(pub Option<bevy::math::Rect>);

#[derive(Resource, Default)]
pub struct GizmoDrag {
    pub active: bool,
    last_cursor: Vec2,
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

    let size = FACE_HALF * 2.0;
    let quad = meshes.add(Rectangle::new(size, size));

    // Blender convention: X=red, Y=green, Z=blue. Negative axes dimmed.
    let red = Color::srgb(0.92, 0.30, 0.34);
    let red_dim = Color::srgb(0.55, 0.22, 0.24);
    let green = Color::srgb(0.40, 0.84, 0.42);
    let green_dim = Color::srgb(0.25, 0.50, 0.27);
    let blue = Color::srgb(0.36, 0.58, 1.00);
    let blue_dim = Color::srgb(0.22, 0.36, 0.62);

    let faces: [(Vec3, Quat, Color); 6] = [
        // +X
        (
            Vec3::X * FACE_HALF,
            Quat::from_axis_angle(Vec3::Y, FRAC_PI_2),
            red,
        ),
        // -X
        (
            -Vec3::X * FACE_HALF,
            Quat::from_axis_angle(Vec3::Y, -FRAC_PI_2),
            red_dim,
        ),
        // +Y
        (
            Vec3::Y * FACE_HALF,
            Quat::from_axis_angle(Vec3::X, -FRAC_PI_2),
            green,
        ),
        // -Y
        (
            -Vec3::Y * FACE_HALF,
            Quat::from_axis_angle(Vec3::X, FRAC_PI_2),
            green_dim,
        ),
        // +Z
        (Vec3::Z * FACE_HALF, Quat::IDENTITY, blue),
        // -Z
        (
            -Vec3::Z * FACE_HALF,
            Quat::from_axis_angle(Vec3::Y, std::f32::consts::PI),
            blue_dim,
        ),
    ];

    for (idx, (pos, rot, color)) in faces.into_iter().enumerate() {
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

pub fn configure_axis_gizmo(mut store: ResMut<GizmoConfigStore>) {
    let (config, _) = store.config_mut::<AxisGizmoGroup>();
    config.render_layers = RenderLayers::layer(GIZMO_LAYER);
    config.line.width = 2.0;
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
) -> Result {
    let ctx = contexts.ctx_mut()?;
    let Ok(window) = windows.single() else {
        return Ok(());
    };
    let Ok(mut cam) = cameras.single_mut() else {
        return Ok(());
    };

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

    rect_res.0 = Some(bevy::math::Rect {
        min: Vec2::new(x_pt, y_pt),
        max: Vec2::new(x_pt + GIZMO_SIZE_PT, y_pt + GIZMO_SIZE_PT),
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

pub fn gizmo_drag_system(
    mouse: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    gizmo_rect: Res<GizmoRect>,
    mut drag: ResMut<GizmoDrag>,
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

    if mouse.just_released(MouseButton::Left) {
        drag.active = false;
    }

    if mouse.just_pressed(MouseButton::Left)
        && let Some(rect) = gizmo_rect.0
        && rect.contains(cursor)
    {
        drag.active = true;
        drag.last_cursor = cursor;
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
