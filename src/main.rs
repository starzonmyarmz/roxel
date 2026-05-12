mod camera;
mod gizmo;
mod grid;
mod history;
mod icon;
mod io;
mod lighting;
mod mesh;
mod picking;
mod preview;
mod shape_preview;
mod shapes;
mod snapshot;
mod theme;
mod tools;
mod ui;

use bevy::prelude::*;
use bevy_egui::EguiPlugin;
use bevy_panorbit_camera::PanOrbitCameraPlugin;

use crate::camera::{frame_view_system, spawn_camera, zoom_click_system};
use crate::gizmo::{
    AxisGizmoGroup, GizmoDrag, GizmoHover, GizmoRect, configure_axis_gizmo, gizmo_drag_system,
    spawn_gizmo, sync_gizmo_camera, update_gizmo_hover, update_gizmo_viewport,
};
use crate::grid::{GRID, VoxelGrid};
use crate::history::History;
use crate::lighting::spawn_lights;
use crate::mesh::{PreviewHide, VoxelMesh, VoxelMeshHandle, regenerate_mesh_system};
use crate::preview::{brush_preview_system, spawn_brush_preview};
use crate::shape_preview::{shape_preview_system, spawn_shape_preview};
use crate::snapshot::{GroundPlane, SnapshotRequest, SnapshotSession, WallPlane, start_snapshot_system};
use crate::tools::{CurrentColor, PointerState, RecentColors, ShapeOptions, ShapeState, ToolState, alt_eyedropper_system, tool_input_system, tool_shortcut_system, undo_redo_system};
use crate::theme::{
    Preferences, PreferencesWindow, Theme, install_fonts, load_preferences, refresh_theme_system,
    resolve_canvas_color, resolve_theme,
};
use crate::ui::{PaletteChoice, Palettes, PendingDialog, poll_dialogs_system, ui_system};

fn main() {
    let prefs = load_preferences();
    let theme = resolve_theme(prefs.theme);
    let initial_canvas = {
        let [r, g, b] = resolve_canvas_color(&prefs, &theme);
        Color::srgb_u8(r, g, b)
    };
    App::new()
        .insert_resource(prefs)
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
        .init_resource::<PreviewHide>()
        .init_resource::<PendingDialog>()
        .init_resource::<PaletteChoice>()
        .init_resource::<Palettes>()
        .init_resource::<SnapshotRequest>()
        .init_resource::<SnapshotSession>()
        .init_resource::<GizmoRect>()
        .init_resource::<GizmoDrag>()
        .init_resource::<GizmoHover>()
        .init_gizmo_group::<AxisGizmoGroup>()
        .add_systems(Startup, (setup_scene, configure_axis_gizmo))
        .add_systems(
            Update,
            (
                alt_eyedropper_system,
                tool_input_system,
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
                start_snapshot_system,
            ),
        )
        .add_systems(
            Update,
            (
                crate::icon::set_window_icon,
                refresh_theme_system,
                zoom_click_system,
                apply_canvas_bg_system,
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
            (ui_system, update_gizmo_viewport.after(ui_system)),
        )
        .run();
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

    // Voxel mesh entity (empty initially; mesher fills it).
    let mesh_handle = meshes.add(Mesh::from(bevy::math::primitives::Cuboid::new(0.0, 0.0, 0.0)));
    let mat = materials.add(StandardMaterial {
        base_color: Color::WHITE,
        unlit: true,
        ..default()
    });
    commands.spawn((
        Mesh3d(mesh_handle.clone()),
        MeshMaterial3d(mat),
        Transform::default(),
        VoxelMesh,
    ));
    commands.insert_resource(VoxelMeshHandle(mesh_handle));

    // Ground plane at y=0 for spatial reference.
    let plane = meshes.add(Mesh::from(bevy::math::primitives::Plane3d::default().mesh().size(GRID as f32, GRID as f32)));
    let plane_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.13, 0.14, 0.17),
        perceptual_roughness: 1.0,
        reflectance: 0.0,
        ..default()
    });
    commands.spawn((
        Mesh3d(plane),
        MeshMaterial3d(plane_mat),
        Transform::from_xyz(GRID as f32 / 2.0, -0.01, GRID as f32 / 2.0),
        GroundPlane,
    ));

    // Wall planes: back wall (z=0, normal +Z) and left wall (x=0, normal +X).
    // Slightly outside the grid so they don't z-fight with edge voxels.
    let half = GRID as f32 / 2.0;
    let wall_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.13, 0.14, 0.17),
        perceptual_roughness: 1.0,
        reflectance: 0.0,
        ..default()
    });
    let back_wall = meshes.add(Mesh::from(
        bevy::math::primitives::Plane3d::new(Vec3::Z, Vec2::splat(half)).mesh(),
    ));
    commands.spawn((
        Mesh3d(back_wall),
        MeshMaterial3d(wall_mat.clone()),
        Transform::from_xyz(half, half, -0.01),
        Visibility::Hidden,
        WallPlane,
    ));
    let left_wall = meshes.add(Mesh::from(
        bevy::math::primitives::Plane3d::new(Vec3::X, Vec2::splat(half)).mesh(),
    ));
    commands.spawn((
        Mesh3d(left_wall),
        MeshMaterial3d(wall_mat),
        Transform::from_xyz(-0.01, half, half),
        Visibility::Hidden,
        WallPlane,
    ));
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
