use crate::grid::{Color8, VoxelGrid};
use crate::history::History;
use crate::picking::{Hit, cursor_ray, pick, pick_with};
use crate::select::{
    DOUBLE_CLICK_SECS, SelectPhase, SelectState, Selection, SelectionAabb, clear_selection,
    connected_same_color, fill_region, recolor_selection,
};
use crate::shapes::{
    ShapePrimitive, ellipse_cells, ellipsoid_cells, extrude, line2d_cells, rect_cells,
};
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

#[derive(SystemParam)]
pub struct InputGates<'w> {
    pub gizmo_drag: Res<'w, crate::gizmo::GizmoDrag>,
    pub gizmo_rect: Res<'w, crate::gizmo::GizmoRect>,
    pub flyby: Res<'w, crate::camera::FlybyState>,
}

#[derive(SystemParam)]
pub struct Pointer<'w> {
    pub mouse: Res<'w, ButtonInput<MouseButton>>,
    pub keys: Res<'w, ButtonInput<KeyCode>>,
    pub time: Res<'w, Time>,
}

#[derive(SystemParam)]
pub struct Viewport<'w, 's> {
    pub cameras: Query<
        'w,
        's,
        (&'static Camera, &'static GlobalTransform),
        With<bevy_panorbit_camera::PanOrbitCamera>,
    >,
    pub windows: Query<'w, 's, &'static Window, With<PrimaryWindow>>,
}

#[derive(SystemParam)]
pub struct ShapeInput<'w> {
    pub options: Res<'w, ShapeOptions>,
    pub state: ResMut<'w, ShapeState>,
}

#[derive(SystemParam)]
pub struct MoveEdit<'w> {
    pub grid: ResMut<'w, VoxelGrid>,
    pub history: ResMut<'w, History>,
    pub tool: Res<'w, ToolState>,
    pub selection: ResMut<'w, Selection>,
    pub drag: ResMut<'w, MoveDragState>,
}

#[derive(SystemParam)]
pub struct ToolEdit<'w> {
    pub grid: ResMut<'w, VoxelGrid>,
    pub history: ResMut<'w, History>,
    pub tool: ResMut<'w, ToolState>,
    pub color: ResMut<'w, CurrentColor>,
    pub extras: Res<'w, ExtraColors>,
    pub recent: ResMut<'w, RecentColors>,
    pub state: ResMut<'w, PointerState>,
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
        Self {
            current: Tool::Brush,
            previous: Tool::Brush,
        }
    }
}

#[derive(Resource)]
pub struct CurrentColor(pub Color8);

impl Default for CurrentColor {
    fn default() -> Self {
        Self([200, 200, 200, 255])
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

/// Additional swatches shift-clicked alongside `CurrentColor`. Together with
/// the primary they form a sampling pool (see [`color_pool`]) — paint and
/// shape commits draw one color per voxel from that pool. Session-scoped:
/// not persisted.
#[derive(Resource, Default)]
pub struct ExtraColors(pub Vec<Color8>);

impl ExtraColors {
    pub fn contains(&self, c: Color8) -> bool {
        self.0.contains(&c)
    }
    pub fn clear(&mut self) {
        self.0.clear();
    }
    /// Add `c` if absent, remove if present. Returns true when `c` is in the
    /// set after the call.
    pub fn toggle(&mut self, c: Color8) -> bool {
        if let Some(idx) = self.0.iter().position(|x| *x == c) {
            self.0.remove(idx);
            false
        } else {
            self.0.push(c);
            true
        }
    }
}

/// Build the sampling pool from primary + extras. Primary always first; any
/// extras matching primary are dropped so each pool color is unique.
pub fn color_pool(primary: Color8, extras: &[Color8]) -> Vec<Color8> {
    let mut out = Vec::with_capacity(1 + extras.len());
    out.push(primary);
    for c in extras {
        if !out.contains(c) {
            out.push(*c);
        }
    }
    out
}

/// Deterministic per-voxel color from `pool`. Same `pos` always returns the
/// same color so preview and commit agree (WYSIWYG). Empty pool would panic
/// — callers must ensure pool always carries the primary.
pub fn sample_color(pos: IVec3, pool: &[Color8]) -> Color8 {
    if pool.len() == 1 {
        return pool[0];
    }
    let h = (pos.x as u32).wrapping_mul(73856093)
        ^ (pos.y as u32).wrapping_mul(19349663)
        ^ (pos.z as u32).wrapping_mul(83492791);
    pool[(h as usize) % pool.len()]
}

/// Pure click-handler for palette swatches. Plain click sets primary and
/// clears extras; shift-click toggles a non-primary color in/out of extras
/// (clicking the primary itself is a no-op). Returns the new primary.
pub fn apply_swatch_click(
    shift: bool,
    clicked: Color8,
    primary: Color8,
    extras: &mut ExtraColors,
) -> Color8 {
    if shift {
        if clicked == primary {
            return primary;
        }
        extras.toggle(clicked);
        primary
    } else {
        extras.clear();
        clicked
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
    /// `originals` cells as a hash set keyed by tuple. Cached so the collision
    /// check in each drag frame is O(1) per moving cell instead of rebuilding
    /// a set every frame.
    pub originals_set: std::collections::HashSet<(i32, i32, i32)>,
    pub original_aabb: Option<SelectionAabb>,
    /// Snapshot of the selection's cell mask at drag start. Carries the per-
    /// cell selection through the move so cancel/abort restores it intact and
    /// each frame shifts it alongside the AABB.
    pub original_cells: Option<std::collections::HashSet<IVec3>>,
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
        self.originals_set.clear();
        self.original_aabb = None;
        self.original_cells = None;
        self.prev_state.clear();
        self.ad_hoc = false;
    }
}

#[derive(Resource, Default)]
pub struct PointerState {
    pub stroking: bool,
    pub anchor: Option<StrokeAnchor>,
    pub last_placed: Option<IVec3>,
    /// First cell of the active stroke. While Shift is held mid-stroke, brush
    /// placement locks to a single in-plane axis measured from this origin,
    /// producing straight rows/columns. `None` outside a stroke.
    pub stroke_origin: Option<IVec3>,
    /// Distinct colors written during the current stroke. Flushed into
    /// `RecentColors` on stroke end so the Recent grid doesn't reshuffle on
    /// every painted voxel mid-drag.
    pub stroke_used: Vec<Color8>,
    /// Cell hit by the previous Paint LMB-press and its timestamp — used to
    /// detect a double-click (same cell within `DOUBLE_CLICK_SECS`) so Paint can
    /// flood-fill the connected region. Mirrors `SelectState`'s scheme.
    pub last_press_cell: Option<IVec3>,
    pub last_press_secs: f64,
    /// Color of `last_press_cell` *before* the first click recolored it. The
    /// double-click flood matches against this so it covers the original region,
    /// not the single voxel the first click already changed.
    pub last_press_color: Option<Color8>,
}

#[derive(Resource, Clone, Copy, Default)]
pub struct ShapeOptions {
    pub primitive: ShapePrimitive,
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
            if p1 >= 0 {
                cur.y += s.y;
                p1 -= 2 * dx;
            }
            if p2 >= 0 {
                cur.z += s.z;
                p2 -= 2 * dx;
            }
            cur.x += s.x;
            p1 += 2 * dy;
            p2 += 2 * dz;
            out.push(cur);
        }
    } else if dy >= dx && dy >= dz {
        let (mut p1, mut p2) = (2 * dx - dy, 2 * dz - dy);
        while cur.y != b.y {
            if p1 >= 0 {
                cur.x += s.x;
                p1 -= 2 * dy;
            }
            if p2 >= 0 {
                cur.z += s.z;
                p2 -= 2 * dy;
            }
            cur.y += s.y;
            p1 += 2 * dx;
            p2 += 2 * dz;
            out.push(cur);
        }
    } else {
        let (mut p1, mut p2) = (2 * dy - dz, 2 * dx - dz);
        while cur.z != b.z {
            if p1 >= 0 {
                cur.y += s.y;
                p1 -= 2 * dz;
            }
            if p2 >= 0 {
                cur.x += s.x;
                p2 -= 2 * dz;
            }
            cur.z += s.z;
            p1 += 2 * dy;
            p2 += 2 * dx;
            out.push(cur);
        }
    }
    out
}

