use crate::grid::{Color8, VoxelGrid};
use crate::history::History;
use crate::picking::{cursor_ray, pick, pick_with};
use crate::select::{Selection, SelectPhase, SelectState, SelectionAabb, clear_aabb, recolor_aabb};
use crate::shapes::{ShapePrimitive, ellipse_cells, extrude, line2d_cells, rect_cells};
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_egui::EguiContexts;
use std::collections::HashMap;

#[derive(SystemParam)]
pub struct SelectParams<'w> {
    pub state: ResMut<'w, SelectState>,
    pub selection: ResMut<'w, Selection>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tool {
    Brush,
    Erase,
    Paint,
    Eyedropper,
    Shape,
    Select,
    Move,
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

/// Live state for a click-drag move with `Tool::Move`. The drag is anchored
/// on a face plane just like the Select/Shape tools, so cursor motion maps
/// to integer cell offsets on the two in-plane axes. The third axis (the
/// face normal) stays fixed during a drag — use arrow keys for that.
#[derive(Resource, Default)]
pub struct MoveDragState {
    pub active: bool,
    pub anchor: Option<StrokeAnchor>,
    pub start_cell: Option<IVec3>,
    pub applied_delta: IVec3,
    /// Pre-drag occupied cells inside the selection AABB.
    pub originals: Vec<(IVec3, Color8)>,
    pub original_aabb: Option<SelectionAabb>,
    /// True when the drag started without a prior selection (clicked a bare
    /// voxel). Selection is cleared on commit so the move stays single-shot.
    pub ad_hoc: bool,
    /// What the previous frame wrote, keyed by world cell. Lets the next
    /// frame restore cells that fall out of the new write set.
    pub prev_state: HashMap<(i32, i32, i32), Option<Color8>>,
}

impl MoveDragState {
    pub fn reset(&mut self) {
        self.active = false;
        self.anchor = None;
        self.start_cell = None;
        self.applied_delta = IVec3::ZERO;
        self.originals.clear();
        self.original_aabb = None;
        self.prev_state.clear();
        self.ad_hoc = false;
    }
}

#[derive(Resource, Default)]
pub struct PointerState {
    pub stroking: bool,
    pub anchor: Option<StrokeAnchor>,
    pub last_placed: Option<IVec3>,
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

/// Zero out the component of `delta` that runs along the face-normal `axis`
/// so a drag stays on the picked face plane. When `lock_horizontal` is set
/// (Shift held), also zero the Y component so the voxel stays on the same
/// horizontal plane regardless of which face the user grabbed.
pub(crate) fn constrain_move_delta(
    delta: IVec3,
    axis: usize,
    lock_horizontal: bool,
) -> IVec3 {
    let mut out = delta;
    match axis {
        0 => out.x = 0,
        1 => out.y = 0,
        _ => out.z = 0,
    }
    if lock_horizontal {
        out.y = 0;
    }
    out
}

/// Translate a signed extrude offset into the `(count, dir_sign)` pair that
/// `shapes::extrude` expects. Offset `0` is a single on-plane slab, `+N`
/// extrudes `N` cells outward in `base_sign`, `-N` extrudes `N` cells inward.
pub(crate) fn extrude_args_from_signed_offset(offset: i32, base_sign: i32) -> (i32, i32) {
    let count = offset.unsigned_abs() as i32 + 1;
    let dir_sign = if offset >= 0 { base_sign } else { -base_sign };
    (count, dir_sign)
}

/// Signed extrude offset: cells away from the anchor plane in the normal
/// direction (positive) or back into the surface (negative). `0` means the
/// cursor is still on the anchor plane, i.e. a single-cell-deep extrude.
fn signed_offset_from_ray(
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
        return 0;
    }
    let s = (a * e - b * dd) / denom;
    let dist = s * normal_sign as f32;
    if dist >= 0.0 {
        dist.floor() as i32
    } else {
        dist.ceil() as i32
    }
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
    let (count, dir_sign) = extrude_args_from_signed_offset(state.thickness, state.normal_sign);
    let cells = extrude(&base, anchor.axis, count, dir_sign);
    history.begin();
    for cell in cells {
        if grid.in_bounds(cell) {
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
            state.thickness = 0;
        }
        Some(ShapePhase::Footprint) => {
            let Some(anchor) = state.anchor else { return; };
            if let Some(target) = anchor_target(&anchor, origin, dir) {
                state.corner2 = Some(target);
            }
            if lmb_released {
                state.phase = Some(ShapePhase::Extrude);
                state.thickness = 0;
            }
        }
        Some(ShapePhase::Extrude) => {
            let Some(anchor) = state.anchor else { return; };
            let (Some(c1), Some(c2)) = (state.corner1, state.corner2) else { return; };
            let center = footprint_center_world(c1, c2, anchor.axis, anchor.plane_world);
            state.thickness =
                signed_offset_from_ray(&anchor, state.normal_sign, center, origin, dir);
            if lmb_just && !blocked {
                shape_commit(options, state, grid, history, color, recent);
            }
        }
    }
}

