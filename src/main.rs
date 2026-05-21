mod camera;
mod clipboard;
mod color_space;
mod gizmo;
mod grid;
mod history;
mod icon;
mod io;
mod lighting;
#[cfg(target_os = "macos")]
mod menu;
mod mesh;
mod onboarding;
mod picking;
mod preview;
mod select;
mod shape_preview;
mod shapes;
mod snapshot;
mod theme;
mod tools;
mod ui;
mod updater;

use bevy::prelude::*;
use bevy_egui::EguiPlugin;
use bevy_panorbit_camera::PanOrbitCameraPlugin;
use std::collections::HashMap;

use crate::camera::{
    EMPTY_WORLD_RADIUS, FlybyState, PendingFrameView, PendingViewPreset, RecenterRequest,
    ViewportRect, apply_pending_view_preset_system, apply_recenter_system,
    camera_preset_keys_system, default_camera_focus, flyby_system, frame_view_system, spawn_camera,
    update_viewport_rect, update_zoom_limits_system, zoom_click_system, zoom_key_system,
};
use crate::gizmo::{
    AxisGizmoGroup, GizmoDrag, GizmoHover, GizmoRect, configure_axis_gizmo, gizmo_drag_system,
    spawn_gizmo, sync_gizmo_camera, update_gizmo_hover, update_gizmo_viewport,
};
use crate::grid::{
    NewProject, VoxelGrid, large_scene_threshold_crossed, large_scene_warning_cleared,
};
use crate::history::History;
use crate::lighting::spawn_lights;
use crate::mesh::{PreviewHide, VoxelChunkMeshes, regenerate_mesh_system};
use crate::onboarding::{
    Onboarding, OnboardingAnchors, onboarding_autostart_system, onboarding_overlay_system,
};
use crate::preview::{brush_preview_system, spawn_brush_preview};
use crate::shape_preview::{shape_preview_system, spawn_shape_preview};
use crate::snapshot::{
    SnapshotInProgress, SnapshotRequest, SnapshotSession, start_snapshot_system,
};
use crate::theme::{
    Preferences, PreferencesWindow, Theme, install_fonts, load_preferences, refresh_theme_system,
    resolve_canvas_color, resolve_theme,
};
use crate::tools::{
    CurrentColor, MoveDragState, PointerState, RecentColors, ShapeOptions, ShapeState, ToolState,
    alt_eyedropper_system, move_drag_system, tool_input_system, tool_shortcut_system,
    undo_redo_system,
};
use crate::ui::{
    CommandPalette, CurrentProjectPath, PaletteChoice, Palettes, PendingDialog, PendingImport,
    RecentFiles, Toasts, UiVisible, command_palette_shortcut_system,
    dispatch_command_palette_system, poll_dialogs_system, tab_toggle_system, toast_lifetime_system,
    ui_system,
};
use bevy_panorbit_camera::PanOrbitCamera;

