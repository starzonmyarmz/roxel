use bevy::ecs::system::SystemParam;
use bevy::gizmos::config::{GizmoConfigGroup, GizmoConfigStore};
use bevy::prelude::*;
use bevy::reflect::Reflect;
use bevy::window::PrimaryWindow;
use bevy_egui::EguiContexts;
use bevy_panorbit_camera::PanOrbitCamera;

use crate::grid::VoxelGrid;
use crate::mesh::PreviewHide;
use crate::picking::{cursor_ray, pick};
use crate::theme::Theme;
use crate::tools::{CurrentColor, PointerState, Tool, ToolState};

/// Dedicated gizmo group for tool preview outlines (brush ghost, erase/paint
/// target highlight, shape silhouette). Uses `depth_bias = -1.0` so the outline
/// always reads on top of adjacent voxel geometry — without this, edges flush
/// against a neighboring voxel's face get z-occluded and the affordance
/// disappears.
#[derive(Default, Reflect, GizmoConfigGroup)]
pub struct PreviewGizmos;

pub fn configure_preview_gizmos(mut store: ResMut<GizmoConfigStore>) {
    let (config, _) = store.config_mut::<PreviewGizmos>();
    config.depth_bias = -1.0;
    config.line.width = 2.5;
    config.line.perspective = false;
}

pub fn accent_outline_color(theme: &Theme) -> Color {
    let c = theme.accent;
    Color::srgba(
        c.r() as f32 / 255.0,
        c.g() as f32 / 255.0,
        c.b() as f32 / 255.0,
        1.0,
    )
}

#[derive(SystemParam)]
pub struct StrokeGates<'w> {
    pub pointer: Res<'w, PointerState>,
    pub shape: Res<'w, crate::tools::ShapeState>,
    pub theme: Res<'w, Theme>,
}

#[derive(Component)]
pub struct BrushPreview;

#[derive(Resource)]
pub struct BrushPreviewMaterial(pub Handle<StandardMaterial>);

pub fn spawn_brush_preview(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
) {
    let mesh = meshes.add(Mesh::from(bevy::math::primitives::Cuboid::new(
        1.0, 1.0, 1.0,
    )));
    let mat = materials.add(StandardMaterial {
        base_color: Color::srgba(1.0, 1.0, 1.0, 0.45),
        unlit: true,
        alpha_mode: AlphaMode::Blend,
        cull_mode: None,
        ..default()
    });
    commands.spawn((
        Mesh3d(mesh),
        MeshMaterial3d(mat.clone()),
        Transform::default(),
        Visibility::Hidden,
        BrushPreview,
    ));
    commands.insert_resource(BrushPreviewMaterial(mat));
}

