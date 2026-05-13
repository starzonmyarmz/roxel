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

use crate::camera::{
    ViewportRect, frame_view_system, spawn_camera, update_viewport_rect, zoom_click_system,
};
use crate::gizmo::{
    AxisGizmoGroup, GizmoDrag, GizmoHover, GizmoRect, configure_axis_gizmo, gizmo_drag_system,
    spawn_gizmo, sync_gizmo_camera, update_gizmo_hover, update_gizmo_viewport,
};
use crate::grid::{MAX_CHUNKS_PER_AXIS, NewProject, VoxelGrid};
use crate::history::History;
use crate::lighting::spawn_lights;
use bevy_panorbit_camera::PanOrbitCamera;
use crate::mesh::{PreviewHide, VoxelChunkMeshes, VoxelMesh, regenerate_mesh_system};
use crate::preview::{brush_preview_system, spawn_brush_preview};
use crate::shape_preview::{shape_preview_system, spawn_shape_preview};
use crate::snapshot::{GroundPlane, SnapshotRequest, SnapshotSession, WallPlane, start_snapshot_system};
use crate::tools::{CurrentColor, MoveDragState, PointerState, RecentColors, ShapeOptions, ShapeState, ToolState, alt_eyedropper_system, move_drag_system, tool_input_system, tool_shortcut_system, undo_redo_system};
use crate::theme::{
    Preferences, PreferencesWindow, Theme, install_fonts, load_preferences, refresh_theme_system,
    resolve_canvas_color, resolve_floor_color, resolve_theme, resolve_wall_color,
};
use crate::ui::{PaletteChoice, Palettes, PendingDialog, poll_dialogs_system, ui_system};

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
        .init_resource::<PaletteChoice>()
        .insert_resource(Palettes::with_user_loaded())
        .init_resource::<SnapshotRequest>()
        .init_resource::<SnapshotSession>()
        .init_resource::<GizmoRect>()
        .init_resource::<GizmoDrag>()
        .init_resource::<GizmoHover>()
        .init_resource::<ViewportRect>()
        .init_resource::<NewProject>()
        .init_gizmo_group::<AxisGizmoGroup>();

    #[cfg(target_os = "macos")]
    {
        app.init_resource::<crate::menu::MenuQueue>()
            .add_systems(
                Update,
                (
                    crate::menu::install_menu_system,
                    crate::menu::poll_menu_events_system
                        .after(crate::menu::install_menu_system),
                    crate::menu::apply_menu_actions_system
                        .after(crate::menu::poll_menu_events_system),
                    crate::menu::update_menu_enabled_system
                        .after(crate::menu::install_menu_system),
                ),
            );
    }

    app.add_systems(Startup, (setup_scene, configure_axis_gizmo))
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
            ),
        )
        .add_systems(
            Update,
            (
                crate::icon::set_window_icon,
                refresh_theme_system,
                zoom_click_system,
                apply_canvas_bg_system,
                apply_floor_color_system,
                apply_wall_color_system,
                apply_floor_visibility_system,
                apply_walls_visibility_system,
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
    crate::select::spawn_selection_preview(&mut commands, &mut meshes, &mut materials);

    // One mesh entity per chunk. Mesher rebuilds only flagged chunks each
    // frame so a single-cell edit doesn't touch the whole grid.
    let mat = materials.add(StandardMaterial {
        base_color: Color::WHITE,
        unlit: true,
        ..default()
    });
    let chunk_count = MAX_CHUNKS_PER_AXIS.pow(3);
    let mut chunk_handles = Vec::with_capacity(chunk_count);
    for _ in 0..chunk_count {
        let h = meshes.add(Mesh::from(bevy::math::primitives::Cuboid::new(0.0, 0.0, 0.0)));
        commands.spawn((
            Mesh3d(h.clone()),
            MeshMaterial3d(mat.clone()),
            Transform::default(),
            VoxelMesh,
        ));
        chunk_handles.push(h);
    }
    commands.insert_resource(VoxelChunkMeshes { handles: chunk_handles });

    // Separate materials for floor vs walls so each can have its own color.
    let size = crate::grid::DEFAULT_SIZE as f32;
    let half = size / 2.0;
    let floor_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.13, 0.14, 0.17),
        perceptual_roughness: 1.0,
        reflectance: 0.0,
        ..default()
    });
    let wall_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.13, 0.14, 0.17),
        perceptual_roughness: 1.0,
        reflectance: 0.0,
        ..default()
    });

    let plane = meshes.add(floor_mesh(size));
    commands.spawn((
        Mesh3d(plane),
        MeshMaterial3d(floor_mat),
        Transform::from_xyz(half, -0.01, half),
        GroundPlane,
    ));

    // Wall planes: back wall (z=0, normal +Z) and left wall (x=0, normal +X).
    // Slightly outside the grid so they don't z-fight with edge voxels.
    let back_wall = meshes.add(wall_mesh(0, size));
    commands.spawn((
        Mesh3d(back_wall),
        MeshMaterial3d(wall_mat.clone()),
        Transform::from_xyz(half, half, -0.01),
        Visibility::Hidden,
        WallPlane(0),
    ));
    let left_wall = meshes.add(wall_mesh(1, size));
    commands.spawn((
        Mesh3d(left_wall),
        MeshMaterial3d(wall_mat),
        Transform::from_xyz(-0.01, half, half),
        Visibility::Hidden,
        WallPlane(1),
    ));
}