fn main() {
    let prefs = load_preferences();
    let theme = resolve_theme(prefs.theme);
    let initial_canvas = {
        let [r, g, b] = resolve_canvas_color(&prefs, &theme);
        Color::srgb_u8(r, g, b)
    };
    let mut app = App::new();
    app.insert_resource(prefs)
        .insert_resource(theme)
        .init_resource::<PreferencesWindow>()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Roxel".into(),
                resolution: (1280u32, 800u32).into(),
                #[cfg(target_os = "macos")]
                titlebar_transparent: true,
                #[cfg(target_os = "macos")]
                titlebar_show_title: false,
                #[cfg(target_os = "macos")]
                fullsize_content_view: true,
                ..default()
            }),
            ..default()
        }))
        .add_plugins(EguiPlugin::default())
        .insert_resource(bevy_egui::EguiGlobalSettings {
            auto_create_primary_context: false,
            ..default()
        })
        .add_plugins(PanOrbitCameraPlugin)
        .insert_resource(ClearColor(initial_canvas))
        .insert_resource(bevy::light::GlobalAmbientLight {
            color: Color::WHITE,
            brightness: 350.0,
            ..default()
        })
        .init_resource::<VoxelGrid>()
        .init_resource::<History>()
        .init_resource::<ToolState>()
        .init_resource::<CurrentColor>()
        .init_resource::<crate::color_space::ColorEditBuffer>()
        .init_resource::<RecentColors>()
        .init_resource::<PointerState>()
        .init_resource::<ShapeOptions>()
        .init_resource::<ShapeState>()
        .init_resource::<MoveDragState>()
        .init_resource::<crate::select::Selection>()
        .init_resource::<crate::select::SelectState>()
        .init_resource::<crate::clipboard::Clipboard>()
        .init_resource::<PreviewHide>()
        .init_resource::<PendingDialog>()
        .init_resource::<CurrentProjectPath>()
        .insert_resource(RecentFiles::loaded())
        .init_resource::<PaletteChoice>()
        .insert_resource(Palettes::with_user_loaded())
        .init_resource::<SnapshotRequest>()
        .init_resource::<SnapshotSession>()
        .init_resource::<SnapshotInProgress>()
        .init_resource::<FlybyState>()
        .init_resource::<PendingViewPreset>()
        .init_resource::<PendingFrameView>()
        .init_resource::<GizmoRect>()
        .init_resource::<GizmoDrag>()
        .init_resource::<GizmoHover>()
        .init_resource::<ViewportRect>()
        .insert_resource(RecenterRequest {
            base_focus: Some(default_camera_focus()),
        })
        .init_resource::<NewProject>()
        .init_resource::<PendingImport>()
        .init_resource::<Toasts>()
        .init_resource::<crate::updater::UpdateCheck>()
        .init_resource::<CommandPalette>()
        .init_resource::<Onboarding>()
        .init_resource::<OnboardingAnchors>()
        .init_resource::<UiVisible>()
        .init_gizmo_group::<AxisGizmoGroup>()
        .init_gizmo_group::<OriginAxesGizmos>()
        .init_gizmo_group::<FloorDotsGizmos>()
        .init_gizmo_group::<crate::preview::PreviewGizmos>()
        .init_gizmo_group::<crate::select::SelectionGizmos>();

    #[cfg(target_os = "macos")]
    {
        app.init_resource::<crate::menu::MenuQueue>().add_systems(
            Update,
            (
                crate::menu::install_menu_system,
                crate::menu::poll_menu_events_system.after(crate::menu::install_menu_system),
                crate::menu::apply_menu_actions_system.after(crate::menu::poll_menu_events_system),
                crate::menu::update_menu_enabled_system.after(crate::menu::install_menu_system),
                crate::menu::update_recent_menu_system.after(crate::menu::install_menu_system),
            ),
        );
    }

    app.add_systems(
        Startup,
        (
            setup_scene,
            configure_axis_gizmo,
            configure_origin_axes_gizmos,
            configure_floor_dots_gizmos,
            crate::preview::configure_preview_gizmos,
            crate::select::configure_selection_gizmos,
        ),
    )
    .add_systems(
        Update,
        (
            alt_eyedropper_system,
            tool_input_system,
            move_drag_system,
            tool_shortcut_system,
            undo_redo_system,
            regenerate_mesh_system,
            poll_dialogs_system,
            sync_gizmo_camera,
            gizmo_drag_system,
            update_gizmo_hover,
            frame_view_system,
            brush_preview_system.before(regenerate_mesh_system),
            shape_preview_system.before(regenerate_mesh_system),
            crate::select::selection_render_system.before(regenerate_mesh_system),
            (
                crate::select::selection_key_action_system,
                crate::select::move_selection_keys_system,
                crate::clipboard::clipboard_key_system,
            ),
            start_snapshot_system
                .before(floor_dots_system)
                .before(draw_origin_system)
                .before(crate::select::selection_render_system),
            apply_new_project_system.before(regenerate_mesh_system),
            apply_import_system,
            toast_lifetime_system,
        ),
    )
    .add_systems(
        Update,
        (
            crate::icon::set_window_icon,
            refresh_theme_system,
            zoom_click_system,
            zoom_key_system,
            apply_canvas_bg_system,
            floor_dots_system,
            draw_origin_system,
            perf_warn_system,
            command_palette_shortcut_system,
            dispatch_command_palette_system,
            apply_recenter_system,
            update_zoom_limits_system,
            flyby_system,
            camera_preset_keys_system,
            apply_pending_view_preset_system.after(camera_preset_keys_system),
            crate::updater::startup_check_system,
            crate::updater::poll_update_check_system,
            onboarding_autostart_system,
        ),
    )
    .add_systems(
        PreUpdate,
        font_setup
            .after(bevy_egui::EguiPreUpdateSet::InitContexts)
            .before(bevy_egui::EguiPreUpdateSet::BeginPass),
    )
    .add_systems(Update, tab_toggle_system)
    .add_systems(
        bevy_egui::EguiPrimaryContextPass,
        (
            ui_system,
            update_gizmo_viewport.after(ui_system),
            update_viewport_rect.after(ui_system),
            vignette_system.after(ui_system),
            onboarding_overlay_system.after(ui_system),
        ),
    );

    app.run();
}

