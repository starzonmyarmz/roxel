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

#[derive(Clone, Copy)]
pub struct StrokeAnchor {
    pub axis: usize,
    pub plane_world: f32,
    pub target_layer: i32,
}

#[derive(Resource, Default)]
pub struct PointerState {
    pub stroking: bool,
    pub anchor: Option<StrokeAnchor>,
    pub last_placed: Option<IVec3>,
    // Pre-stroke snapshot of the grid. Ray-picks during a stroke run against
    // this snapshot, so voxels placed earlier in the same stroke are invisible
    // to the picker — that's what prevents the freshly placed voxel from being
    // re-hit and triggering runaway stacking. Pattern lifted from goxel.
    pub snapshot: Option<VoxelGrid>,
}

fn line3d(a: IVec3, b: IVec3) -> Vec<IVec3> {
    let d = (b - a).abs();
    let s = IVec3::new(
        if b.x >= a.x { 1 } else { -1 },
        if b.y >= a.y { 1 } else { -1 },
        if b.z >= a.z { 1 } else { -1 },
    );
    let (dx, dy, dz) = (d.x, d.y, d.z);
    let mut cur = a;
    let mut out = Vec::with_capacity(dx.max(dy).max(dz) as usize + 1);
    out.push(cur);
    if dx >= dy && dx >= dz {
        let (mut p1, mut p2) = (2 * dy - dx, 2 * dz - dx);
        while cur.x != b.x {
            if p1 >= 0 { cur.y += s.y; p1 -= 2 * dx; }
            if p2 >= 0 { cur.z += s.z; p2 -= 2 * dx; }
            cur.x += s.x;
            p1 += 2 * dy;
            p2 += 2 * dz;
            out.push(cur);
        }
    } else if dy >= dx && dy >= dz {
        let (mut p1, mut p2) = (2 * dx - dy, 2 * dz - dy);
        while cur.y != b.y {
            if p1 >= 0 { cur.x += s.x; p1 -= 2 * dy; }
            if p2 >= 0 { cur.z += s.z; p2 -= 2 * dy; }
            cur.y += s.y;
            p1 += 2 * dx;
            p2 += 2 * dz;
            out.push(cur);
        }
    } else {
        let (mut p1, mut p2) = (2 * dy - dz, 2 * dx - dz);
        while cur.z != b.z {
            if p1 >= 0 { cur.y += s.y; p1 -= 2 * dz; }
            if p2 >= 0 { cur.x += s.x; p2 -= 2 * dz; }
            cur.z += s.z;
            p1 += 2 * dy;
            p2 += 2 * dx;
            out.push(cur);
        }
    }
    out
}

fn axis_of_normal(n: IVec3) -> usize {
    if n.x != 0 { 0 } else if n.y != 0 { 1 } else { 2 }
}

fn anchor_target(anchor: &StrokeAnchor, origin: Vec3, dir: Vec3) -> Option<IVec3> {
    let d_arr = dir.to_array();
    let o_arr = origin.to_array();
    let denom = d_arr[anchor.axis];
    if denom.abs() < 1e-6 {
        return None;
    }
    let t = (anchor.plane_world - o_arr[anchor.axis]) / denom;
    if t < 0.0 {
        return None;
    }
    let p = (origin + dir * t).to_array();
    let mut cell = [0i32; 3];
    for i in 0..3 {
        cell[i] = if i == anchor.axis {
            anchor.target_layer
        } else {
            p[i].floor() as i32
        };
    }
    Some(IVec3::new(cell[0], cell[1], cell[2]))
}