fn axis_of_normal(n: IVec3) -> usize {
    if n.x != 0 {
        0
    } else if n.y != 0 {
        1
    } else {
        2
    }
}

/// Which integer layer along the face-normal axis a stroke anchors on. `Picked`
/// is the layer of the hit voxel itself (Select, Erase, Move); `Adjacent` is
/// the layer one step along the face normal (Brush, Shape).
enum AnchorTarget {
    Picked,
    Adjacent,
}

/// Build a face-plane `StrokeAnchor` from a pick hit and return the face
/// normal's sign along the anchor axis. Tools that track `normal_sign`
/// (Shape, Select) use the second return value; tools that don't (Brush,
/// Erase, Move) can ignore it.
fn stroke_anchor_from_hit(hit: &Hit, target: AnchorTarget) -> (StrokeAnchor, i32) {
    let axis = axis_of_normal(hit.normal);
    let n_arr = hit.normal.to_array();
    let sign = if n_arr[axis] >= 0 { 1 } else { -1 };
    let cell_arr = hit.cell.to_array();
    let plane_world = cell_arr[axis] as f32 + if sign > 0 { 1.0 } else { 0.0 };
    let target_layer = match target {
        AnchorTarget::Picked => cell_arr[axis],
        AnchorTarget::Adjacent => cell_arr[axis] + n_arr[axis],
    };
    (
        StrokeAnchor {
            axis,
            plane_world,
            target_layer,
        },
        sign,
    )
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
pub(crate) fn constrain_move_delta(delta: IVec3, axis: usize, lock_horizontal: bool) -> IVec3 {
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

/// Snap `target` so the footprint defined by `(c1, target)` keeps a uniform
/// aspect ratio. For rectangle/ellipse: square footprint (both 2D-plane axes
/// equal in magnitude). For line: snap to the nearest 45° direction in the
/// face plane.
pub(crate) fn constrain_shape_corner2(
    primitive: ShapePrimitive,
    axis: usize,
    c1: IVec3,
    target: IVec3,
) -> IVec3 {
    let u_axis = (axis + 1) % 3;
    let v_axis = (axis + 2) % 3;
    let c1a = c1.to_array();
    let ta = target.to_array();
    let du = ta[u_axis] - c1a[u_axis];
    let dv = ta[v_axis] - c1a[v_axis];
    let (nu, nv) = match primitive {
        ShapePrimitive::Rectangle | ShapePrimitive::Ellipse | ShapePrimitive::Sphere => {
            let m = du.abs().max(dv.abs());
            let su = if du == 0 { 1 } else { du.signum() };
            let sv = if dv == 0 { 1 } else { dv.signum() };
            (su * m, sv * m)
        }
        ShapePrimitive::Line => {
            let abs_du = du.abs();
            let abs_dv = dv.abs();
            if abs_du == 0 && abs_dv == 0 {
                (0, 0)
            } else {
                let lo = abs_du.min(abs_dv) as f32;
                let hi = abs_du.max(abs_dv) as f32;
                let ratio = lo / hi;
                if ratio < 0.4142 {
                    if abs_du >= abs_dv { (du, 0) } else { (0, dv) }
                } else {
                    let m = abs_du.max(abs_dv);
                    (du.signum() * m, dv.signum() * m)
                }
            }
        }
    };
    let mut out = [0i32; 3];
    out[axis] = c1a[axis];
    out[u_axis] = c1a[u_axis] + nu;
    out[v_axis] = c1a[v_axis] + nv;
    IVec3::from_array(out)
}

/// Lock `target` to a single in-plane axis measured from `origin`, snapping the
/// weaker of the two in-plane axes back to the origin. `plane_axis` is the
/// stroke anchor's fixed (face-normal) axis, already pinned. Used while Shift
/// is held mid brush stroke so placement runs straight along one axis.
fn axis_lock(origin: IVec3, target: IVec3, plane_axis: usize) -> IVec3 {
    let (u_axis, v_axis) = crate::shapes::other_axes(plane_axis);
    let o = origin.to_array();
    let mut out = target.to_array();
    let du = (out[u_axis] - o[u_axis]).abs();
    let dv = (out[v_axis] - o[v_axis]).abs();
    if du >= dv {
        out[v_axis] = o[v_axis];
    } else {
        out[u_axis] = o[u_axis];
    }
    IVec3::from_array(out)
}

fn shape_commit(
    options: &ShapeOptions,
    state: &mut ShapeState,
    grid: &mut VoxelGrid,
    history: &mut History,
    pool: &[Color8],
    recent: &mut RecentColors,
) {
    let (Some(anchor), Some(c1), Some(c2)) = (state.anchor, state.corner1, state.corner2) else {
        state.reset();
        return;
    };
    let cells = if options.primitive == ShapePrimitive::Sphere {
        ellipsoid_cells(c1, c2, anchor.axis, state.thickness, state.normal_sign)
    } else {
        let base = match options.primitive {
            ShapePrimitive::Rectangle => rect_cells(c1, c2, anchor.axis, true),
            ShapePrimitive::Ellipse => ellipse_cells(c1, c2, anchor.axis, true),
            ShapePrimitive::Line => line2d_cells(c1, c2, anchor.axis),
            ShapePrimitive::Sphere => unreachable!(),
        };
        let (count, dir_sign) = extrude_args_from_signed_offset(state.thickness, state.normal_sign);
        extrude(&base, anchor.axis, count, dir_sign)
    };
    history.begin();
    let mut used: Vec<Color8> = Vec::new();
    for cell in cells {
        if grid.in_bounds(cell) {
            let c = sample_color(cell, pool);
            history.record(grid, cell, Some(c));
            if !used.contains(&c) {
                used.push(c);
            }
        }
    }
    history.end();
    for c in used {
        recent.push(c);
    }
    state.reset();
}

#[allow(clippy::too_many_arguments)]
fn shape_input(
    options: &ShapeOptions,
    state: &mut ShapeState,
    grid: &mut VoxelGrid,
    history: &mut History,
    pool: &[Color8],
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
            let Some(hit) = pick(grid, origin, dir) else {
                return;
            };
            let (anchor, sign) = stroke_anchor_from_hit(&hit, AnchorTarget::Adjacent);
            let start_cell = anchor_target(&anchor, origin, dir).unwrap_or(hit.cell + hit.normal);
            state.phase = Some(ShapePhase::Footprint);
            state.anchor = Some(anchor);
            state.normal_sign = sign;
            state.corner1 = Some(start_cell);
            state.corner2 = Some(start_cell);
            state.thickness = 0;
        }
        Some(ShapePhase::Footprint) => {
            let Some(anchor) = state.anchor else {
                return;
            };
            if let Some(target) = anchor_target(&anchor, origin, dir) {
                let shift = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
                let c2 = if shift && let Some(c1) = state.corner1 {
                    constrain_shape_corner2(options.primitive, anchor.axis, c1, target)
                } else {
                    target
                };
                state.corner2 = Some(c2);
            }
            if lmb_released {
                state.phase = Some(ShapePhase::Extrude);
                state.thickness = 0;
            }
        }
        Some(ShapePhase::Extrude) => {
            let Some(anchor) = state.anchor else {
                return;
            };
            let (Some(c1), Some(c2)) = (state.corner1, state.corner2) else {
                return;
            };
            let center = footprint_center_world(c1, c2, anchor.axis, anchor.plane_world);
            state.thickness =
                signed_offset_from_ray(&anchor, state.normal_sign, center, origin, dir);
            if lmb_just && !blocked {
                shape_commit(options, state, grid, history, pool, recent);
            }
        }
    }
}