fn setup_scene(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    spawn_camera(&mut commands);
    spawn_gizmo(&mut commands, &mut meshes, &mut materials);
    spawn_lights(&mut commands);
    spawn_brush_preview(&mut commands, &mut meshes, &mut materials);
    spawn_shape_preview(&mut commands, &mut meshes, &mut materials);

    // Shared chunk material — every chunk entity references this handle so we
    // don't allocate a material per chunk. The mesher spawns entities lazily
    // (as `dirty_chunks` produces new coords) and despawns when they empty.
    let mat = materials.add(StandardMaterial {
        base_color: Color::WHITE,
        unlit: true,
        ..default()
    });
    commands.insert_resource(VoxelChunkMeshes {
        chunks: HashMap::new(),
        material: mat,
    });
}

fn apply_new_project_system(
    mut commands: Commands,
    mut new_project: ResMut<NewProject>,
    mut grid: ResMut<VoxelGrid>,
    mut history: ResMut<History>,
    mut chunk_meshes: ResMut<VoxelChunkMeshes>,
    mut cameras: Query<&mut PanOrbitCamera>,
    mut recenter: ResMut<RecenterRequest>,
) {
    if !std::mem::take(&mut new_project.apply) {
        return;
    }
    grid.clear();
    history.undo.clear();
    history.redo.clear();
    history.current = None;

    // Despawn every chunk entity that was spawned for this scene; the mesher
    // will recreate them as the user paints fresh voxels.
    for (_, (entity, _)) in chunk_meshes.chunks.drain() {
        commands.entity(entity).despawn();
    }

    for mut cam in cameras.iter_mut() {
        cam.target_focus = Vec3::ZERO;
        cam.target_radius = EMPTY_WORLD_RADIUS;
    }
    recenter.base_focus = Some(Vec3::ZERO);
}

fn apply_import_system(mut pending: ResMut<PendingImport>) {
    // Open-world: imports just `grid.set` cells at their source coordinates.
    // No camera reframe — the user can press Cmd+0 to frame the imported
    // model. Consume the flag to signal we saw it.
    if pending.0 {
        pending.0 = false;
    }
}

fn apply_canvas_bg_system(
    prefs: Res<Preferences>,
    theme: Res<Theme>,
    mut clear: ResMut<ClearColor>,
) {
    let [r, g, b] = resolve_canvas_color(&prefs, &theme);
    let next = Color::srgb_u8(r, g, b);
    if clear.0 != next {
        clear.0 = next;
    }
}

/// Quadratic-falloff alpha for a floor dot at integer-cell distance `d` from
/// camera focus, with window half-extent `half`. Returns 0 outside the window.
fn floor_dot_alpha(d_sq: f32, half_sq: f32, base: f32) -> f32 {
    if half_sq <= 0.0 || d_sq >= half_sq {
        return 0.0;
    }
    let t = 1.0 - d_sq / half_sq;
    base * t * t
}

/// Dedicated gizmo group for the floor dots. A thick `line_width` paired
/// with a near-zero tick length is what makes each intersection render as a
/// round dot — Bevy gizmos only expose line primitives, so this is how we
/// fake a point sprite without a custom mesh.
#[derive(Default, Reflect, GizmoConfigGroup)]
pub struct FloorDotsGizmos;

fn configure_floor_dots_gizmos(mut store: ResMut<GizmoConfigStore>) {
    let (config, _) = store.config_mut::<FloorDotsGizmos>();
    config.line.width = 3.0;
    config.line.perspective = false;
}

