mod camera;
mod gizmo;
mod grid;
mod history;
mod io;
mod lighting;
mod mesh;
mod picking;
mod preview;
mod snapshot;
mod tools;
mod ui;

use bevy::prelude::*;
use bevy_egui::EguiPlugin;
use bevy_panorbit_camera::PanOrbitCameraPlugin;

use crate::camera::{frame_view_system, spawn_camera};
use crate::gizmo::{
    AxisGizmoGroup, GizmoDrag, GizmoHover, GizmoRect, configure_axis_gizmo, gizmo_drag_system,
    spawn_gizmo, sync_gizmo_camera, update_gizmo_hover, update_gizmo_viewport,
};
use crate::grid::{GRID, VoxelGrid};
use crate::history::History;
use crate::lighting::spawn_lights;
use crate::mesh::{PreviewHide, VoxelMesh, VoxelMeshHandle, regenerate_mesh_system};
use crate::preview::{brush_preview_system, spawn_brush_preview};
use crate::snapshot::{GroundPlane, SnapshotRequest, SnapshotSession, start_snapshot_system};
use crate::tools::{CurrentColor, PointerState, RecentColors, ToolState, alt_eyedropper_system, tool_input_system, tool_shortcut_system, undo_redo_system};
use crate::ui::{PaletteChoice, Palettes, PendingDialog, apply_style, poll_dialogs_system, ui_system};

fn main() {
    App::new()
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
        .insert_resource(ClearColor(Color::srgb(0.07, 0.08, 0.10)))
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
        .add_systems(Startup, (style_startup, setup_scene, configure_axis_gizmo))
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
                start_snapshot_system,
            ),
        )
        .add_systems(
            bevy_egui::EguiPrimaryContextPass,
            (ui_system, update_gizmo_viewport.after(ui_system)),
        )
        .run();
}

fn style_startup(contexts: bevy_egui::EguiContexts) {
    let _ = apply_style(contexts);
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
}
