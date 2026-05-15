mod camera;
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

use bevy::prelude::*;
use bevy_egui::EguiPlugin;
use bevy_panorbit_camera::PanOrbitCameraPlugin;
use std::collections::HashMap;

use crate::camera::{
    EMPTY_WORLD_RADIUS, RecenterRequest, ViewportRect, apply_recenter_system,
    default_camera_focus, frame_view_system, spawn_camera, update_viewport_rect,
    update_zoom_limits_system, zoom_click_system, zoom_key_system,
};
use crate::gizmo::{
    AxisGizmoGroup, GizmoDrag, GizmoHover, GizmoRect, configure_axis_gizmo, gizmo_drag_system,
    spawn_gizmo, sync_gizmo_camera, update_gizmo_hover, update_gizmo_viewport,
};
use crate::grid::{NewProject, VoxelGrid, large_scene_threshold_crossed, large_scene_warning_cleared};
use crate::history::History;
use crate::lighting::spawn_lights;
use crate::mesh::{PreviewHide, VoxelChunkMeshes, regenerate_mesh_system};
use crate::preview::{brush_preview_system, spawn_brush_preview};
use crate::shape_preview::{shape_preview_system, spawn_shape_preview};
use crate::snapshot::{GroundPlane, SnapshotRequest, SnapshotSession, start_snapshot_system};
use crate::theme::{
    Preferences, PreferencesWindow, Theme, install_fonts, load_preferences, refresh_theme_system,
    resolve_canvas_color, resolve_floor_color, resolve_theme,
};
use crate::tools::{
    CurrentColor, MoveDragState, PointerState, RecentColors, ShapeOptions, ShapeState, ToolState,
    alt_eyedropper_system, move_drag_system, tool_input_system, tool_shortcut_system,
    undo_redo_system,
};
use crate::ui::{
    CommandPalette, CurrentProjectPath, PaletteChoice, Palettes, PendingDialog, PendingImport,
    Toasts, command_palette_shortcut_system, dispatch_command_palette_system, poll_dialogs_system,
    toast_lifetime_system, ui_system,
};
use bevy_panorbit_camera::PanOrbitCamera;

/// Visual extent of the camera-following floor plane. Large enough that the
/// user can't see the plane's edge at typical orbit radii; recentered every
/// frame so it stays under the camera focus regardless of pan distance.
const FLOOR_PLANE_SIZE: f32 = 256.0;

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
        .init_resource::<PaletteChoice>()
        .insert_resource(Palettes::with_user_loaded())
        .init_resource::<SnapshotRequest>()
        .init_resource::<SnapshotSession>()
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
        .init_resource::<CommandPalette>()
        .init_gizmo_group::<AxisGizmoGroup>()
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
            ),
        );
    }

    app.add_systems(
        Startup,
        (
            setup_scene,
            configure_axis_gizmo,
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
            start_snapshot_system,
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
            apply_floor_color_system,
            apply_floor_visibility_system,
            floor_follow_camera_system,
            floor_grid_system,
            draw_origin_system,
            perf_warn_system,
            command_palette_shortcut_system,
            dispatch_command_palette_system,
            apply_recenter_system,
            update_zoom_limits_system,
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

    // One large floor plane recentered under the camera focus each frame. The
    // open-world grid has no fixed extent, so the floor visually "follows" the
    // user as they pan, with chunk-grid lines drawn by `floor_grid_system`.
    let floor_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.13, 0.14, 0.17),
        perceptual_roughness: 1.0,
        reflectance: 0.0,
        ..default()
    });
    let plane = meshes.add(floor_mesh(FLOOR_PLANE_SIZE));
    commands.spawn((
        Mesh3d(plane),
        MeshMaterial3d(floor_mat),
        Transform::from_xyz(0.0, -0.01, 0.0),
        GroundPlane,
    ));
}

pub fn floor_mesh(size: f32) -> Mesh {
    Mesh::from(
        bevy::math::primitives::Plane3d::default()
            .mesh()
            .size(size, size),
    )
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
    // No floor/wall rebuild, no camera reframe — the user can press Cmd+0 to
    // frame the imported model if they want. Consume the flag to signal we
    // saw it.
    if pending.0 {
        pending.0 = false;
    }
}