/// Draws a per-voxel dot grid on the y=0 plane. Each intersection is a small
/// `+` cross of two short, thick line segments. Alpha fades quadratically
/// from the camera focus outward so the rim dissolves into the canvas. Always
/// renders at voxel spacing (1) regardless of zoom — the per-dot fade is what
/// keeps far dots from cluttering the canvas, not an LOD spacing change.
fn floor_dots_system(
    prefs: Res<Preferences>,
    theme: Res<crate::theme::Theme>,
    snapshot_active: Res<crate::snapshot::SnapshotInProgress>,
    cameras: Query<&PanOrbitCamera>,
    mut gizmos: Gizmos<FloorDotsGizmos>,
) {
    if snapshot_active.0 || !prefs.show_floor_grid {
        return;
    }
    let Ok(cam) = cameras.single() else { return };
    let radius = cam.target_radius.max(0.001);
    let lift = 0.001;
    let cx = cam.focus.x.round() as i32;
    let cz = cam.focus.z.round() as i32;

    // Cross ticks compress visually when zoomed out (camera angle flattens
    // the y=0 plane) so two thick perpendicular segments overlap into a
    // darker blob. Attenuate base alpha as radius grows so far views feel as
    // light as close ones.
    let base_alpha_close = match theme.mode {
        crate::theme::ThemeMode::Dark => 0.08,
        crate::theme::ThemeMode::Light => 0.40,
    };
    let zoom_atten = (24.0 / radius).clamp(0.35, 1.0);
    let base_alpha = base_alpha_close * zoom_atten;

    let half = ((radius * 1.5) as i32).clamp(8, 96);
    let half_sq = (half as f32).powi(2);
    let (br, bg, bb) = match theme.mode {
        crate::theme::ThemeMode::Dark => (1.0, 1.0, 1.0),
        crate::theme::ThemeMode::Light => (0.0, 0.0, 0.0),
    };
    // Two perpendicular short, thick segments per intersection. Bevy gizmos
    // can't render real point sprites, so a small `+` cross + a fat
    // line_width is the closest approximation that reads as a dot from any
    // orbit angle (a single tick foreshortens into a hyphen).
    let tick = 0.05;

    let lo_x = cx - half;
    let hi_x = cx + half;
    let lo_z = cz - half;
    let hi_z = cz + half;

    let mut x = lo_x;
    while x <= hi_x {
        let dx = (x - cx) as f32;
        let mut z = lo_z;
        while z <= hi_z {
            let dz = (z - cz) as f32;
            let alpha = floor_dot_alpha(dx * dx + dz * dz, half_sq, base_alpha);
            if alpha > 0.01 {
                let c = Color::srgba(br, bg, bb, alpha);
                let p = Vec3::new(x as f32, lift, z as f32);
                gizmos.line(p - Vec3::X * tick, p + Vec3::X * tick, c);
                gizmos.line(p - Vec3::Z * tick, p + Vec3::Z * tick, c);
            }
            z += 1;
        }
        x += 1;
    }
}

/// Triad-fade factor. Goes from 1.0 (no nearby voxels — empty scene
/// wayfinder) to 0.0 (~8 voxels packed around origin — triad would just clash
/// with the model).
fn triad_fade(near_count: usize) -> f32 {
    let t = near_count.min(8) as f32 / 8.0;
    (1.0 - t).clamp(0.0, 1.0)
}

/// Dedicated gizmo group for the origin axis triad. Uses `depth_bias = -1.0`
/// so the lines win against the floor dots (both sit at y≈0) and read clearly
/// on top.
#[derive(Default, Reflect, GizmoConfigGroup)]
pub struct OriginAxesGizmos;

fn configure_origin_axes_gizmos(mut store: ResMut<GizmoConfigStore>) {
    let (config, _) = store.config_mut::<OriginAxesGizmos>();
    config.depth_bias = -1.0;
}

/// Small RGB axis triad at world origin. Fades out as voxels appear inside a
/// 4-cell cube around origin — it's a wayfinder for the empty-scene start,
/// not always-on chrome.
fn draw_origin_system(
    prefs: Res<Preferences>,
    snapshot_active: Res<crate::snapshot::SnapshotInProgress>,
    grid: Res<VoxelGrid>,
    mut gizmos: Gizmos<OriginAxesGizmos>,
) {
    if snapshot_active.0 || !prefs.show_origin_axes {
        return;
    }
    let near = grid
        .iter_occupied()
        .filter(|(p, _)| p.x.abs() <= 4 && p.y <= 4 && p.z.abs() <= 4)
        .take(8)
        .count();
    let fade = triad_fade(near);
    if fade < 0.05 {
        return;
    }
    let len = 1.0;
    gizmos.line(Vec3::ZERO, Vec3::X * len, Color::srgba(1.0, 0.3, 0.3, fade));
    gizmos.line(Vec3::ZERO, Vec3::Y * len, Color::srgba(0.3, 1.0, 0.3, fade));
    gizmos.line(Vec3::ZERO, Vec3::Z * len, Color::srgba(0.3, 0.3, 1.0, fade));
}