pub fn brush_preview_system(
    mut contexts: EguiContexts,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    cameras: Query<(&Camera, &GlobalTransform), With<PanOrbitCamera>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    grid: Res<VoxelGrid>,
    tool: Res<ToolState>,
    color: Res<CurrentColor>,
    gates: StrokeGates,
    mat_handle: Res<BrushPreviewMaterial>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut hide: ResMut<PreviewHide>,
    mut q: Query<(&mut Transform, &mut Visibility), With<BrushPreview>>,
    gizmo_view: crate::ui::GizmoView,
    flyby: Res<crate::camera::FlybyState>,
    mut gizmos: Gizmos<PreviewGizmos>,
) {
    let theme = *gates.theme;
    let Ok((mut tf, mut vis)) = q.single_mut() else {
        return;
    };

    let egui_wants_pointer = contexts
        .ctx_mut()
        .map(|c| c.is_pointer_over_area())
        .unwrap_or(false);

    let clear = |vis: &mut Visibility, hide: &mut PreviewHide| {
        *vis = Visibility::Hidden;
        hide.set(None);
        hide.set_recolor(None);
    };

    if flyby.active
        || egui_wants_pointer
        || gizmo_view.drag.active
        || keys.pressed(KeyCode::Space)
        || mouse.pressed(MouseButton::Right)
        || gates.pointer.stroking
    {
        clear(&mut vis, &mut hide);
        return;
    }
    if let (Some(rect), Ok(window)) = (gizmo_view.rect.0, windows.single())
        && let Some(c) = window.cursor_position()
        && rect.contains(c)
    {
        clear(&mut vis, &mut hide);
        return;
    }

    let Some((origin, dir)) = cursor_ray(&cameras, &windows) else {
        clear(&mut vis, &mut hide);
        return;
    };
    let Some(hit) = pick(&grid, origin, dir) else {
        clear(&mut vis, &mut hide);
        return;
    };

    let show_brush_ghost = match tool.current {
        Tool::Brush => true,
        Tool::Shape => gates.shape.phase.is_none(),
        _ => false,
    };
    if show_brush_ghost {
        hide.set(None);
        hide.set_recolor(None);
        let target = hit.cell + hit.normal;
        if !grid.in_bounds(target) {
            *vis = Visibility::Hidden;
            return;
        }
        let c = color.0;
        let pos = target.as_vec3() + Vec3::splat(0.5);
        *tf = Transform::from_translation(pos);
        *vis = Visibility::Visible;
        if let Some(m) = materials.get_mut(&mat_handle.0) {
            m.base_color = Color::srgba(
                c[0] as f32 / 255.0,
                c[1] as f32 / 255.0,
                c[2] as f32 / 255.0,
                0.45,
            );
        }
        gizmos.cube(
            Transform::from_translation(pos).with_scale(Vec3::splat(1.01)),
            accent_outline_color(&theme),
        );
        return;
    }
    let emit_target_outline = |gizmos: &mut Gizmos<PreviewGizmos>, cell: IVec3| {
        let pos = cell.as_vec3() + Vec3::splat(0.5);
        gizmos.cube(
            Transform::from_translation(pos).with_scale(Vec3::splat(1.02)),
            accent_outline_color(&theme),
        );
    };
    match tool.current {
        Tool::Erase => {
            *vis = Visibility::Hidden;
            hide.set_recolor(None);
            if hit.hit_voxel {
                hide.set(Some(hit.cell));
                emit_target_outline(&mut gizmos, hit.cell);
            } else {
                hide.set(None);
            }
        }
        Tool::Paint => {
            *vis = Visibility::Hidden;
            hide.set(None);
            if hit.hit_voxel {
                hide.set_recolor(Some((hit.cell, color.0)));
                emit_target_outline(&mut gizmos, hit.cell);
            } else {
                hide.set_recolor(None);
            }
        }
        _ => {
            clear(&mut vis, &mut hide);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn srgb_components(c: Color) -> (f32, f32, f32, f32) {
        let s = c.to_srgba();
        (s.red, s.green, s.blue, s.alpha)
    }

    fn assert_matches_accent(theme: &Theme) {
        let (r, g, b, a) = srgb_components(accent_outline_color(theme));
        let expected_r = theme.accent.r() as f32 / 255.0;
        let expected_g = theme.accent.g() as f32 / 255.0;
        let expected_b = theme.accent.b() as f32 / 255.0;
        assert!((r - expected_r).abs() < 1e-4, "r {} vs {}", r, expected_r);
        assert!((g - expected_g).abs() < 1e-4, "g {} vs {}", g, expected_g);
        assert!((b - expected_b).abs() < 1e-4, "b {} vs {}", b, expected_b);
        assert!((a - 1.0).abs() < 1e-4, "alpha {} vs 1.0", a);
    }

    #[test]
    fn accent_outline_color_matches_dark_theme_accent() {
        assert_matches_accent(&Theme::dark());
    }

    #[test]
    fn accent_outline_color_matches_light_theme_accent() {
        assert_matches_accent(&Theme::light());
    }
}