pub fn tool_input_system(
    mut contexts: EguiContexts,
    mouse: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
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
        state.anchor = None;
        state.snapshot = None;
    }

    if egui_wants_pointer || gizmo_drag.active {
        return;
    }

    // Space held = pan modifier for the camera; suppress tool input so a pan
    // drag doesn't also paint.
    if keys.pressed(KeyCode::Space) {
        return;
    }

    // Suppress tool clicks that land on the gizmo viewport rect.
    if let (Some(rect), Ok(window)) = (gizmo_rect.0, windows.single())
        && let Some(c) = window.cursor_position()
            && rect.contains(c) {
                return;
            }

    let Some((origin, dir)) = cursor_ray(&cameras, &windows) else { return; };

    let alt = keys.pressed(KeyCode::AltLeft) || keys.pressed(KeyCode::AltRight);

    // Single-click tools (eyedropper).
    if lmb_just && tool.current == Tool::Eyedropper {
        if let Some(hit) = pick(&grid, origin, dir)
            && hit.hit_voxel
                && let Some(c) = grid.get(hit.cell) {
                    color.0 = c;
                    recent.push(c);
                }
        // Stay in eyedropper while Alt is held; otherwise restore previous tool.
        if !alt {
            tool.current = tool.previous;
        }
        return;
    }

    let shift = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);

    // Shift + click: draw a 3D line from the last placed voxel to the new target.
    // Performs a single-frame stroke; does not enter drag mode.
    if lmb_just && shift
        && let Some(from) = state.last_placed {
            let Some(hit) = pick(&grid, origin, dir) else { return; };
            let target = match tool.current {
                Tool::Brush => hit.cell + hit.normal,
                Tool::Erase | Tool::Paint if hit.hit_voxel => hit.cell,
                _ => return,
            };
            if !VoxelGrid::in_bounds(target) {
                return;
            }
            history.begin();
            for cell in line3d(from, target) {
                if !VoxelGrid::in_bounds(cell) {
                    continue;
                }
                match tool.current {
                    Tool::Brush => history.record(&mut grid, cell, Some(color.0)),
                    Tool::Erase => {
                        if grid.get(cell).is_some() {
                            history.record(&mut grid, cell, None);
                        }
                    }
                    Tool::Paint => {
                        if grid.get(cell).is_some() {
                            history.record(&mut grid, cell, Some(color.0));
                        }
                    }
                    Tool::Eyedropper => {}
                }
            }
            history.end();
            state.last_placed = Some(target);
            recent.push(color.0);
            return;
        }

    // Stroke start: anchor a build plane based on the first hit, then stick to it
    // for the duration of the stroke. Prevents runaway stacking when the freshly
    // placed voxel becomes the next frame's pick target.
    if lmb_just {
        let Some(hit) = pick(&grid, origin, dir) else { return; };
        let axis = axis_of_normal(hit.normal);
        let cell_arr = hit.cell.to_array();
        let n_arr = hit.normal.to_array();
        let plane_world = cell_arr[axis] as f32 + if n_arr[axis] > 0 { 1.0 } else { 0.0 };
        let target_layer = match tool.current {
            Tool::Brush => cell_arr[axis] + n_arr[axis],
            _ => cell_arr[axis],
        };
        history.begin();
        state.stroking = true;
        state.anchor = Some(StrokeAnchor { axis, plane_world, target_layer });
        state.last_placed = None;
        state.snapshot = Some(grid.clone());
        recent.push(color.0);
    }

    if !state.stroking {
        return;
    }
    let Some(anchor) = state.anchor else { return; };
    let Some(anchored) = anchor_target(&anchor, origin, dir) else { return; };

    // Goxel-style: pick against the pre-stroke snapshot, never the live grid.
    // Voxels placed earlier in this stroke are invisible to the picker, so they
    // can't become next frame's hit target — that's what kills the runaway.
    let snap = state.snapshot.as_ref().unwrap_or(&grid);
    let target = match (tool.current, pick(snap, origin, dir)) {
        (Tool::Brush, Some(hit)) if hit.hit_voxel => hit.cell + hit.normal,
        (Tool::Erase | Tool::Paint, Some(hit)) if hit.hit_voxel => hit.cell,
        _ => anchored,
    };

    if !VoxelGrid::in_bounds(target) {
        return;
    }

    let path: Vec<IVec3> = match state.last_placed {
        Some(from) if from != target => line3d(from, target),
        _ => vec![target],
    };

    for cell in path {
        if !VoxelGrid::in_bounds(cell) {
            continue;
        }
        match tool.current {
            Tool::Brush => {
                history.record(&mut grid, cell, Some(color.0));
                state.last_placed = Some(cell);
            }
            Tool::Erase => {
                if grid.get(cell).is_some() {
                    history.record(&mut grid, cell, None);
                }
                state.last_placed = Some(cell);
            }
            Tool::Paint => {
                if grid.get(cell).is_some() {
                    history.record(&mut grid, cell, Some(color.0));
                    state.last_placed = Some(cell);
                }
            }
            Tool::Eyedropper => {}
        }
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

pub fn alt_eyedropper_system(
    keys: Res<ButtonInput<KeyCode>>,
    mut tool: ResMut<ToolState>,
) {
    let alt_just = keys.just_pressed(KeyCode::AltLeft) || keys.just_pressed(KeyCode::AltRight);
    let alt_released =
        keys.just_released(KeyCode::AltLeft) || keys.just_released(KeyCode::AltRight);

    if alt_just && tool.current != Tool::Eyedropper {
        tool.previous = tool.current;
        tool.current = Tool::Eyedropper;
    } else if alt_released && tool.current == Tool::Eyedropper {
        tool.current = tool.previous;
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
    if let Some(t) = next
        && tool.current != t {
            tool.previous = tool.current;
            tool.current = t;
        }
}