/// Soft radial vignette painted over the 3D canvas. Fakes a fullscreen
/// gradient with a 5-vertex egui mesh (4 dark corners + transparent center)
/// so the canvas edges fall into shadow and the eye is drawn toward the
/// model. Drawn on the egui `Background` layer so it sits below every UI
/// surface but above the 3D pass. Gated on the same chrome toggle as the dot
/// grid (`show_floor_grid`) so users can kill all decorative chrome at once.
fn vignette_system(
    mut contexts: bevy_egui::EguiContexts,
    prefs: Res<Preferences>,
    theme: Res<Theme>,
    snapshot_active: Res<SnapshotInProgress>,
) {
    if snapshot_active.0 || !prefs.show_floor_grid {
        return;
    }
    let Ok(ctx) = contexts.ctx_mut() else { return };
    let rect = ctx.available_rect();
    if rect.width() < 4.0 || rect.height() < 4.0 {
        return;
    }
    let corner_alpha: u8 = match theme.mode {
        crate::theme::ThemeMode::Dark => 28,
        crate::theme::ThemeMode::Light => 14,
    };
    let corner = bevy_egui::egui::Color32::from_black_alpha(corner_alpha);
    let center = bevy_egui::egui::Color32::TRANSPARENT;
    let layer = bevy_egui::egui::LayerId::new(
        bevy_egui::egui::Order::Background,
        bevy_egui::egui::Id::new("vignette"),
    );
    let painter = ctx.layer_painter(layer);

    let mut mesh = bevy_egui::egui::Mesh::default();
    mesh.colored_vertex(rect.center(), center);
    mesh.colored_vertex(rect.left_top(), corner);
    mesh.colored_vertex(rect.right_top(), corner);
    mesh.colored_vertex(rect.right_bottom(), corner);
    mesh.colored_vertex(rect.left_bottom(), corner);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    mesh.add_triangle(0, 3, 4);
    mesh.add_triangle(0, 4, 1);
    painter.add(bevy_egui::egui::Shape::mesh(mesh));
}

/// Per-frame perf-warning latch. Fires a one-shot toast the first time the
/// scene crosses either the cell-count or chunk-count threshold; clears the
/// latch once both counters fall below 80 % of their thresholds. Cheap to
/// run every frame — it's a couple of integer compares.
fn perf_warn_system(mut grid: ResMut<VoxelGrid>, mut toasts: ResMut<Toasts>) {
    if !grid.is_changed() {
        return;
    }
    let cells = grid.total_count;
    let chunks = grid.chunks.len() as u32;
    if grid.warned_large {
        if large_scene_warning_cleared(cells, chunks) {
            grid.warned_large = false;
        }
    } else if large_scene_threshold_crossed(cells, chunks) {
        toasts.info("Large scene — performance may degrade.");
        grid.warned_large = true;
    }
}

fn font_setup(mut contexts: bevy_egui::EguiContexts, mut done: Local<bool>) {
    if *done {
        return;
    }
    if let Ok(ctx) = contexts.ctx_mut() {
        install_fonts(ctx);
        ctx.options_mut(|o| o.zoom_with_keyboard = false);
        *done = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn floor_dot_alpha_peaks_at_center_and_zero_at_rim() {
        let half_sq = 32.0_f32.powi(2);
        let base = 0.22;
        assert!((floor_dot_alpha(0.0, half_sq, base) - base).abs() < 1e-6);
        assert!(floor_dot_alpha(half_sq, half_sq, base).abs() < 1e-6);
        assert!(floor_dot_alpha(half_sq + 1.0, half_sq, base).abs() < 1e-6);
    }

    #[test]
    fn floor_dot_alpha_decays_quadratically() {
        let half_sq = 100.0_f32;
        let base = 1.0;
        let near = floor_dot_alpha(25.0, half_sq, base);
        let far = floor_dot_alpha(75.0, half_sq, base);
        assert!(near > far);
        assert!(near > 0.0 && far > 0.0);
    }

    #[test]
    fn floor_dot_alpha_never_negative() {
        for d in 0..200 {
            let d = d as f32;
            let a = floor_dot_alpha(d * d, 50.0 * 50.0, 0.3);
            assert!(a >= 0.0, "alpha negative at d={d}: {a}");
        }
    }

    #[test]
    fn triad_fade_full_on_empty_scene() {
        assert!((triad_fade(0) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn triad_fade_zero_when_packed() {
        assert!(triad_fade(8).abs() < 1e-6);
        assert!(triad_fade(99).abs() < 1e-6);
    }

    #[test]
    fn triad_fade_monotonically_decreases() {
        let mut prev = f32::INFINITY;
        for n in 0..=8 {
            let f = triad_fade(n);
            assert!(f <= prev + 1e-6, "non-monotone at n={n}");
            prev = f;
        }
    }
}
