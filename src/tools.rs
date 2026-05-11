use crate::grid::{Color8, VoxelGrid};
use crate::history::History;
use crate::picking::{cursor_ray, pick};
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_egui::EguiContexts;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tool {
    Brush,
    Erase,
    Paint,
    Eyedropper,
}

#[derive(Resource)]
pub struct ToolState {
    pub current: Tool,
    pub previous: Tool,
}

impl Default for ToolState {
    fn default() -> Self {
        Self { current: Tool::Brush, previous: Tool::Brush }
    }
}

#[derive(Resource)]
pub struct CurrentColor(pub Color8);

impl Default for CurrentColor {
    fn default() -> Self {
        Self([220, 200, 160, 255])
    }
}

#[derive(Resource, Default)]
pub struct RecentColors(pub Vec<Color8>);

impl RecentColors {
    pub fn push(&mut self, c: Color8) {
        if let Some(idx) = self.0.iter().position(|x| *x == c) {
            self.0.remove(idx);
        }
        self.0.insert(0, c);
        self.0.truncate(8);
    }
}

#[derive(Resource, Default)]
pub struct PointerState {
    pub stroking: bool,
}

pub fn tool_input_system(
    mut contexts: EguiContexts,
    mouse: Res<ButtonInput<MouseButton>>,
    cameras: Query<(&Camera, &GlobalTransform), With<bevy_panorbit_camera::PanOrbitCamera>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut grid: ResMut<VoxelGrid>,
    mut history: ResMut<History>,
    mut tool: ResMut<ToolState>,
    mut color: ResMut<CurrentColor>,
    mut recent: ResMut<RecentColors>,
    mut state: ResMut<PointerState>,
    gizmo_drag: Res<crate::gizmo::GizmoDrag>,
    gizmo_rect: Res<crate::gizmo::GizmoRect>,
) {
    let egui_wants_pointer = contexts
        .ctx_mut()
        .map(|c| c.is_pointer_over_area() || c.wants_pointer_input())
        .unwrap_or(false);

    let lmb_pressed = mouse.pressed(MouseButton::Left);
    let lmb_just = mouse.just_pressed(MouseButton::Left);
    let lmb_released = mouse.just_released(MouseButton::Left);

    // End stroke whenever LMB is released, regardless of where pointer is now.
    if state.stroking && (lmb_released || !lmb_pressed) {
        history.end();
        state.stroking = false;
    }

    if egui_wants_pointer || gizmo_drag.active {
        return;
    }

    // Suppress tool clicks that land on the gizmo viewport rect.
    if let (Some(rect), Ok(window)) = (gizmo_rect.0, windows.single()) {
        if let Some(c) = window.cursor_position() {
            if rect.contains(c) {
                return;
            }
        }
    }

    let Some((origin, dir)) = cursor_ray(&cameras, &windows) else { return; };
    let Some(hit) = pick(&grid, origin, dir) else { return; };

    // Single-click tools (eyedropper).
    if lmb_just && tool.current == Tool::Eyedropper {
        if hit.hit_voxel {
            if let Some(c) = grid.get(hit.cell) {
                color.0 = c;
                recent.push(c);
            }
        }
        tool.current = tool.previous;
        return;
    }

    // Stroke-based tools (brush, erase, paint).
    if lmb_just {
        history.begin();
        state.stroking = true;
        recent.push(color.0);
    }
    if !state.stroking {
        return;
    }

    match tool.current {
        Tool::Brush => {
            let target = hit.cell + hit.normal;
            if VoxelGrid::in_bounds(target) {
                history.record(&mut grid, target, Some(color.0));
            }
        }
        Tool::Erase => {
            if hit.hit_voxel {
                history.record(&mut grid, hit.cell, None);
            }
        }
        Tool::Paint => {
            if hit.hit_voxel {
                history.record(&mut grid, hit.cell, Some(color.0));
            }
        }
        Tool::Eyedropper => {}
    }
}

pub fn undo_redo_system(
    keys: Res<ButtonInput<KeyCode>>,
    mut grid: ResMut<VoxelGrid>,
    mut history: ResMut<History>,
) {
    let cmd = keys.pressed(KeyCode::SuperLeft) || keys.pressed(KeyCode::SuperRight)
        || keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight);
    if !cmd {
        return;
    }
    let shift = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
    if keys.just_pressed(KeyCode::KeyZ) {
        if shift {
            history.redo(&mut grid);
        } else {
            history.undo(&mut grid);
        }
    }
}

pub fn tool_shortcut_system(
    mut contexts: EguiContexts,
    keys: Res<ButtonInput<KeyCode>>,
    mut tool: ResMut<ToolState>,
) {
    let wants_kb = contexts
        .ctx_mut()
        .map(|c| c.wants_keyboard_input())
        .unwrap_or(false);
    if wants_kb {
        return;
    }
    let modded = keys.pressed(KeyCode::SuperLeft)
        || keys.pressed(KeyCode::SuperRight)
        || keys.pressed(KeyCode::ControlLeft)
        || keys.pressed(KeyCode::ControlRight)
        || keys.pressed(KeyCode::AltLeft)
        || keys.pressed(KeyCode::AltRight);
    if modded {
        return;
    }
    let next = if keys.just_pressed(KeyCode::KeyB) {
        Some(Tool::Brush)
    } else if keys.just_pressed(KeyCode::KeyE) {
        Some(Tool::Erase)
    } else if keys.just_pressed(KeyCode::KeyP) {
        Some(Tool::Paint)
    } else if keys.just_pressed(KeyCode::KeyI) {
        Some(Tool::Eyedropper)
    } else {
        None
    };
    if let Some(t) = next {
        if tool.current != t {
            tool.previous = tool.current;
            tool.current = t;
        }
    }
}
