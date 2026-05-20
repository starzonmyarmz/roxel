mod camera;
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
    RecentFiles, Toasts, command_palette_shortcut_system, dispatch_command_palette_system,
    poll_dialogs_system, toast_lifetime_system, ui_system,
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
        .init_gizmo_group::<AxisGizmoGroup>()
        .init_gizmo_group::<OriginAxesGizmos>()
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
            crate::select::selection_key_action_system,
            crate::select::move_selection_keys_system,
            start_snapshot_system
                .before(floor_grid_system)
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
            floor_grid_system,
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
        ),
    )
    .add_systems(
        PreUpdate,
        font_setup
            .after(bevy_egui::EguiPreUpdateSet::InitContexts)
            .before(bevy_egui::EguiPreUpdateSet::BeginPass),
    )
    .add_systems(
        bevy_egui::EguiPrimaryContextPass,
        (
            ui_system,
            update_gizmo_viewport.after(ui_system),
            update_viewport_rect.after(ui_system),
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

/// Draws procedural grid lines on the y=0 plane. Two-band LOD:
/// - radius ≤ `GRID_VOXEL_RADIUS`: per-voxel grid (spacing 1) with heavier
///   lines every 16.
/// - radius ≤ `GRID_CHUNK_RADIUS`: spacing-16 grid only.
/// - beyond: hidden — at that range the cluster itself is the scale.
fn floor_grid_system(
    prefs: Res<Preferences>,
    theme: Res<crate::theme::Theme>,
    snapshot_active: Res<crate::snapshot::SnapshotInProgress>,
    cameras: Query<&PanOrbitCamera>,
    mut gizmos: Gizmos,
) {
    if snapshot_active.0 || !prefs.show_floor_grid {
        return;
    }
    let Ok(cam) = cameras.single() else { return };
    let radius = cam.target_radius.max(0.001);
    if radius > GRID_CHUNK_RADIUS {
        return;
    }
    let lift = 0.001;
    let cx = cam.focus.x.round() as i32;
    let cz = cam.focus.z.round() as i32;
    let make = |a: f32| match theme.mode {
        crate::theme::ThemeMode::Dark => Color::srgba(1.0, 1.0, 1.0, a),
        crate::theme::ThemeMode::Light => Color::srgba(0.0, 0.0, 0.0, a),
    };
    let (a_minor, a_major) = match theme.mode {
        crate::theme::ThemeMode::Dark => (0.07, 0.18),
        crate::theme::ThemeMode::Light => (0.11, 0.26),
    };

    let half = ((radius * 3.0) as i32).clamp(48, 1024);
    let lo_x = cx - half;
    let hi_x = cx + half;
    let lo_z = cz - half;
    let hi_z = cz + half;

    let (spacing, line_color) = if radius <= GRID_VOXEL_RADIUS {
        (1, make(a_minor))
    } else {
        // Chunk-band: only every-16 lines, drawn at the major alpha so the
        // grid still reads clearly against open canvas at that distance.
        (16, make(a_major))
    };
    let major_color = make(a_major);
    let start_x = lo_x.div_euclid(spacing) * spacing;
    let start_z = lo_z.div_euclid(spacing) * spacing;

    let mut i = start_x;
    while i <= hi_x {
        let c = if spacing == 1 && i.rem_euclid(16) == 0 {
            major_color
        } else {
            line_color
        };
        gizmos.line(
            Vec3::new(i as f32, lift, lo_z as f32),
            Vec3::new(i as f32, lift, hi_z as f32),
            c,
        );
        i += spacing;
    }
    let mut i = start_z;
    while i <= hi_z {
        let c = if spacing == 1 && i.rem_euclid(16) == 0 {
            major_color
        } else {
            line_color
        };
        gizmos.line(
            Vec3::new(lo_x as f32, lift, i as f32),
            Vec3::new(hi_x as f32, lift, i as f32),
            c,
        );
        i += spacing;
    }
}

/// Orbit radius past which the per-voxel (spacing-1) grid is dropped — the
/// individual voxel lines alias into noise beyond this point.
pub const GRID_VOXEL_RADIUS: f32 = 128.0;

/// Orbit radius past which the grid is hidden entirely. Between this and
/// `GRID_VOXEL_RADIUS` only the every-16 chunk grid is drawn.
pub const GRID_CHUNK_RADIUS: f32 = 512.0;

/// Draws a small RGB axis triad at world origin so the user can always see
/// where (0, 0, 0) sits even with no voxels painted. The green Y axis
/// extends far up into the sky as a vertical anchor when `show_y_axis` is
/// enabled, so the user never loses track of the origin column.
/// Dedicated gizmo group for the origin axis triad. Uses `depth_bias = -1.0`
/// so the lines win against the floor grid (both sit at y≈0) and read clearly
/// on top.
#[derive(Default, Reflect, GizmoConfigGroup)]
pub struct OriginAxesGizmos;

fn configure_origin_axes_gizmos(mut store: ResMut<GizmoConfigStore>) {
    let (config, _) = store.config_mut::<OriginAxesGizmos>();
    config.depth_bias = -1.0;
}

fn draw_origin_system(
    prefs: Res<Preferences>,
    snapshot_active: Res<crate::snapshot::SnapshotInProgress>,
    mut gizmos: Gizmos<OriginAxesGizmos>,
) {
    if snapshot_active.0 || !prefs.show_origin_axes {
        return;
    }
    let len = 1.0;
    gizmos.line(Vec3::ZERO, Vec3::X * len, Color::srgb(1.0, 0.3, 0.3));
    gizmos.line(Vec3::ZERO, Vec3::Z * len, Color::srgb(0.3, 0.3, 1.0));
    if prefs.show_y_axis {
        gizmos.line(
            Vec3::ZERO,
            Vec3::Y * 10_000.0,
            Color::srgba(0.3, 1.0, 0.3, 0.55),
        );
    } else {
        gizmos.line(Vec3::ZERO, Vec3::Y * len, Color::srgb(0.3, 1.0, 0.3));
    }
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