fn select_commit(state: &mut SelectState, selection: &mut Selection) {
    if let Some(aabb) = crate::select::in_progress_aabb(state) {
        selection.aabb = Some(aabb);
    }
    state.reset();
}

fn select_input(
    state: &mut SelectState,
    selection: &mut Selection,
    keys: &ButtonInput<KeyCode>,
    mouse: &ButtonInput<MouseButton>,
    grid: &VoxelGrid,
    origin: Vec3,
    dir: Vec3,
    blocked: bool,
) {
    let lmb_just = mouse.just_pressed(MouseButton::Left);
    let lmb_released = mouse.just_released(MouseButton::Left);
    let rmb_just = mouse.just_pressed(MouseButton::Right);
    let esc = keys.just_pressed(KeyCode::Escape);

    if esc || rmb_just {
        if state.phase != SelectPhase::Idle {
            state.reset();
        } else {
            selection.aabb = None;
        }
        return;
    }

    match state.phase {
        SelectPhase::Idle => {
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
            state.phase = SelectPhase::Footprint;
            state.anchor = Some(anchor);
            state.normal_sign = sign;
            state.corner1 = Some(start_cell);
            state.corner2 = Some(start_cell);
            state.thickness = 0;
        }
        SelectPhase::Footprint => {
            let Some(anchor) = state.anchor else { return; };
            if let Some(target) = anchor_target(&anchor, origin, dir) {
                state.corner2 = Some(target);
            }
            if lmb_released {
                state.phase = SelectPhase::Extrude;
                state.thickness = 0;
            }
        }
        SelectPhase::Extrude => {
            let Some(anchor) = state.anchor else { return; };
            let (Some(c1), Some(c2)) = (state.corner1, state.corner2) else { return; };
            let center = footprint_center_world(c1, c2, anchor.axis, anchor.plane_world);
            state.thickness =
                signed_offset_from_ray(&anchor, state.normal_sign, center, origin, dir);
            if lmb_just && !blocked {
                select_commit(state, selection);
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
    select_params: SelectParams,
    gizmo_drag: Res<crate::gizmo::GizmoDrag>,
    gizmo_rect: Res<crate::gizmo::GizmoRect>,
) {
    let SelectParams { state: mut select_state, mut selection } = select_params;
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
    }

    if tool.current != Tool::Shape && shape_state.phase.is_some() {
        shape_state.reset();
    }
    if tool.current != Tool::Select && select_state.phase != SelectPhase::Idle {
        select_state.reset();
    }

    if tool.current == Tool::Select {
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
            select_input(
                &mut select_state,
                &mut selection,
                &keys,
                &mouse,
                &grid,
                origin,
                dir,
                blocked,
            );
        } else {
            let rmb = mouse.just_pressed(MouseButton::Right);
            let esc = keys.just_pressed(KeyCode::Escape);
            if esc || rmb {
                if select_state.phase != SelectPhase::Idle {
                    select_state.reset();
                } else {
                    selection.aabb = None;
                }
            }
        }
        return;
    }

    // Move tool handled by `move_drag_system` (mouse drag) plus
    // `move_selection_keys_system` (arrow-key nudge). Suppress paint clicks.
    if tool.current == Tool::Move {
        return;
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
            if !grid.in_bounds(target) {
                return;
            }
            history.begin();
            for cell in line3d(from, target) {
                if !grid.in_bounds(cell) {
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
                    Tool::Eyedropper | Tool::Shape | Tool::Select | Tool::Move => {}
                }
            }
            history.end();
            state.last_placed = Some(target);
            recent.push(color.0);
            return;
        }

    // Paint/Erase on a voxel inside the active selection → operate on the
    // whole selection in a single history stroke. Click outside the selection
    // falls through to normal single-cell stroke behavior.
    if lmb_just
        && matches!(tool.current, Tool::Paint | Tool::Erase)
        && let Some(aabb) = selection.aabb
        && let Some(hit) = pick(&grid, origin, dir)
        && hit.hit_voxel
        && aabb.contains(hit.cell)
    {
        match tool.current {
            Tool::Paint => {
                recolor_aabb(&mut grid, &mut history, &aabb, color.0);
                recent.push(color.0);
            }
            Tool::Erase => clear_aabb(&mut grid, &mut history, &aabb),
            _ => {}
        }
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
        recent.push(color.0);
    }

    if !state.stroking {
        return;
    }
    let Some(anchor) = state.anchor else { return; };
    let Some(anchored) = anchor_target(&anchor, origin, dir) else { return; };

    // Goxel-style: pick against the pre-stroke state, never the live grid.
    // History tracks the pre-stroke value of every cell touched this stroke,
    // so a brush placement made earlier this stroke reports as empty here —
    // can't become next frame's hit target, no runaway stacking.
    let history_ref: &History = &history;
    let grid_ref: &VoxelGrid = &grid;
    let target = {
        let read = |p: IVec3| -> Option<Color8> {
            match history_ref.pre_stroke_value(p) {
                Some(prev) => prev,
                None => grid_ref.get(p),
            }
        };
        match (tool.current, pick_with(read, grid_ref.size_i(), origin, dir)) {
            (Tool::Brush, Some(hit)) if hit.hit_voxel => hit.cell + hit.normal,
            (Tool::Erase | Tool::Paint, Some(hit)) if hit.hit_voxel => hit.cell,
            _ => anchored,
        }
    };

    if !grid.in_bounds(target) {
        return;
    }

    let path: Vec<IVec3> = match state.last_placed {
        Some(from) if from != target => line3d(from, target),
        _ => vec![target],
    };

    for cell in path {
        if !grid.in_bounds(cell) {
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
            Tool::Eyedropper | Tool::Shape | Tool::Select | Tool::Move => {}
        }
    }
}

/// Mouse drag for `Tool::Move`. Press inside a selected voxel anchors a face
/// plane (like the Select tool's footprint phase); drag projects the cursor
/// onto that plane and translates the selection contents to that integer
/// cell. Lives in its own system so the painting/picking flow in
/// `tool_input_system` stays untouched.
pub fn move_drag_system(
    mut contexts: EguiContexts,
    mouse: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    cameras: Query<(&Camera, &GlobalTransform), With<bevy_panorbit_camera::PanOrbitCamera>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut grid: ResMut<VoxelGrid>,
    mut history: ResMut<History>,
    tool: Res<ToolState>,
    mut selection: ResMut<Selection>,
    mut drag: ResMut<MoveDragState>,
    gizmo_drag: Res<crate::gizmo::GizmoDrag>,
    gizmo_rect: Res<crate::gizmo::GizmoRect>,
) {
    // Bail if tool switched away mid-drag — revert any partial writes.
    if tool.current != Tool::Move {
        if drag.active {
            history.abort(&mut grid);
            selection.aabb = if drag.ad_hoc { None } else { drag.original_aabb };
            drag.reset();
        }
        return;
    }

    let egui_wants = contexts
        .ctx_mut()
        .map(|c| c.is_pointer_over_area() || c.wants_pointer_input())
        .unwrap_or(false);
    let cursor_over_gizmo = if let (Some(rect), Ok(window)) = (gizmo_rect.0, windows.single())
        && let Some(c) = window.cursor_position()
    {
        rect.contains(c)
    } else {
        false
    };
    let blocked = egui_wants
        || gizmo_drag.active
        || cursor_over_gizmo
        || keys.pressed(KeyCode::Space)
        || keys.pressed(KeyCode::KeyZ);

    let lmb_just = mouse.just_pressed(MouseButton::Left);
    let lmb_pressed = mouse.pressed(MouseButton::Left);
    let rmb_just = mouse.just_pressed(MouseButton::Right);
    let esc = keys.just_pressed(KeyCode::Escape);

    // Cancel mid-drag → revert and abandon the stroke.
    if drag.active && (esc || rmb_just) {
        history.abort(&mut grid);
        selection.aabb = if drag.ad_hoc { None } else { drag.original_aabb };
        drag.reset();
        return;
    }

    // Drag start: click on a voxel inside the selection, or any voxel when
    // no selection exists (ad-hoc single-voxel move).
    if !drag.active {
        if !lmb_just || blocked {
            return;
        }
        let Some((origin, dir)) = cursor_ray(&cameras, &windows) else { return; };
        let Some(hit) = pick(&grid, origin, dir) else { return; };
        if !hit.hit_voxel {
            return;
        }
        let (effective_aabb, ad_hoc) = match selection.aabb {
            Some(aabb) if aabb.contains(hit.cell) => (aabb, false),
            Some(_) => return, // Click outside an existing selection: ignore.
            None => (SelectionAabb::from_corners(hit.cell, hit.cell), true),
        };
        let axis = axis_of_normal(hit.normal);
        let n_arr = hit.normal.to_array();
        let sign = if n_arr[axis] >= 0 { 1 } else { -1 };
        let cell_arr = hit.cell.to_array();
        let plane_world = cell_arr[axis] as f32 + if sign > 0 { 1.0 } else { 0.0 };
        let anchor = StrokeAnchor { axis, plane_world, target_layer: cell_arr[axis] };
        let start_cell = anchor_target(&anchor, origin, dir).unwrap_or(hit.cell);

        let originals: Vec<(IVec3, Color8)> = effective_aabb
            .iter_cells()
            .filter_map(|p| grid.get(p).map(|c| (p, c)))
            .collect();
        if originals.is_empty() {
            return;
        }
        history.begin();
        drag.active = true;
        drag.anchor = Some(anchor);
        drag.start_cell = Some(start_cell);
        drag.applied_delta = IVec3::ZERO;
        drag.originals = originals;
        drag.original_aabb = Some(effective_aabb);
        drag.prev_state.clear();
        drag.ad_hoc = ad_hoc;
        // Show the ad-hoc 1-cell selection while dragging so the overlay
        // tracks the moving voxel.
        if ad_hoc {
            selection.aabb = Some(effective_aabb);
        }
        return;
    }

    // Drag in progress.
    if lmb_pressed {
        let Some((origin, dir)) = cursor_ray(&cameras, &windows) else { return; };
        let (Some(anchor), Some(start), Some(orig_aabb)) =
            (drag.anchor, drag.start_cell, drag.original_aabb)
        else {
            return;
        };
        let Some(target) = anchor_target(&anchor, origin, dir) else { return; };
        let shift = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
        let new_delta = constrain_move_delta(target - start, anchor.axis, shift);
        if new_delta == drag.applied_delta {
            return;
        }
        // Refuse shifts that would leave the grid; the cursor can roam past
        // the edge without dragging voxels into oblivion.
        let s = grid.size_i();
        let new_min = orig_aabb.min + new_delta;
        let new_max = orig_aabb.max + new_delta;
        if new_min.x < 0 || new_min.y < 0 || new_min.z < 0
            || new_max.x >= s || new_max.y >= s || new_max.z >= s
        {
            return;
        }

        // Collision check: refuse shifts that would land a moving voxel on
        // a pre-stroke voxel that isn't part of the moving set. Keeps the
        // selection from devouring obstacles in its path.
        let originals_set: std::collections::HashSet<(i32, i32, i32)> = drag
            .originals
            .iter()
            .map(|(p, _)| (p.x, p.y, p.z))
            .collect();
        let mut collides = false;
        for (src, _) in &drag.originals {
            let dst = *src + new_delta;
            let key = (dst.x, dst.y, dst.z);
            if originals_set.contains(&key) {
                continue;
            }
            let pre = match history.pre_stroke_value(dst) {
                Some(v) => v,
                None => grid.get(dst),
            };
            if pre.is_some() {
                collides = true;
                break;
            }
        }
        if collides {
            return;
        }

        // Frame's desired write set: sources cleared, destinations colored.
        let mut new_state: HashMap<(i32, i32, i32), Option<Color8>> = HashMap::new();
        for (src, _) in &drag.originals {
            new_state.insert((src.x, src.y, src.z), None);
        }
        for (src, color) in &drag.originals {
            let dst = *src + new_delta;
            new_state.insert((dst.x, dst.y, dst.z), Some(*color));
        }

        // Restore any cell written last frame that's no longer in the set
        // (e.g. a destination from the old delta the user dragged away from).
        let prev = std::mem::take(&mut drag.prev_state);
        for key in prev.keys() {
            if !new_state.contains_key(key) {
                let p = IVec3::new(key.0, key.1, key.2);
                let restore = history.pre_stroke_value(p).flatten();
                history.record(&mut grid, p, restore);
            }
        }
        for (key, value) in &new_state {
            let p = IVec3::new(key.0, key.1, key.2);
            history.record(&mut grid, p, *value);
        }
        drag.prev_state = new_state;
        drag.applied_delta = new_delta;
        selection.aabb = Some(SelectionAabb {
            min: orig_aabb.min + new_delta,
            max: orig_aabb.max + new_delta,
        });
        return;
    }

    // LMB released → commit the move.
    history.end();
    if drag.ad_hoc {
        selection.aabb = None;
    }
    drag.reset();
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
    } else if keys.just_pressed(KeyCode::KeyM) {
        Some(Tool::Select)
    } else if keys.just_pressed(KeyCode::KeyV) {
        Some(Tool::Move)
    } else {
        None
    };
    if let Some(t) = next
        && tool.current != t {
            tool.previous = tool.current;
            tool.current = t;
        }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shapes::{extrude, rect_cells};
    use std::collections::HashSet;

    fn cells_set(cells: Vec<IVec3>) -> HashSet<(i32, i32, i32)> {
        cells.into_iter().map(|c| (c.x, c.y, c.z)).collect()
    }

    #[test]
    fn constrain_move_delta_zeros_face_normal_axis() {
        let d = IVec3::new(3, 5, -2);
        assert_eq!(constrain_move_delta(d, 0, false), IVec3::new(0, 5, -2));
        assert_eq!(constrain_move_delta(d, 1, false), IVec3::new(3, 0, -2));
        assert_eq!(constrain_move_delta(d, 2, false), IVec3::new(3, 5, 0));
    }

    #[test]
    fn constrain_move_delta_shift_locks_y_for_horizontal_plane() {
        // Side face (X axis): in-plane is YZ. Shift drops Y → motion only on Z.
        let d = IVec3::new(99, 4, -3);
        assert_eq!(constrain_move_delta(d, 0, true), IVec3::new(0, 0, -3));
    }

    #[test]
    fn constrain_move_delta_shift_on_top_face_is_noop_for_y() {
        // Top face already has Y zeroed; Shift just leaves things alone.
        let d = IVec3::new(3, 8, -2);
        assert_eq!(constrain_move_delta(d, 1, true), IVec3::new(3, 0, -2));
    }

    #[test]
    fn constrain_move_delta_shift_on_front_face_drops_y() {
        // Front face (Z axis): in-plane is XY. Shift drops Y → motion only on X.
        let d = IVec3::new(4, -5, 99);
        assert_eq!(constrain_move_delta(d, 2, true), IVec3::new(4, 0, 0));
    }

    #[test]
    fn extrude_args_zero_offset_is_single_slab_in_normal_direction() {
        let (count, dir) = extrude_args_from_signed_offset(0, 1);
        assert_eq!((count, dir), (1, 1));
        let (count, dir) = extrude_args_from_signed_offset(0, -1);
        assert_eq!((count, dir), (1, -1));
    }

    #[test]
    fn extrude_args_positive_offset_grows_outward() {
        assert_eq!(extrude_args_from_signed_offset(3, 1), (4, 1));
        assert_eq!(extrude_args_from_signed_offset(3, -1), (4, -1));
    }

    #[test]
    fn extrude_args_negative_offset_flips_direction() {
        assert_eq!(extrude_args_from_signed_offset(-3, 1), (4, -1));
        assert_eq!(extrude_args_from_signed_offset(-3, -1), (4, 1));
    }

    #[test]
    fn shape_extrude_negative_offset_carves_into_surface() {
        // Footprint on a horizontal slab at y=5, normal +Y.
        let c1 = IVec3::new(0, 5, 0);
        let c2 = IVec3::new(1, 5, 1);
        let base = rect_cells(c1, c2, 1, true);
        // Negative offset → cells extend toward y < 5.
        let (count, dir) = extrude_args_from_signed_offset(-2, 1);
        let cells = cells_set(extrude(&base, 1, count, dir));
        assert!(cells.contains(&(0, 5, 0)));
        assert!(cells.contains(&(0, 4, 0)));
        assert!(cells.contains(&(0, 3, 0)));
        assert!(!cells.contains(&(0, 6, 0)));
        assert!(!cells.contains(&(0, 2, 0)));
    }

    #[test]
    fn shape_extrude_positive_offset_extends_in_normal_direction() {
        let c1 = IVec3::new(0, 5, 0);
        let c2 = IVec3::new(1, 5, 1);
        let base = rect_cells(c1, c2, 1, true);
        let (count, dir) = extrude_args_from_signed_offset(2, 1);
        let cells = cells_set(extrude(&base, 1, count, dir));
        assert!(cells.contains(&(0, 5, 0)));
        assert!(cells.contains(&(0, 6, 0)));
        assert!(cells.contains(&(0, 7, 0)));
        assert!(!cells.contains(&(0, 4, 0)));
        assert!(!cells.contains(&(0, 8, 0)));
    }
}