fn select_commit(state: &mut SelectState, selection: &mut Selection) {
    if let Some(aabb) = crate::select::in_progress_aabb(state) {
        selection.set_aabb(aabb);
    }
    state.reset();
}

#[allow(clippy::too_many_arguments)]
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
            // Abort in-progress footprint/extrude.
            state.reset();
            return;
        }
        // Idle: Esc deselects, but RMB is reserved for camera orbit/pan —
        // don't clear the committed selection when the user rotates the view.
        if esc {
            selection.clear();
        }
        return;
    }

    match state.phase {
        SelectPhase::Idle => {
            if !lmb_just || blocked {
                return;
            }
            let Some(hit) = pick(grid, origin, dir) else {
                return;
            };
            // Select targets the picked voxel itself, not the adjacent empty
            // cell — clicking a voxel should select that voxel.
            let (anchor, sign) = stroke_anchor_from_hit(&hit, AnchorTarget::Picked);
            let start_cell = anchor_target(&anchor, origin, dir).unwrap_or(hit.cell);
            state.phase = SelectPhase::Footprint;
            state.anchor = Some(anchor);
            state.normal_sign = sign;
            state.corner1 = Some(start_cell);
            state.corner2 = Some(start_cell);
            state.thickness = 0;
        }
        SelectPhase::Footprint => {
            let Some(anchor) = state.anchor else {
                return;
            };
            if let Some(target) = anchor_target(&anchor, origin, dir) {
                state.corner2 = Some(target);
            }
            if lmb_released {
                state.phase = SelectPhase::Extrude;
                state.thickness = 0;
            }
        }
        SelectPhase::Extrude => {
            let Some(anchor) = state.anchor else {
                return;
            };
            let (Some(c1), Some(c2)) = (state.corner1, state.corner2) else {
                return;
            };
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
    pointer: Pointer,
    viewport: Viewport,
    edit: ToolEdit,
    shape: ShapeInput,
    select_params: SelectParams,
    gates: InputGates,
) {
    let ToolEdit {
        mut grid,
        mut history,
        mut tool,
        mut color,
        extras,
        mut recent,
        mut state,
    } = edit;
    let Pointer { mouse, keys, time } = pointer;
    let Viewport { cameras, windows } = viewport;
    let ShapeInput {
        options: shape_options,
        state: mut shape_state,
    } = shape;
    let InputGates {
        gizmo_drag,
        gizmo_rect,
        flyby,
    } = gates;
    if flyby.active {
        if state.stroking {
            history.abort(&mut grid);
            state.stroking = false;
            state.anchor = None;
            state.stroke_origin = None;
            state.stroke_used.clear();
        }
        return;
    }
    let SelectParams {
        state: mut select_state,
        mut selection,
    } = select_params;
    let egui_wants_pointer = contexts
        .ctx_mut()
        .map(|c| c.is_pointer_over_area() || c.wants_pointer_input())
        .unwrap_or(false);

    let lmb_pressed = mouse.pressed(MouseButton::Left);
    let lmb_just = mouse.just_pressed(MouseButton::Left);
    let lmb_released = mouse.just_released(MouseButton::Left);
    let pool = color_pool(color.0, &extras.0);

    // End stroke whenever LMB is released, regardless of where pointer is now.
    if state.stroking && (lmb_released || !lmb_pressed) {
        history.end();
        state.stroking = false;
        state.anchor = None;
        state.stroke_origin = None;
        for c in state.stroke_used.drain(..) {
            recent.push(c);
        }
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
            // Double-click on a voxel selects every 6-connected same-color
            // voxel touching it. Detect before delegating to `select_input`
            // because the second LMB-press would otherwise commit the
            // in-progress extrude from the first click.
            if lmb_just && !blocked {
                let pick_cell =
                    pick(&grid, origin, dir).and_then(|h| h.hit_voxel.then_some(h.cell));
                let now = time.elapsed_secs_f64();
                let is_double = pick_cell.is_some()
                    && select_state.last_press_cell == pick_cell
                    && (now - select_state.last_press_secs) < DOUBLE_CLICK_SECS;
                select_state.last_press_secs = now;
                select_state.last_press_cell = pick_cell;
                if is_double && let Some(c) = pick_cell {
                    let cells = connected_same_color(&grid, c);
                    if !cells.is_empty() {
                        selection.set_cells(cells.into_iter().collect());
                        select_state.reset();
                        return;
                    }
                }
            }
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
                &pool,
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
        && rect.contains(c)
    {
        return;
    }

    let Some((origin, dir)) = cursor_ray(&cameras, &windows) else {
        return;
    };

    let alt = keys.pressed(KeyCode::AltLeft) || keys.pressed(KeyCode::AltRight);

    // Single-click tools (eyedropper).
    if lmb_just && tool.current == Tool::Eyedropper {
        if let Some(hit) = pick(&grid, origin, dir)
            && hit.hit_voxel
            && let Some(c) = grid.get(hit.cell)
        {
            color.0 = c;
            recent.push(c);
        }
        // Stay in eyedropper while Alt is held; otherwise restore previous tool.
        if !alt {
            tool.current = tool.previous;
        }
        return;
    }

    // Paint click-only paths, handled ahead of the anchor/stroke machinery so
    // they never enter drag mode. A plain single click on a voxel falls through
    // to the per-voxel stroke below (freehand recolor).
    if lmb_just && tool.current == Tool::Paint {
        // Active selection: a click fills the whole selection (per-cell sampled),
        // one history stroke. Reachable from the keyboard via `F` too.
        if selection.aabb.is_some() {
            let used = recolor_selection(&mut grid, &mut history, &selection, &pool);
            for c in used {
                recent.push(c);
            }
            return;
        }
        // Double-click a voxel → flood-fill its 6-connected same-color region
        // (mirrors the Select tool's double-click-to-pick-region gesture). The
        // first click recolored the seed voxel; the flood re-covers it as a
        // no-op, so the region ends uniformly the current color.
        if let Some(hit) = pick(&grid, origin, dir)
            && hit.hit_voxel
        {
            let now = time.elapsed_secs_f64();
            let is_double = state.last_press_cell == Some(hit.cell)
                && (now - state.last_press_secs) < DOUBLE_CLICK_SECS;
            if is_double && let Some(orig) = state.last_press_color {
                // Match the region by the seed's *pre-first-click* color: the
                // first click already recolored the seed, so a plain
                // `fill_connected` would only see the new color and spread
                // nowhere.
                let used = fill_region(&mut grid, &mut history, hit.cell, orig, &pool);
                state.last_press_cell = None;
                for c in used {
                    recent.push(c);
                }
                return;
            }
            // First click: remember the seed and its color before the stroke
            // machinery below recolors it, so a follow-up click can flood.
            state.last_press_secs = now;
            state.last_press_cell = Some(hit.cell);
            state.last_press_color = grid.get(hit.cell);
        }
    }

    let shift = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);

    // Shift + click: draw a 3D line from the last placed voxel to the new target.
    // Performs a single-frame stroke; does not enter drag mode.
    if lmb_just
        && shift
        && let Some(from) = state.last_placed
    {
        let Some(hit) = pick(&grid, origin, dir) else {
            return;
        };
        let target = match tool.current {
            Tool::Brush => hit.cell + hit.normal,
            Tool::Erase | Tool::Paint if hit.hit_voxel => hit.cell,
            _ => return,
        };
        if !grid.in_bounds(target) {
            return;
        }
        history.begin();
        let mut used: Vec<Color8> = Vec::new();
        for cell in line3d(from, target) {
            if !grid.in_bounds(cell) {
                continue;
            }
            match tool.current {
                Tool::Brush => {
                    let c = sample_color(cell, &pool);
                    history.record(&mut grid, cell, Some(c));
                    if !used.contains(&c) {
                        used.push(c);
                    }
                }
                Tool::Erase => {
                    if grid.get(cell).is_some() {
                        history.record(&mut grid, cell, None);
                    }
                }
                Tool::Paint => {
                    if grid.get(cell).is_some() {
                        let c = sample_color(cell, &pool);
                        history.record(&mut grid, cell, Some(c));
                        if !used.contains(&c) {
                            used.push(c);
                        }
                    }
                }
                Tool::Eyedropper | Tool::Shape | Tool::Select | Tool::Move => {}
            }
        }
        history.end();
        state.last_placed = Some(target);
        for c in used {
            recent.push(c);
        }
        return;
    }

    // Erase on a voxel inside the active selection → clear the whole selection
    // in a single history stroke. Click outside the selection falls through to
    // normal single-cell stroke behavior. (Recoloring a selection lives on the
    // Fill tool.)
    if lmb_just
        && tool.current == Tool::Erase
        && selection.aabb.is_some()
        && let Some(hit) = pick(&grid, origin, dir)
        && hit.hit_voxel
        && selection.contains(hit.cell)
    {
        clear_selection(&mut grid, &mut history, &selection);
        return;
    }

    // Stroke start: anchor a build plane based on the first hit, then stick to it
    // for the duration of the stroke. Prevents runaway stacking when the freshly
    // placed voxel becomes the next frame's pick target.
    if lmb_just {
        let Some(hit) = pick(&grid, origin, dir) else {
            return;
        };
        let target = match tool.current {
            Tool::Brush => AnchorTarget::Adjacent,
            _ => AnchorTarget::Picked,
        };
        let (anchor, _) = stroke_anchor_from_hit(&hit, target);
        let start_cell = match tool.current {
            Tool::Brush => hit.cell + hit.normal,
            _ => hit.cell,
        };
        history.begin();
        state.stroking = true;
        state.anchor = Some(anchor);
        state.last_placed = None;
        state.stroke_origin = Some(start_cell);
        // Recent push for the stroke happens per-voxel below so multi-color
        // pools surface every color used, not just the primary.
    }

    if !state.stroking {
        return;
    }
    let Some(anchor) = state.anchor else {
        return;
    };
    let Some(anchored) = anchor_target(&anchor, origin, dir) else {
        return;
    };

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
        match (tool.current, pick_with(read, origin, dir)) {
            (Tool::Brush, Some(hit)) if hit.hit_voxel => hit.cell + hit.normal,
            (Tool::Erase | Tool::Paint, Some(hit)) if hit.hit_voxel => hit.cell,
            _ => anchored,
        }
    };

    // Shift held mid-stroke locks placement to one in-plane axis from the
    // stroke origin, drawing a straight line; releasing it resumes free draw.
    let target = match state.stroke_origin {
        Some(o) if shift => axis_lock(o, target, anchor.axis),
        _ => target,
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
                let c = sample_color(cell, &pool);
                history.record(&mut grid, cell, Some(c));
                if !state.stroke_used.contains(&c) {
                    state.stroke_used.push(c);
                }
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
                    let c = sample_color(cell, &pool);
                    history.record(&mut grid, cell, Some(c));
                    if !state.stroke_used.contains(&c) {
                        state.stroke_used.push(c);
                    }
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
    viewport: Viewport,
    edit: MoveEdit,
    gizmo_drag: Res<crate::gizmo::GizmoDrag>,
    gizmo_rect: Res<crate::gizmo::GizmoRect>,
) {
    let Viewport { cameras, windows } = viewport;
    let MoveEdit {
        mut grid,
        mut history,
        tool,
        mut selection,
        mut drag,
    } = edit;
    // Bail if tool switched away mid-drag — revert any partial writes.
    if tool.current != Tool::Move {
        if drag.active {
            history.abort(&mut grid);
            if drag.ad_hoc {
                selection.clear();
            } else {
                selection.aabb = drag.original_aabb;
                selection.cells = drag.original_cells.clone();
            }
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
        if drag.ad_hoc {
            selection.clear();
        } else {
            selection.aabb = drag.original_aabb;
            selection.cells = drag.original_cells.clone();
        }
        drag.reset();
        return;
    }

    // Drag start: click on a voxel inside the selection, or any voxel when
    // no selection exists (ad-hoc single-voxel move).
    if !drag.active {
        if !lmb_just || blocked {
            return;
        }
        let Some((origin, dir)) = cursor_ray(&cameras, &windows) else {
            return;
        };
        let Some(hit) = pick(&grid, origin, dir) else {
            return;
        };
        if !hit.hit_voxel {
            return;
        }
        let (effective_aabb, ad_hoc) = match selection.aabb {
            Some(aabb) if selection.contains(hit.cell) => (aabb, false),
            Some(_) => return, // Click outside an existing selection: ignore.
            None => (SelectionAabb::from_corners(hit.cell, hit.cell), true),
        };
        let (anchor, _) = stroke_anchor_from_hit(&hit, AnchorTarget::Picked);
        let start_cell = anchor_target(&anchor, origin, dir).unwrap_or(hit.cell);

        let originals: Vec<(IVec3, Color8)> = match &selection.cells {
            Some(cells) if !ad_hoc => cells
                .iter()
                .filter_map(|p| grid.get(*p).map(|c| (*p, c)))
                .collect(),
            _ => effective_aabb
                .iter_cells()
                .filter_map(|p| grid.get(p).map(|c| (p, c)))
                .collect(),
        };
        if originals.is_empty() {
            return;
        }
        history.begin();
        drag.active = true;
        drag.anchor = Some(anchor);
        drag.start_cell = Some(start_cell);
        drag.applied_delta = IVec3::ZERO;
        drag.originals_set = originals.iter().map(|(p, _)| (p.x, p.y, p.z)).collect();
        drag.originals = originals;
        drag.original_aabb = Some(effective_aabb);
        drag.original_cells = selection.cells.clone();
        drag.prev_state.clear();
        drag.ad_hoc = ad_hoc;
        // Show the ad-hoc 1-cell selection while dragging so the overlay
        // tracks the moving voxel.
        if ad_hoc {
            selection.set_aabb(effective_aabb);
        }
        return;
    }

    // Drag in progress.
    if lmb_pressed {
        let Some((origin, dir)) = cursor_ray(&cameras, &windows) else {
            return;
        };
        let (Some(anchor), Some(start), Some(orig_aabb)) =
            (drag.anchor, drag.start_cell, drag.original_aabb)
        else {
            return;
        };
        let Some(target) = anchor_target(&anchor, origin, dir) else {
            return;
        };
        let shift = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
        let new_delta = constrain_move_delta(target - start, anchor.axis, shift);
        if new_delta == drag.applied_delta {
            return;
        }
        // Refuse shifts that would drop voxels below the floor. The open
        // world has no upper bound on X/Y/Z, but Y < 0 is forbidden.
        let new_min = orig_aabb.min + new_delta;
        if new_min.y < 0 {
            return;
        }

        // Collision check: refuse shifts that would land a moving voxel on
        // a pre-stroke voxel that isn't part of the moving set. Keeps the
        // selection from devouring obstacles in its path.
        let mut collides = false;
        for (src, _) in &drag.originals {
            let dst = *src + new_delta;
            let key = (dst.x, dst.y, dst.z);
            if drag.originals_set.contains(&key) {
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
        if let Some(orig_cells) = &drag.original_cells {
            selection.cells = Some(orig_cells.iter().map(|p| *p + new_delta).collect());
        }
        return;
    }

    // LMB released → commit the move.
    history.end();
    if drag.ad_hoc {
        selection.clear();
    }
    drag.reset();
}

pub fn undo_redo_system(
    keys: Res<ButtonInput<KeyCode>>,
    mut grid: ResMut<VoxelGrid>,
    mut history: ResMut<History>,
) {
    let cmd = keys.pressed(KeyCode::SuperLeft)
        || keys.pressed(KeyCode::SuperRight)
        || keys.pressed(KeyCode::ControlLeft)
        || keys.pressed(KeyCode::ControlRight);
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

pub fn alt_eyedropper_system(keys: Res<ButtonInput<KeyCode>>, mut tool: ResMut<ToolState>) {
    let alt_just = keys.just_pressed(KeyCode::AltLeft) || keys.just_pressed(KeyCode::AltRight);
    let alt_released =
        keys.just_released(KeyCode::AltLeft) || keys.just_released(KeyCode::AltRight);
    let alt_held = keys.pressed(KeyCode::AltLeft) || keys.pressed(KeyCode::AltRight);
    let z_just = keys.just_pressed(KeyCode::KeyZ);
    let z = keys.pressed(KeyCode::KeyZ);

    if alt_just && tool.current != Tool::Eyedropper {
        if !z {
            tool.previous = tool.current;
            tool.current = Tool::Eyedropper;
        }
    } else if (alt_released || (z_just && alt_held)) && tool.current == Tool::Eyedropper {
        tool.current = tool.previous;
    }
}

pub fn tool_shortcut_system(
    mut contexts: EguiContexts,
    keys: Res<ButtonInput<KeyCode>>,
    mut tool: ResMut<ToolState>,
    mut shape_options: ResMut<ShapeOptions>,
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
        && tool.current != t
    {
        tool.previous = tool.current;
        tool.current = t;
    } else if keys.just_pressed(KeyCode::KeyS) && tool.current == Tool::Shape {
        shape_options.primitive = match shape_options.primitive {
            ShapePrimitive::Rectangle => ShapePrimitive::Ellipse,
            ShapePrimitive::Ellipse => ShapePrimitive::Line,
            ShapePrimitive::Line => ShapePrimitive::Sphere,
            ShapePrimitive::Sphere => ShapePrimitive::Rectangle,
        };
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

    fn hit(cell: IVec3, normal: IVec3) -> Hit {
        Hit {
            cell,
            normal,
            hit_voxel: true,
        }
    }

    #[test]
    fn stroke_anchor_adjacent_targets_one_layer_along_normal() {
        let (a, s) =
            stroke_anchor_from_hit(&hit(IVec3::new(2, 3, 4), IVec3::Y), AnchorTarget::Adjacent);
        assert_eq!(a.axis, 1);
        assert_eq!(a.plane_world, 4.0);
        assert_eq!(a.target_layer, 4);
        assert_eq!(s, 1);
    }

    #[test]
    fn stroke_anchor_picked_targets_hit_layer() {
        let (a, s) =
            stroke_anchor_from_hit(&hit(IVec3::new(2, 3, 4), IVec3::Y), AnchorTarget::Picked);
        assert_eq!(a.target_layer, 3);
        assert_eq!(s, 1);
    }

    #[test]
    fn stroke_anchor_negative_normal_keeps_plane_on_low_face() {
        let (a, s) = stroke_anchor_from_hit(
            &hit(IVec3::new(0, 5, 0), IVec3::NEG_X),
            AnchorTarget::Adjacent,
        );
        assert_eq!(a.axis, 0);
        assert_eq!(a.plane_world, 0.0);
        assert_eq!(a.target_layer, -1);
        assert_eq!(s, -1);
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
    fn axis_lock_keeps_dominant_axis_on_top_face() {
        // Top face → plane axis=1 (Y), in-plane is XZ. dx=5, dz=2 → X wins, Z snaps.
        let o = IVec3::new(0, 4, 0);
        assert_eq!(axis_lock(o, IVec3::new(5, 4, 2), 1), IVec3::new(5, 4, 0));
        // dz dominant → X snaps to origin.
        assert_eq!(axis_lock(o, IVec3::new(2, 4, 5), 1), IVec3::new(0, 4, 5));
    }

    #[test]
    fn axis_lock_ties_favor_first_in_plane_axis() {
        // Equal deltas → du >= dv keeps the u (first) axis, snaps v.
        let o = IVec3::new(1, 4, 1);
        assert_eq!(axis_lock(o, IVec3::new(4, 4, 4), 1), IVec3::new(4, 4, 1));
    }

    #[test]
    fn axis_lock_respects_plane_normal_axis() {
        // Side face → plane axis=0 (X), in-plane is YZ. Never touches X.
        let o = IVec3::new(7, 0, 0);
        assert_eq!(axis_lock(o, IVec3::new(7, 6, 2), 0), IVec3::new(7, 6, 0));
        assert_eq!(axis_lock(o, IVec3::new(7, 2, 6), 0), IVec3::new(7, 0, 6));
    }

    #[test]
    fn constrain_shape_rect_makes_square_on_top_face() {
        // Top face → axis=1 (Y), 2D plane is XZ.
        let c1 = IVec3::new(0, 4, 0);
        // dx=3, dz=-7 → both legs magnitude 7 with original signs.
        let out = constrain_shape_corner2(ShapePrimitive::Rectangle, 1, c1, IVec3::new(3, 4, -7));
        assert_eq!(out, IVec3::new(7, 4, -7));
    }

    #[test]
    fn constrain_shape_ellipse_matches_rect_square_rule() {
        let c1 = IVec3::new(-2, 0, 5);
        let out = constrain_shape_corner2(ShapePrimitive::Ellipse, 1, c1, IVec3::new(2, 0, 9));
        // du=4, dv=4 → already square; unchanged.
        assert_eq!(out, IVec3::new(2, 0, 9));
    }

    #[test]
    fn constrain_shape_sphere_locks_to_square_footprint() {
        // Sphere shares the square-footprint rule so Shift gives equal in-plane radii.
        let c1 = IVec3::new(0, 0, 0);
        let out = constrain_shape_corner2(ShapePrimitive::Sphere, 1, c1, IVec3::new(5, 0, 2));
        // max(|du|, |dv|) = 5 along both in-plane axes.
        assert_eq!(out, IVec3::new(5, 0, 5));
    }

    #[test]
    fn constrain_shape_rect_preserves_anchor_axis() {
        // Front face → axis=2 (Z), 2D plane is XY. The Z coord must stay put.
        let c1 = IVec3::new(0, 0, 8);
        let out = constrain_shape_corner2(ShapePrimitive::Rectangle, 2, c1, IVec3::new(5, -2, 99));
        assert_eq!(out.z, 8);
        // |du|=5, |dv|=2 → square of size 5 with original signs.
        assert_eq!(out, IVec3::new(5, -5, 8));
    }

    #[test]
    fn constrain_shape_line_snaps_to_axis_when_nearly_horizontal() {
        // Axis=1 (Y face), du=10 dx, dv=1 dz → close to horizontal, snap to u-axis only.
        let c1 = IVec3::new(0, 0, 0);
        let out = constrain_shape_corner2(ShapePrimitive::Line, 1, c1, IVec3::new(10, 0, 1));
        assert_eq!(out, IVec3::new(10, 0, 0));
    }

    #[test]
    fn constrain_shape_line_snaps_to_diagonal_when_balanced() {
        // du=5, dv=4 → ratio 0.8 ≥ tan(22.5°) → diagonal, magnitude max(|du|,|dv|)=5.
        let c1 = IVec3::new(0, 0, 0);
        let out = constrain_shape_corner2(ShapePrimitive::Line, 1, c1, IVec3::new(5, 0, 4));
        assert_eq!(out, IVec3::new(5, 0, 5));
    }

    #[test]
    fn constrain_shape_line_preserves_sign_on_diagonal() {
        let c1 = IVec3::new(0, 0, 0);
        let out = constrain_shape_corner2(ShapePrimitive::Line, 1, c1, IVec3::new(-6, 0, 5));
        assert_eq!(out, IVec3::new(-6, 0, 6));
    }

    #[test]
    fn constrain_shape_zero_delta_is_identity() {
        let c1 = IVec3::new(2, 3, 4);
        for prim in [
            ShapePrimitive::Rectangle,
            ShapePrimitive::Ellipse,
            ShapePrimitive::Line,
            ShapePrimitive::Sphere,
        ] {
            assert_eq!(constrain_shape_corner2(prim, 1, c1, c1), c1);
        }
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

    const C_R: Color8 = [255, 0, 0, 255];
    const C_G: Color8 = [0, 255, 0, 255];
    const C_B: Color8 = [0, 0, 255, 255];

    #[test]
    fn color_pool_primary_first_dedup_against_primary() {
        let pool = color_pool(C_R, &[C_G, C_R, C_B]);
        assert_eq!(pool, vec![C_R, C_G, C_B]);
    }

    #[test]
    fn color_pool_preserves_extras_order() {
        let pool = color_pool(C_R, &[C_B, C_G]);
        assert_eq!(pool, vec![C_R, C_B, C_G]);
    }

    #[test]
    fn sample_color_pool_of_one_returns_primary() {
        for x in -3..3 {
            for y in 0..3 {
                for z in -3..3 {
                    assert_eq!(sample_color(IVec3::new(x, y, z), &[C_R]), C_R);
                }
            }
        }
    }

    #[test]
    fn sample_color_is_deterministic_for_same_position() {
        let pool = color_pool(C_R, &[C_G, C_B]);
        let p = IVec3::new(7, 2, -4);
        let first = sample_color(p, &pool);
        for _ in 0..16 {
            assert_eq!(sample_color(p, &pool), first);
        }
    }

    #[test]
    fn sample_color_distributes_across_pool() {
        let pool = color_pool(C_R, &[C_G, C_B]);
        let mut seen: HashSet<Color8> = HashSet::new();
        for x in 0..16 {
            for z in 0..16 {
                seen.insert(sample_color(IVec3::new(x, 0, z), &pool));
            }
        }
        assert_eq!(seen.len(), 3);
    }

    #[test]
    fn sample_color_always_returns_member_of_pool() {
        let pool = color_pool(C_R, &[C_G, C_B]);
        for x in -8..8 {
            for y in 0..4 {
                for z in -8..8 {
                    let c = sample_color(IVec3::new(x, y, z), &pool);
                    assert!(pool.contains(&c));
                }
            }
        }
    }

    #[test]
    fn extra_colors_toggle_adds_and_removes() {
        let mut e = ExtraColors::default();
        assert!(e.toggle(C_G));
        assert!(e.contains(C_G));
        assert!(!e.toggle(C_G));
        assert!(!e.contains(C_G));
    }

    #[test]
    fn apply_swatch_click_plain_resets_extras_and_swaps_primary() {
        let mut e = ExtraColors::default();
        e.toggle(C_G);
        e.toggle(C_B);
        let new_primary = apply_swatch_click(false, C_G, C_R, &mut e);
        assert_eq!(new_primary, C_G);
        assert!(e.0.is_empty());
    }

    #[test]
    fn apply_swatch_click_shift_on_primary_is_noop() {
        let mut e = ExtraColors::default();
        let new_primary = apply_swatch_click(true, C_R, C_R, &mut e);
        assert_eq!(new_primary, C_R);
        assert!(e.0.is_empty());
    }

    #[test]
    fn apply_swatch_click_shift_toggles_extras() {
        let mut e = ExtraColors::default();
        apply_swatch_click(true, C_G, C_R, &mut e);
        assert!(e.contains(C_G));
        apply_swatch_click(true, C_B, C_R, &mut e);
        assert!(e.contains(C_B));
        apply_swatch_click(true, C_G, C_R, &mut e);
        assert!(!e.contains(C_G));
        assert!(e.contains(C_B));
    }

    #[test]
    fn select_rmb_in_idle_keeps_committed_selection() {
        // RMB is camera orbit/pan — must not deselect.
        let mut state = SelectState::default();
        let mut selection = Selection::default();
        selection.set_aabb(SelectionAabb::from_corners(
            IVec3::ZERO,
            IVec3::new(1, 1, 1),
        ));
        let keys = ButtonInput::<KeyCode>::default();
        let mut mouse = ButtonInput::<MouseButton>::default();
        mouse.press(MouseButton::Right);
        select_input(
            &mut state,
            &mut selection,
            &keys,
            &mouse,
            &VoxelGrid::default(),
            Vec3::ZERO,
            Vec3::Z,
            false,
        );
        assert!(selection.aabb.is_some());
        assert_eq!(state.phase, SelectPhase::Idle);
    }

    #[test]
    fn select_esc_in_idle_clears_selection() {
        let mut state = SelectState::default();
        let mut selection = Selection::default();
        selection.set_aabb(SelectionAabb::from_corners(
            IVec3::ZERO,
            IVec3::new(1, 1, 1),
        ));
        let mut keys = ButtonInput::<KeyCode>::default();
        keys.press(KeyCode::Escape);
        let mouse = ButtonInput::<MouseButton>::default();
        select_input(
            &mut state,
            &mut selection,
            &keys,
            &mouse,
            &VoxelGrid::default(),
            Vec3::ZERO,
            Vec3::Z,
            false,
        );
        assert!(selection.aabb.is_none());
    }
}