fn floor_follow_camera_system(
    cameras: Query<&PanOrbitCamera>,
    mut floor: Query<&mut Transform, With<GroundPlane>>,
) {
    let Ok(cam) = cameras.single() else { return };
    for mut tf in &mut floor {
        // Floor stays at y = -0.01 (just below the y = 0 voxel layer to avoid
        // z-fighting). XZ tracks the camera focus.
        tf.translation.x = cam.focus.x;
        tf.translation.z = cam.focus.z;
        tf.translation.y = -0.01;
    }
}

fn apply_floor_color_system(
    prefs: Res<Preferences>,
    theme: Res<Theme>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    planes: Query<&MeshMaterial3d<StandardMaterial>, With<GroundPlane>>,
) {
    let [r, g, b] = resolve_floor_color(&prefs, &theme);
    let next = Color::srgb_u8(r, g, b);
    for handle in &planes {
        if let Some(mat) = mats.get_mut(&handle.0)
            && mat.base_color != next
        {
            mat.base_color = next;
        }
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

fn apply_floor_visibility_system(
    prefs: Res<Preferences>,
    mut floor: Query<&mut Visibility, With<GroundPlane>>,
) {
    let want = if prefs.show_floor {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    };
    for mut v in &mut floor {
        if *v != want {
            *v = want;
        }
    }
}

/// Draws the procedural-feel chunk-grid lines on the floor. Minecraft-style:
/// thin lines every voxel, heavier lines every 16 voxels. Centered on the
/// camera focus so the grid pattern follows the user as they pan.
fn floor_grid_system(
    prefs: Res<Preferences>,
    theme: Res<crate::theme::Theme>,
    cameras: Query<&PanOrbitCamera>,
    mut gizmos: Gizmos,
) {
    if !prefs.show_floor_grid || !prefs.show_floor {
        return;
    }
    let Ok(cam) = cameras.single() else { return };
    let lift = 0.001;
    let alpha_minor = match theme.mode {
        crate::theme::ThemeMode::Dark => 0.06,
        crate::theme::ThemeMode::Light => 0.10,
    };
    let alpha_major = match theme.mode {
        crate::theme::ThemeMode::Dark => 0.14,
        crate::theme::ThemeMode::Light => 0.22,
    };
    let make = |a: f32| match theme.mode {
        crate::theme::ThemeMode::Dark => Color::srgba(1.0, 1.0, 1.0, a),
        crate::theme::ThemeMode::Light => Color::srgba(0.0, 0.0, 0.0, a),
    };

    // Grid spans a 64-voxel-wide window centered on the camera focus rounded
    // to the nearest voxel. Far enough to fill the visible floor at typical
    // zoom levels without spending gizmos on cells the user can't see.
    let half: i32 = 32;
    let cx = cam.focus.x.round() as i32;
    let cz = cam.focus.z.round() as i32;
    let lo_x = cx - half;
    let hi_x = cx + half;
    let lo_z = cz - half;
    let hi_z = cz + half;
    for i in lo_x..=hi_x {
        let major = i.rem_euclid(16) == 0;
        let c = make(if major { alpha_major } else { alpha_minor });
        gizmos.line(
            Vec3::new(i as f32, lift, lo_z as f32),
            Vec3::new(i as f32, lift, hi_z as f32),
            c,
        );
    }
    for i in lo_z..=hi_z {
        let major = i.rem_euclid(16) == 0;
        let c = make(if major { alpha_major } else { alpha_minor });
        gizmos.line(
            Vec3::new(lo_x as f32, lift, i as f32),
            Vec3::new(hi_x as f32, lift, i as f32),
            c,
        );
    }
}

/// Draws a small RGB axis triad at world origin so the user can always see
/// where (0, 0, 0) sits even with no voxels painted.
fn draw_origin_system(mut gizmos: Gizmos) {
    let len = 1.0;
    gizmos.line(Vec3::ZERO, Vec3::X * len, Color::srgb(1.0, 0.3, 0.3));
    gizmos.line(Vec3::ZERO, Vec3::Y * len, Color::srgb(0.3, 1.0, 0.3));
    gizmos.line(Vec3::ZERO, Vec3::Z * len, Color::srgb(0.3, 0.3, 1.0));
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
