use crate::grid::{Color8, VoxelGrid};
use crate::history::History;
use crate::picking::{cursor_ray, pick};
use crate::shapes::{ShapePrimitive, ellipse_cells, extrude, line2d_cells, rect_cells};
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_egui::EguiContexts;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tool {
    Brush,
    Erase,
    Paint,
    Eyedropper,
    Shape,
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

#[derive(Resource, Clone, Copy)]
pub struct ShapeOptions {
    pub primitive: ShapePrimitive,
    pub filled: bool,
}

impl Default for ShapeOptions {
    fn default() -> Self {
        Self { primitive: ShapePrimitive::Rectangle, filled: true }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ShapePhase {
    Footprint,
    Extrude,
}

#[derive(Resource, Default)]
pub struct ShapeState {
    pub phase: Option<ShapePhase>,
    pub anchor: Option<StrokeAnchor>,
    pub normal_sign: i32,
    pub corner1: Option<IVec3>,
    pub corner2: Option<IVec3>,
    pub thickness: i32,
}

impl ShapeState {
    pub fn reset(&mut self) {
        self.phase = None;
        self.anchor = None;
        self.normal_sign = 0;
        self.corner1 = None;
        self.corner2 = None;
        self.thickness = 1;
    }
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

fn footprint_center_world(c1: IVec3, c2: IVec3, axis: usize, plane_world: f32) -> Vec3 {
    let a1 = c1.to_array();
    let a2 = c2.to_array();
    let mut out = [0.0f32; 3];
    for i in 0..3 {
        out[i] = if i == axis {
            plane_world
        } else {
            (a1[i] as f32 + a2[i] as f32) * 0.5 + 0.5
        };
    }
    Vec3::from_array(out)
}

fn thickness_from_ray(
    anchor: &StrokeAnchor,
    normal_sign: i32,
    footprint_center: Vec3,
    origin: Vec3,
    dir: Vec3,
) -> i32 {
    let mut line_dir = [0.0f32; 3];
    line_dir[anchor.axis] = 1.0;
    let l = Vec3::from_array(line_dir);
    let r = origin - footprint_center;
    let a = dir.dot(dir);
    let b = dir.dot(l);
    let c = l.dot(l);
    let dd = dir.dot(r);
    let e = l.dot(r);
    let denom = a * c - b * b;
    if denom.abs() < 1e-6 {
        return 1;
    }
    let s = (a * e - b * dd) / denom;
    let dist = s * normal_sign as f32;
    dist.max(1.0).ceil() as i32
}

fn shape_commit(
    options: &ShapeOptions,
    state: &mut ShapeState,
    grid: &mut VoxelGrid,
    history: &mut History,
    color: Color8,
    recent: &mut RecentColors,
) {
    let (Some(anchor), Some(c1), Some(c2)) = (state.anchor, state.corner1, state.corner2) else {
        state.reset();
        return;
    };
    let base = match options.primitive {
        ShapePrimitive::Rectangle => rect_cells(c1, c2, anchor.axis, options.filled),
        ShapePrimitive::Ellipse => ellipse_cells(c1, c2, anchor.axis, options.filled),
        ShapePrimitive::Line => line2d_cells(c1, c2, anchor.axis),
    };
    let cells = extrude(&base, anchor.axis, state.thickness.max(1), state.normal_sign);
    history.begin();
    for cell in cells {
        if VoxelGrid::in_bounds(cell) {
            history.record(grid, cell, Some(color));
        }
    }
    history.end();
    recent.push(color);
    state.reset();
}

fn shape_input(
    options: &ShapeOptions,
    state: &mut ShapeState,
    grid: &mut VoxelGrid,
    history: &mut History,
    color: Color8,
    recent: &mut RecentColors,
    keys: &ButtonInput<KeyCode>,
    mouse: &ButtonInput<MouseButton>,
    origin: Vec3,
    dir: Vec3,
    blocked: bool,
) {
    let lmb_just = mouse.just_pressed(MouseButton::Left);
    let lmb_released = mouse.just_released(MouseButton::Left);
    let rmb_just = mouse.just_pressed(MouseButton::Right);
    let esc = keys.just_pressed(KeyCode::Escape);

    if esc || rmb_just {
        state.reset();
        return;
    }

    match state.phase {
        None => {
            if !lmb_just || blocked {
                return;
            }
            let Some(hit) = pick(grid, origin, dir) else { return; };
            let axis = axis_of_normal(hit.normal);
            let n_arr = hit.normal.to_array();
            let sign = if n_arr[axis] >= 0 { 1 } else { -1 };
            let cell_arr = hit.cell.to_array();
            let plane_world = cell_arr[axis] as f32 + if sign > 0 { 1.0 } else { 0.0 };
            let target_layer = cell_arr[axis] + n_arr[axis];
            let anchor = StrokeAnchor { axis, plane_world, target_layer };
            let start_cell = anchor_target(&anchor, origin, dir).unwrap_or_else(|| {
                IVec3::new(
                    cell_arr[0] + n_arr[0],
                    cell_arr[1] + n_arr[1],
                    cell_arr[2] + n_arr[2],
                )
            });
            state.phase = Some(ShapePhase::Footprint);
            state.anchor = Some(anchor);
            state.normal_sign = sign;
            state.corner1 = Some(start_cell);
            state.corner2 = Some(start_cell);
            state.thickness = 1;
        }
        Some(ShapePhase::Footprint) => {
            let Some(anchor) = state.anchor else { return; };
            if let Some(target) = anchor_target(&anchor, origin, dir) {
                state.corner2 = Some(target);
            }
            if lmb_released {
                state.phase = Some(ShapePhase::Extrude);
                state.thickness = 1;
            }
        }
        Some(ShapePhase::Extrude) => {
            let Some(anchor) = state.anchor else { return; };
            let (Some(c1), Some(c2)) = (state.corner1, state.corner2) else { return; };
            let center = footprint_center_world(c1, c2, anchor.axis, anchor.plane_world);
            state.thickness = thickness_from_ray(&anchor, state.normal_sign, center, origin, dir);
            if lmb_just && !blocked {
                shape_commit(options, state, grid, history, color, recent);
            }
        }
    }
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
    shape_options: Res<ShapeOptions>,
    mut shape_state: ResMut<ShapeState>,
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

    if tool.current != Tool::Shape && shape_state.phase.is_some() {
        shape_state.reset();
    }

    if tool.current == Tool::Shape {
        let cursor_over_gizmo = if let (Some(rect), Ok(window)) = (gizmo_rect.0, windows.single())
            && let Some(c) = window.cursor_position()
        {
            rect.contains(c)
        } else {
            false
        };
        let space = keys.pressed(KeyCode::Space);
        let z = keys.pressed(KeyCode::KeyZ);
        let blocked = egui_wants_pointer || gizmo_drag.active || cursor_over_gizmo || space || z;
        if let Some((origin, dir)) = cursor_ray(&cameras, &windows) {
            shape_input(
                &shape_options,
                &mut shape_state,
                &mut grid,
                &mut history,
                color.0,
                &mut recent,
                &keys,
                &mouse,
                origin,
                dir,
                blocked,
            );
        } else {
            let rmb = mouse.just_pressed(MouseButton::Right);
            let esc = keys.just_pressed(KeyCode::Escape);
            if (esc || rmb) && shape_state.phase.is_some() {
                shape_state.reset();
            }
        }
        return;
    }

    if egui_wants_pointer || gizmo_drag.active {
        return;
    }

    // Space held = pan modifier for the camera; suppress tool input so a pan
    // drag doesn't also paint.
    if keys.pressed(KeyCode::Space) {
        return;
    }

    // Z held = zoom modifier (handled by zoom_click_system); suppress tools.
    if keys.pressed(KeyCode::KeyZ) {
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
                    Tool::Eyedropper | Tool::Shape => {}
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
            Tool::Eyedropper | Tool::Shape => {}
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
    } else if keys.just_pressed(KeyCode::KeyS) {
        Some(Tool::Shape)
    } else {
        None
    };
    if let Some(t) = next
        && tool.current != t {
            tool.previous = tool.current;
            tool.current = t;
        }
}