pub fn floor_mesh(size: f32) -> Mesh {
    Mesh::from(
        bevy::math::primitives::Plane3d::default()
            .mesh()
            .size(size, size),
    )
}

pub fn wall_mesh(axis: u8, size: f32) -> Mesh {
    let half = size / 2.0;
    let normal = if axis == 0 { Vec3::Z } else { Vec3::X };
    Mesh::from(bevy::math::primitives::Plane3d::new(normal, Vec2::splat(half)).mesh())
}

fn apply_new_project_system(
    mut new_project: ResMut<NewProject>,
    mut grid: ResMut<VoxelGrid>,
    mut history: ResMut<History>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut floor: Query<(&Mesh3d, &mut Transform), (With<GroundPlane>, Without<WallPlane>)>,
    mut walls: Query<(&Mesh3d, &mut Transform, &WallPlane), Without<GroundPlane>>,
    mut cameras: Query<&mut PanOrbitCamera>,
) {
    let Some(new_size) = new_project.apply.take() else {
        return;
    };
    grid.resize(new_size);
    history.undo.clear();
    history.redo.clear();
    history.current = None;

    let s = new_size as f32;
    let half = s / 2.0;

    // Replace floor mesh data + reposition.
    for (mesh3d, mut tf) in &mut floor {
        if let Some(m) = meshes.get_mut(&mesh3d.0) {
            *m = floor_mesh(s);
        }
        *tf = Transform::from_xyz(half, -0.01, half);
    }

    // Walls: 0 = back (z=-0.01, centred at (half, half)), 1 = left (x=-0.01).
    for (mesh3d, mut tf, plane) in &mut walls {
        if let Some(m) = meshes.get_mut(&mesh3d.0) {
            *m = wall_mesh(plane.0, s);
        }
        *tf = match plane.0 {
            0 => Transform::from_xyz(half, half, -0.01),
            _ => Transform::from_xyz(-0.01, half, half),
        };
    }

    // Recenter the orbit camera on the new grid.
    for mut cam in &mut cameras {
        cam.target_focus = Vec3::new(half, 0.0, half);
        cam.target_radius = s * 1.875;
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

fn apply_wall_color_system(
    prefs: Res<Preferences>,
    theme: Res<Theme>,
    mut mats: ResMut<Assets<StandardMaterial>>,
    planes: Query<&MeshMaterial3d<StandardMaterial>, With<WallPlane>>,
) {
    let [r, g, b] = resolve_wall_color(&prefs, &theme);
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
    let want = if prefs.show_floor { Visibility::Inherited } else { Visibility::Hidden };
    for mut v in &mut floor {
        if *v != want {
            *v = want;
        }
    }
}

fn apply_walls_visibility_system(
    prefs: Res<Preferences>,
    mut walls: Query<&mut Visibility, With<WallPlane>>,
) {
    let want = if prefs.show_walls { Visibility::Inherited } else { Visibility::Hidden };
    for mut v in &mut walls {
        if *v != want {
            *v = want;
        }
    }
}

fn font_setup(mut contexts: bevy_egui::EguiContexts, mut done: Local<bool>) {
    if *done {
        return;
    }
    if let Ok(ctx) = contexts.ctx_mut() {
        install_fonts(ctx);
        *done = true;
    }
}
