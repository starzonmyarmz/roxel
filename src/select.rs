use crate::grid::{Color8, VoxelGrid};
use crate::history::History;
use crate::tools::{StrokeAnchor, Tool, ToolState};
use bevy::prelude::*;
use std::collections::{HashSet, VecDeque};

/// Max gap between two LMB-press events that still counts as a double-click.
/// Bevy 0.18 has no native double-click detection so the Select tool tracks
/// last press time + cell itself.
pub const DOUBLE_CLICK_SECS: f64 = 0.4;

/// Gizmo group for selection visuals: marching-ants outline tracing either
/// the AABB hull or the silhouette of the cell mask. Configured with
/// `depth_bias = -1.0` so the overlay x-rays through voxels — users need to
/// see which cells are selected inside a solid block.
#[derive(Default, Reflect, GizmoConfigGroup)]
pub struct SelectionGizmos;

pub fn configure_selection_gizmos(mut store: ResMut<GizmoConfigStore>) {
    let (config, _) = store.config_mut::<SelectionGizmos>();
    config.depth_bias = -1.0;
    config.line.width = 1.5;
}

/// World-units per dash stripe along the AABB outline.
const STRIPE_LEN: f32 = 0.18;
/// Phase advance per second — sets how fast the ants march.
const STRIPE_SPEED: f32 = 0.35;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SelectionAabb {
    pub min: IVec3,
    pub max: IVec3,
}

impl SelectionAabb {
    pub fn from_corners(a: IVec3, b: IVec3) -> Self {
        Self {
            min: IVec3::new(a.x.min(b.x), a.y.min(b.y), a.z.min(b.z)),
            max: IVec3::new(a.x.max(b.x), a.y.max(b.y), a.z.max(b.z)),
        }
    }

    pub fn contains(&self, p: IVec3) -> bool {
        p.x >= self.min.x
            && p.x <= self.max.x
            && p.y >= self.min.y
            && p.y <= self.max.y
            && p.z >= self.min.z
            && p.z <= self.max.z
    }

    pub fn extents(&self) -> IVec3 {
        self.max - self.min + IVec3::ONE
    }

    #[allow(dead_code)]
    pub fn cell_count(&self) -> usize {
        let e = self.extents();
        (e.x as usize) * (e.y as usize) * (e.z as usize)
    }

    pub fn iter_cells(&self) -> impl Iterator<Item = IVec3> + '_ {
        let (mn, mx) = (self.min, self.max);
        (mn.z..=mx.z).flat_map(move |z| {
            (mn.y..=mx.y).flat_map(move |y| (mn.x..=mx.x).map(move |x| IVec3::new(x, y, z)))
        })
    }

    /// Count of non-empty cells inside the AABB (clipped to the grid bounds).
    pub fn voxel_count(&self, grid: &VoxelGrid) -> usize {
        self.iter_cells()
            .filter(|p| grid.in_bounds(*p) && grid.get(*p).is_some())
            .count()
    }
}

/// Active selection. `aabb` is the bounding hull used for the marching-ants
/// outline + every "is this in the selection" check; `cells` is an optional
/// per-cell mask that restricts ops to a specific subset of the hull. The mask
/// is populated by double-click-to-pick-connected-region; AABB drags leave it
/// `None` so the full hull is treated as selected.
#[derive(Resource, Default)]
pub struct Selection {
    pub aabb: Option<SelectionAabb>,
    pub cells: Option<HashSet<IVec3>>,
}

impl Selection {
    pub fn clear(&mut self) {
        self.aabb = None;
        self.cells = None;
    }

    pub fn set_aabb(&mut self, aabb: SelectionAabb) {
        self.aabb = Some(aabb);
        self.cells = None;
    }

    pub fn set_cells(&mut self, cells: HashSet<IVec3>) {
        self.aabb = aabb_of_cells_iter(cells.iter().copied());
        self.cells = if cells.is_empty() { None } else { Some(cells) };
    }

    pub fn contains(&self, p: IVec3) -> bool {
        if let Some(cells) = &self.cells {
            return cells.contains(&p);
        }
        self.aabb.is_some_and(|a| a.contains(p))
    }

    /// Occupied-voxel count inside the active selection. Respects the cell
    /// mask when present so the inspector reports the actual cells the user
    /// double-clicked, not the AABB hull.
    pub fn voxel_count(&self, grid: &VoxelGrid) -> usize {
        if let Some(cells) = &self.cells {
            return cells
                .iter()
                .filter(|p| grid.in_bounds(**p) && grid.get(**p).is_some())
                .count();
        }
        match self.aabb {
            Some(a) => a.voxel_count(grid),
            None => 0,
        }
    }
}

fn aabb_of_cells_iter(mut iter: impl Iterator<Item = IVec3>) -> Option<SelectionAabb> {
    let first = iter.next()?;
    let mut min = first;
    let mut max = first;
    for c in iter {
        min = min.min(c);
        max = max.max(c);
    }
    Some(SelectionAabb { min, max })
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SelectPhase {
    #[default]
    Idle,
    Footprint,
    Extrude,
}

#[derive(Resource, Default)]
pub struct SelectState {
    pub phase: SelectPhase,
    pub anchor: Option<StrokeAnchor>,
    pub corner1: Option<IVec3>,
    pub corner2: Option<IVec3>,
    pub normal_sign: i32,
    pub thickness: i32,
    /// Wall-clock seconds at the previous LMB-press while the Select tool
    /// was active. Used together with `last_press_cell` to detect a
    /// double-click. Not cleared by `reset()` — `reset()` runs between the
    /// first and second click of a double-click sequence.
    pub last_press_secs: f64,
    pub last_press_cell: Option<IVec3>,
}

impl SelectState {
    pub fn reset(&mut self) {
        self.phase = SelectPhase::Idle;
        self.anchor = None;
        self.corner1 = None;
        self.corner2 = None;
        self.normal_sign = 0;
        self.thickness = 1;
    }
}

/// Clear every non-empty cell inside the AABB. One history stroke.
fn clear_aabb(grid: &mut VoxelGrid, history: &mut History, aabb: &SelectionAabb) {
    history.begin();
    for cell in aabb.iter_cells() {
        if grid.in_bounds(cell) && grid.get(cell).is_some() {
            history.record(grid, cell, None);
        }
    }
    history.end();
}

/// Clear every non-empty cell in the active selection. When the selection has
/// a cell mask only those cells clear; otherwise it falls back to the AABB.
pub fn clear_selection(grid: &mut VoxelGrid, history: &mut History, selection: &Selection) {
    if let Some(cells) = &selection.cells {
        history.begin();
        for cell in cells {
            if grid.in_bounds(*cell) && grid.get(*cell).is_some() {
                history.record(grid, *cell, None);
            }
        }
        history.end();
        return;
    }
    if let Some(aabb) = selection.aabb {
        clear_aabb(grid, history, &aabb);
    }
}

/// Recolor every non-empty cell inside the AABB by sampling from `pool`
/// per cell. Empty cells stay empty — Paint must not materialize new voxels.
/// One history stroke. Returns the distinct colors actually used.
fn recolor_aabb(
    grid: &mut VoxelGrid,
    history: &mut History,
    aabb: &SelectionAabb,
    pool: &[Color8],
) -> Vec<Color8> {
    history.begin();
    let mut used: Vec<Color8> = Vec::new();
    for cell in aabb.iter_cells() {
        if grid.in_bounds(cell) && grid.get(cell).is_some() {
            let c = crate::tools::sample_color(cell, pool);
            history.record(grid, cell, Some(c));
            if !used.contains(&c) {
                used.push(c);
            }
        }
    }
    history.end();
    used
}

/// Recolor every non-empty cell in the active selection by sampling from
/// `pool` per cell. Respects the cell mask when present. Returns the distinct
/// colors actually used.
pub fn recolor_selection(
    grid: &mut VoxelGrid,
    history: &mut History,
    selection: &Selection,
    pool: &[Color8],
) -> Vec<Color8> {
    if let Some(cells) = &selection.cells {
        history.begin();
        let mut used: Vec<Color8> = Vec::new();
        for cell in cells {
            if grid.in_bounds(*cell) && grid.get(*cell).is_some() {
                let c = crate::tools::sample_color(*cell, pool);
                history.record(grid, *cell, Some(c));
                if !used.contains(&c) {
                    used.push(c);
                }
            }
        }
        history.end();
        return used;
    }
    if let Some(aabb) = selection.aabb {
        return recolor_aabb(grid, history, &aabb, pool);
    }
    Vec::new()
}

/// Translate every non-empty voxel inside the selection AABB by `delta`,
/// clearing originals and writing them at their new positions in a single
/// history stroke. Updates `selection.aabb` to follow. Returns false (no-op)
/// when `delta` is zero, no selection exists, or the shifted AABB would
/// leave the grid.
pub fn move_selection(
    grid: &mut VoxelGrid,
    history: &mut History,
    selection: &mut Selection,
    delta: IVec3,
) -> bool {
    if delta == IVec3::ZERO {
        return false;
    }
    let Some(aabb) = selection.aabb else {
        return false;
    };

    let new_min = aabb.min + delta;
    let new_max = aabb.max + delta;
    // Open world: only the floor is hard-bounded.
    if new_min.y < 0 {
        return false;
    }

    // Snapshot occupied cells before any mutation so source-clears and
    // destination-writes overlap cleanly inside one stroke. Respect the
    // selection's per-cell mask when present — only voxels in the mask move.
    let occupied: Vec<(IVec3, Color8)> = match &selection.cells {
        Some(cells) => cells
            .iter()
            .filter_map(|p| grid.get(*p).map(|c| (*p, c)))
            .collect(),
        None => aabb
            .iter_cells()
            .filter_map(|p| grid.get(p).map(|c| (p, c)))
            .collect(),
    };

    if occupied.is_empty() {
        selection.aabb = Some(SelectionAabb {
            min: new_min,
            max: new_max,
        });
        if let Some(cells) = selection.cells.as_mut() {
            *cells = cells.iter().map(|p| *p + delta).collect();
        }
        return true;
    }

    // Refuse when any destination collides with a voxel outside the moving
    // set — the move must not destroy unrelated voxels in its path.
    let source_set: std::collections::HashSet<(i32, i32, i32)> =
        occupied.iter().map(|(p, _)| (p.x, p.y, p.z)).collect();
    for (src, _) in &occupied {
        let dst = *src + delta;
        let key = (dst.x, dst.y, dst.z);
        if source_set.contains(&key) {
            continue;
        }
        if grid.get(dst).is_some() {
            return false;
        }
    }

    history.begin();
    for (src, _) in &occupied {
        history.record(grid, *src, None);
    }
    for (src, color) in &occupied {
        history.record(grid, *src + delta, Some(*color));
    }
    history.end();

    selection.aabb = Some(SelectionAabb {
        min: new_min,
        max: new_max,
    });
    if let Some(cells) = selection.cells.as_mut() {
        *cells = cells.iter().map(|p| *p + delta).collect();
    }
    true
}

/// 6-connected flood fill from `start` over cells matching `start`'s color.
/// Returns an empty vec when `start` is empty. Pure: no allocations on the
/// grid, no history side effects.
pub fn connected_same_color(grid: &VoxelGrid, start: IVec3) -> Vec<IVec3> {
    let Some(color) = grid.get(start) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let mut seen: HashSet<IVec3> = HashSet::new();
    let mut q: VecDeque<IVec3> = VecDeque::new();
    seen.insert(start);
    q.push_back(start);
    const DIRS: [IVec3; 6] = [
        IVec3::X,
        IVec3::NEG_X,
        IVec3::Y,
        IVec3::NEG_Y,
        IVec3::Z,
        IVec3::NEG_Z,
    ];
    while let Some(p) = q.pop_front() {
        out.push(p);
        for d in DIRS {
            let n = p + d;
            if seen.insert(n) && grid.get(n) == Some(color) {
                q.push_back(n);
            }
        }
    }
    out
}

/// AABB hull of an arbitrary cell list. `None` for an empty slice.
#[cfg(test)]
pub fn aabb_of_cells(cells: &[IVec3]) -> Option<SelectionAabb> {
    aabb_of_cells_iter(cells.iter().copied())
}

/// Build an in-progress AABB from the current `SelectState` corners + signed
/// extrude offset along the anchor axis. Returns None during `Idle`.
///
/// `state.thickness` here is the signed cell offset from `target_layer` in the
/// normal direction: `0` is a single-cell-deep AABB on the anchor plane, `+N`
/// extends `N` cells in the face-normal direction, `-N` extends `N` cells back
/// into the surface.
pub fn in_progress_aabb(state: &SelectState) -> Option<SelectionAabb> {
    let (Some(anchor), Some(c1), Some(c2)) = (state.anchor, state.corner1, state.corner2) else {
        return None;
    };
    let depth_end = anchor.target_layer + state.thickness * state.normal_sign;
    let mut a = c1.to_array();
    let mut b = c2.to_array();
    a[anchor.axis] = anchor.target_layer;
    b[anchor.axis] = depth_end;
    Some(SelectionAabb::from_corners(
        IVec3::from_array(a),
        IVec3::from_array(b),
    ))
}

/// One stripe along a marching-ants edge: (start, end, is_white). Pure helper
/// so the phase math is unit-testable without spinning up a render.
pub fn marching_segments(len: f32, phase: f32, stripe: f32) -> Vec<(f32, f32, bool)> {
    let mut out = Vec::new();
    if len <= 0.0 || stripe <= 0.0 {
        return out;
    }
    let cycle = 2.0 * stripe;
    let p = phase.rem_euclid(cycle);
    let mut s = -p;
    let mut idx = 0;
    while s < len {
        let e = (s + stripe).min(len);
        let cs = s.max(0.0);
        if e > cs {
            out.push((cs, e, idx % 2 == 0));
        }
        s += stripe;
        idx += 1;
    }
    out
}

/// Outline edges of a mask region. Returns `(axis, anchor)` where the edge
/// runs from `anchor` to `anchor + axis_unit`. An edge is kept when it lies on
/// the boundary of the masked surface — interior edges (4 surrounding cells in
/// mask) and edges where two coplanar exposed faces meet (the K=2 adjacent
/// case) are dropped so straight runs of voxels read as a single shape.
pub fn silhouette_edges(mask: &HashSet<IVec3>) -> Vec<(u8, IVec3)> {
    let mut edges: HashSet<(u8, IVec3)> = HashSet::new();
    for cell in mask {
        for axis in 0u8..3 {
            let (p1, p2) = perp_axes(axis);
            for db in 0..2i32 {
                for dc in 0..2i32 {
                    let mut k = cell.to_array();
                    k[p1] += db;
                    k[p2] += dc;
                    edges.insert((axis, IVec3::from_array(k)));
                }
            }
        }
    }
    let mut out = Vec::with_capacity(edges.len());
    for (axis, key) in edges {
        let (p1, p2) = perp_axes(axis);
        let mut in_grid = [[false; 2]; 2];
        for a in 0..2i32 {
            for b in 0..2i32 {
                let mut c = key.to_array();
                c[p1] += a - 1;
                c[p2] += b - 1;
                if mask.contains(&IVec3::from_array(c)) {
                    in_grid[a as usize][b as usize] = true;
                }
            }
        }
        let k = in_grid.iter().flatten().filter(|&&x| x).count();
        if k == 0 || k == 4 {
            continue;
        }
        if k == 2 {
            let same_row = (in_grid[0][0] && in_grid[0][1]) || (in_grid[1][0] && in_grid[1][1]);
            let same_col = (in_grid[0][0] && in_grid[1][0]) || (in_grid[0][1] && in_grid[1][1]);
            if same_row || same_col {
                continue;
            }
        }
        out.push((axis, key));
    }
    out
}

fn perp_axes(axis: u8) -> (usize, usize) {
    match axis {
        0 => (1, 2),
        1 => (0, 2),
        _ => (0, 1),
    }
}

fn draw_marching_edge(gizmos: &mut Gizmos<SelectionGizmos>, a: Vec3, b: Vec3, phase: f32) {
    let dir = b - a;
    let len = dir.length();
    if len <= 0.0 {
        return;
    }
    let dirn = dir / len;
    for (s, e, white) in marching_segments(len, phase, STRIPE_LEN) {
        let p0 = a + dirn * s;
        let p1 = a + dirn * e;
        let color = if white { Color::WHITE } else { Color::BLACK };
        gizmos.line(p0, p1, color);
    }
}

pub fn selection_render_system(
    selection: Res<Selection>,
    state: Res<SelectState>,
    time: Res<Time>,
    snapshot_active: Res<crate::snapshot::SnapshotInProgress>,
    mut gizmos: Gizmos<SelectionGizmos>,
) {
    if snapshot_active.0 {
        return;
    }
    // In-progress drag takes priority over a committed selection so the user
    // sees the new region they're drawing.
    let active_aabb = if state.phase != SelectPhase::Idle {
        in_progress_aabb(&state).or(selection.aabb)
    } else {
        selection.aabb
    };
    let Some(aabb) = active_aabb else {
        return;
    };

    let phase_now = time.elapsed_secs() * STRIPE_SPEED;

    // Per-cell mask wins: trace the silhouette of the masked region so the
    // marching ants follow one merged shape instead of every voxel's cube.
    if state.phase == SelectPhase::Idle
        && let Some(cells) = &selection.cells
    {
        for (axis, anchor) in silhouette_edges(cells) {
            let a = Vec3::new(anchor.x as f32, anchor.y as f32, anchor.z as f32);
            let mut b = a;
            match axis {
                0 => b.x += 1.0,
                1 => b.y += 1.0,
                _ => b.z += 1.0,
            }
            draw_marching_edge(&mut gizmos, a, b, phase_now);
        }
        return;
    }

    // Outline corners, slightly inflated so the stroke sits just outside cell
    // faces rather than z-fighting them (depth_bias handles the x-ray, this
    // keeps the line crisp at edge corners).
    let pad = 0.01;
    let min = Vec3::new(
        aabb.min.x as f32 - pad,
        aabb.min.y as f32 - pad,
        aabb.min.z as f32 - pad,
    );
    let max = Vec3::new(
        aabb.max.x as f32 + 1.0 + pad,
        aabb.max.y as f32 + 1.0 + pad,
        aabb.max.z as f32 + 1.0 + pad,
    );
    let corners = [
        Vec3::new(min.x, min.y, min.z),
        Vec3::new(max.x, min.y, min.z),
        Vec3::new(max.x, max.y, min.z),
        Vec3::new(min.x, max.y, min.z),
        Vec3::new(min.x, min.y, max.z),
        Vec3::new(max.x, min.y, max.z),
        Vec3::new(max.x, max.y, max.z),
        Vec3::new(min.x, max.y, max.z),
    ];
    let edges = [
        (0, 1),
        (1, 2),
        (2, 3),
        (3, 0),
        (4, 5),
        (5, 6),
        (6, 7),
        (7, 4),
        (0, 4),
        (1, 5),
        (2, 6),
        (3, 7),
    ];
    for (a, b) in edges {
        draw_marching_edge(&mut gizmos, corners[a], corners[b], phase_now);
    }
}

/// Backspace/Delete clears voxels inside selection. Esc clears the selection.
/// Both are gated on egui not capturing keys.
pub fn selection_key_action_system(
    mut contexts: bevy_egui::EguiContexts,
    keys: Res<ButtonInput<KeyCode>>,
    mut selection: ResMut<Selection>,
    mut grid: ResMut<VoxelGrid>,
    mut history: ResMut<History>,
    tool: Res<ToolState>,
    select_state: Res<SelectState>,
) {
    let egui_wants = contexts
        .ctx_mut()
        .map(|c| c.wants_keyboard_input())
        .unwrap_or(false);
    if egui_wants {
        return;
    }
    if keys.just_pressed(KeyCode::Escape) {
        // When the Select tool is active, tool_input_system already handles Esc
        // (it cancels in-progress phases first, then clears the selection on a
        // second press). Avoid clearing twice for the same key event.
        if tool.current != Tool::Select && selection.aabb.is_some() {
            selection.clear();
        }
        return;
    }
    let cmd = keys.pressed(KeyCode::SuperLeft)
        || keys.pressed(KeyCode::SuperRight)
        || keys.pressed(KeyCode::ControlLeft)
        || keys.pressed(KeyCode::ControlRight);
    if cmd && keys.just_pressed(KeyCode::KeyD) && selection.aabb.is_some() {
        selection.clear();
        return;
    }
    if cmd && keys.just_pressed(KeyCode::KeyA) {
        let cells: HashSet<IVec3> = grid.iter_occupied().map(|(p, _)| p).collect();
        if !cells.is_empty() {
            selection.set_cells(cells);
        }
        return;
    }
    if (keys.just_pressed(KeyCode::Backspace) || keys.just_pressed(KeyCode::Delete))
        && selection.aabb.is_some()
        && select_state.phase == SelectPhase::Idle
    {
        clear_selection(&mut grid, &mut history, &selection);
    }
}

/// Arrow keys nudge the selection (and its voxels) by one cell while the
/// Move tool is active. Left/Right = ∓X, Up/Down = ∓Z (ground plane);
/// Shift+Up/Down = ±Y (vertical).
pub fn move_selection_keys_system(
    mut contexts: bevy_egui::EguiContexts,
    keys: Res<ButtonInput<KeyCode>>,
    tool: Res<ToolState>,
    mut grid: ResMut<VoxelGrid>,
    mut history: ResMut<History>,
    mut selection: ResMut<Selection>,
) {
    if tool.current != Tool::Move {
        return;
    }
    if selection.aabb.is_none() {
        return;
    }
    let egui_wants = contexts
        .ctx_mut()
        .map(|c| c.wants_keyboard_input())
        .unwrap_or(false);
    if egui_wants {
        return;
    }
    let shift = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
    let mut delta = IVec3::ZERO;
    if keys.just_pressed(KeyCode::ArrowLeft) {
        delta.x -= 1;
    }
    if keys.just_pressed(KeyCode::ArrowRight) {
        delta.x += 1;
    }
    if shift {
        if keys.just_pressed(KeyCode::ArrowUp) {
            delta.y += 1;
        }
        if keys.just_pressed(KeyCode::ArrowDown) {
            delta.y -= 1;
        }
    } else {
        if keys.just_pressed(KeyCode::ArrowUp) {
            delta.z -= 1;
        }
        if keys.just_pressed(KeyCode::ArrowDown) {
            delta.z += 1;
        }
    }
    if delta == IVec3::ZERO {
        return;
    }
    move_selection(&mut grid, &mut history, &mut selection, delta);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fill_grid(grid: &mut VoxelGrid, color: Color8, points: &[IVec3]) {
        for p in points {
            grid.set(*p, Some(color));
        }
    }

    #[test]
    fn select_all_selects_only_occupied_cells() {
        let mut grid = VoxelGrid::default();
        let c = [10, 20, 30, 255];
        grid.set(IVec3::new(-2, 0, 3), Some(c));
        grid.set(IVec3::new(5, 4, -1), Some(c));
        let cells: HashSet<IVec3> = grid.iter_occupied().map(|(p, _)| p).collect();
        let mut sel = Selection::default();
        sel.set_cells(cells);
        // Only the two occupied voxels are selected.
        assert!(sel.contains(IVec3::new(-2, 0, 3)));
        assert!(sel.contains(IVec3::new(5, 4, -1)));
        assert_eq!(sel.cells.as_ref().map(|c| c.len()), Some(2));
        // An empty cell inside the bounding hull is NOT selected.
        assert!(!sel.contains(IVec3::new(0, 0, 0)));
        // AABB hull still computed for marching-ants fallback.
        let aabb = sel.aabb.expect("hull");
        assert_eq!(aabb.min, IVec3::new(-2, 0, -1));
        assert_eq!(aabb.max, IVec3::new(5, 4, 3));
    }

    #[test]
    fn select_all_on_empty_grid_selects_nothing() {
        let grid = VoxelGrid::default();
        let cells: HashSet<IVec3> = grid.iter_occupied().map(|(p, _)| p).collect();
        assert!(cells.is_empty());
    }

    #[test]
    fn aabb_normalizes_min_max() {
        let a = IVec3::new(5, 2, 7);
        let b = IVec3::new(1, 8, 3);
        let s = SelectionAabb::from_corners(a, b);
        assert_eq!(s.min, IVec3::new(1, 2, 3));
        assert_eq!(s.max, IVec3::new(5, 8, 7));
    }

    #[test]
    fn aabb_normalizes_when_corners_equal() {
        let p = IVec3::new(4, 4, 4);
        let s = SelectionAabb::from_corners(p, p);
        assert_eq!(s.min, p);
        assert_eq!(s.max, p);
    }

    #[test]
    fn aabb_contains_boundary() {
        let s = SelectionAabb::from_corners(IVec3::new(0, 0, 0), IVec3::new(3, 3, 3));
        assert!(s.contains(s.min));
        assert!(s.contains(s.max));
        assert!(s.contains(IVec3::new(1, 2, 3)));
        assert!(!s.contains(IVec3::new(4, 0, 0)));
        assert!(!s.contains(IVec3::new(0, -1, 0)));
        assert!(!s.contains(IVec3::new(0, 0, 4)));
    }

    #[test]
    fn aabb_iter_cell_count() {
        let s = SelectionAabb::from_corners(IVec3::new(0, 0, 0), IVec3::new(2, 3, 4));
        assert_eq!(s.cell_count(), 3 * 4 * 5);
        assert_eq!(s.iter_cells().count(), 3 * 4 * 5);
    }

    #[test]
    fn aabb_iter_cells_unique() {
        use std::collections::HashSet;
        let s = SelectionAabb::from_corners(IVec3::new(-1, -1, -1), IVec3::new(2, 2, 2));
        let set: HashSet<IVec3> = s.iter_cells().collect();
        assert_eq!(set.len(), s.cell_count());
    }

    #[test]
    fn aabb_single_cell_when_min_eq_max() {
        let p = IVec3::new(7, 8, 9);
        let s = SelectionAabb::from_corners(p, p);
        assert_eq!(s.cell_count(), 1);
        let cells: Vec<_> = s.iter_cells().collect();
        assert_eq!(cells, vec![p]);
    }

    #[test]
    fn clear_aabb_clears_only_inside_cells() {
        let mut grid = VoxelGrid::default();
        let red = [200, 0, 0, 255];
        fill_grid(
            &mut grid,
            red,
            &[
                IVec3::new(1, 1, 1),
                IVec3::new(2, 1, 1),
                IVec3::new(5, 5, 5),
            ],
        );
        let s = SelectionAabb::from_corners(IVec3::new(0, 0, 0), IVec3::new(3, 3, 3));
        let mut history = History::default();
        clear_aabb(&mut grid, &mut history, &s);
        assert!(grid.get(IVec3::new(1, 1, 1)).is_none());
        assert!(grid.get(IVec3::new(2, 1, 1)).is_none());
        assert_eq!(grid.get(IVec3::new(5, 5, 5)), Some(red));
    }

    #[test]
    fn clear_aabb_leaves_empty_cells_empty() {
        let mut grid = VoxelGrid::default();
        let red = [200, 0, 0, 255];
        grid.set(IVec3::new(2, 2, 2), Some(red));
        let s = SelectionAabb::from_corners(IVec3::new(0, 0, 0), IVec3::new(3, 3, 3));
        let mut history = History::default();
        clear_aabb(&mut grid, &mut history, &s);
        for cell in s.iter_cells() {
            assert!(grid.get(cell).is_none(), "cell {cell:?} should be empty");
        }
    }

    #[test]
    fn clear_aabb_undoable_as_single_stroke() {
        let mut grid = VoxelGrid::default();
        let red = [200, 0, 0, 255];
        let touched = [
            IVec3::new(1, 1, 1),
            IVec3::new(2, 1, 1),
            IVec3::new(3, 1, 1),
        ];
        fill_grid(&mut grid, red, &touched);
        let s = SelectionAabb::from_corners(IVec3::new(0, 0, 0), IVec3::new(4, 4, 4));
        let mut history = History::default();
        let before_strokes = history.undo.len();
        clear_aabb(&mut grid, &mut history, &s);
        assert_eq!(history.undo.len(), before_strokes + 1);
        history.undo(&mut grid);
        for cell in &touched {
            assert_eq!(grid.get(*cell), Some(red));
        }
    }

    #[test]
    fn recolor_aabb_overwrites_non_empty_cells() {
        let mut grid = VoxelGrid::default();
        let red = [200, 0, 0, 255];
        let blue = [0, 0, 200, 255];
        fill_grid(&mut grid, red, &[IVec3::new(1, 1, 1), IVec3::new(2, 2, 2)]);
        let s = SelectionAabb::from_corners(IVec3::new(0, 0, 0), IVec3::new(3, 3, 3));
        let mut history = History::default();
        recolor_aabb(&mut grid, &mut history, &s, &[blue]);
        assert_eq!(grid.get(IVec3::new(1, 1, 1)), Some(blue));
        assert_eq!(grid.get(IVec3::new(2, 2, 2)), Some(blue));
    }

    #[test]
    fn recolor_aabb_skips_empty_cells() {
        let mut grid = VoxelGrid::default();
        let red = [200, 0, 0, 255];
        let blue = [0, 0, 200, 255];
        grid.set(IVec3::new(1, 1, 1), Some(red));
        let s = SelectionAabb::from_corners(IVec3::new(0, 0, 0), IVec3::new(3, 3, 3));
        let mut history = History::default();
        recolor_aabb(&mut grid, &mut history, &s, &[blue]);
        assert_eq!(grid.get(IVec3::new(1, 1, 1)), Some(blue));
        // Every other cell in the AABB stays empty.
        for cell in s.iter_cells() {
            if cell == IVec3::new(1, 1, 1) {
                continue;
            }
            assert!(
                grid.get(cell).is_none(),
                "cell {cell:?} should remain empty"
            );
        }
    }

    #[test]
    fn recolor_aabb_skips_outside_cells() {
        let mut grid = VoxelGrid::default();
        let red = [200, 0, 0, 255];
        let blue = [0, 0, 200, 255];
        grid.set(IVec3::new(5, 5, 5), Some(red));
        let s = SelectionAabb::from_corners(IVec3::new(0, 0, 0), IVec3::new(3, 3, 3));
        let mut history = History::default();
        recolor_aabb(&mut grid, &mut history, &s, &[blue]);
        assert_eq!(grid.get(IVec3::new(5, 5, 5)), Some(red));
    }

    #[test]
    fn recolor_aabb_undoable_as_single_stroke() {
        let mut grid = VoxelGrid::default();
        let red = [200, 0, 0, 255];
        let blue = [0, 0, 200, 255];
        let touched = [
            IVec3::new(1, 1, 1),
            IVec3::new(2, 2, 2),
            IVec3::new(3, 1, 1),
        ];
        fill_grid(&mut grid, red, &touched);
        let s = SelectionAabb::from_corners(IVec3::new(0, 0, 0), IVec3::new(4, 4, 4));
        let mut history = History::default();
        let before_strokes = history.undo.len();
        recolor_aabb(&mut grid, &mut history, &s, &[blue]);
        assert_eq!(history.undo.len(), before_strokes + 1);
        history.undo(&mut grid);
        for cell in &touched {
            assert_eq!(grid.get(*cell), Some(red));
        }
    }

    #[test]
    fn selection_default_has_no_aabb() {
        let sel = Selection::default();
        assert!(sel.aabb.is_none());
    }

    #[test]
    fn move_selection_translates_voxels_and_updates_aabb() {
        let mut grid = VoxelGrid::default();
        let red = [200, 0, 0, 255];
        fill_grid(&mut grid, red, &[IVec3::new(1, 1, 1), IVec3::new(2, 1, 1)]);
        let mut selection = Selection {
            cells: None,
            aabb: Some(SelectionAabb::from_corners(
                IVec3::new(1, 1, 1),
                IVec3::new(2, 1, 1),
            )),
        };
        let mut history = History::default();
        assert!(move_selection(
            &mut grid,
            &mut history,
            &mut selection,
            IVec3::new(3, 0, 0)
        ));
        assert!(grid.get(IVec3::new(1, 1, 1)).is_none());
        assert!(grid.get(IVec3::new(2, 1, 1)).is_none());
        assert_eq!(grid.get(IVec3::new(4, 1, 1)), Some(red));
        assert_eq!(grid.get(IVec3::new(5, 1, 1)), Some(red));
        let aabb = selection.aabb.unwrap();
        assert_eq!(aabb.min, IVec3::new(4, 1, 1));
        assert_eq!(aabb.max, IVec3::new(5, 1, 1));
    }

    #[test]
    fn move_selection_overlapping_translation_preserves_voxels() {
        let mut grid = VoxelGrid::default();
        let red = [200, 0, 0, 255];
        fill_grid(
            &mut grid,
            red,
            &[
                IVec3::new(1, 1, 1),
                IVec3::new(2, 1, 1),
                IVec3::new(3, 1, 1),
            ],
        );
        let mut selection = Selection {
            cells: None,
            aabb: Some(SelectionAabb::from_corners(
                IVec3::new(1, 1, 1),
                IVec3::new(3, 1, 1),
            )),
        };
        let mut history = History::default();
        // Shift by +1 along X — destination overlaps with source.
        assert!(move_selection(
            &mut grid,
            &mut history,
            &mut selection,
            IVec3::new(1, 0, 0)
        ));
        assert!(grid.get(IVec3::new(1, 1, 1)).is_none());
        assert_eq!(grid.get(IVec3::new(2, 1, 1)), Some(red));
        assert_eq!(grid.get(IVec3::new(3, 1, 1)), Some(red));
        assert_eq!(grid.get(IVec3::new(4, 1, 1)), Some(red));
    }

    #[test]
    fn move_selection_refuses_when_destination_hits_unrelated_voxel() {
        let mut grid = VoxelGrid::default();
        let red = [200, 0, 0, 255];
        let blue = [0, 0, 200, 255];
        grid.set(IVec3::new(1, 1, 1), Some(red));
        // Obstacle one cell away — not in selection.
        grid.set(IVec3::new(2, 1, 1), Some(blue));
        let mut selection = Selection {
            cells: None,
            aabb: Some(SelectionAabb::from_corners(
                IVec3::new(1, 1, 1),
                IVec3::new(1, 1, 1),
            )),
        };
        let mut history = History::default();
        assert!(!move_selection(
            &mut grid,
            &mut history,
            &mut selection,
            IVec3::new(1, 0, 0)
        ));
        // Both voxels preserved, no stroke recorded.
        assert_eq!(grid.get(IVec3::new(1, 1, 1)), Some(red));
        assert_eq!(grid.get(IVec3::new(2, 1, 1)), Some(blue));
        assert!(history.undo.is_empty());
        assert_eq!(selection.aabb.unwrap().min, IVec3::new(1, 1, 1));
    }

    #[test]
    fn move_selection_allows_overlapping_translation_within_self() {
        // Translation overlap with source set itself is allowed (the moving
        // selection sliding partly into its old footprint).
        let mut grid = VoxelGrid::default();
        let red = [200, 0, 0, 255];
        fill_grid(
            &mut grid,
            red,
            &[
                IVec3::new(1, 1, 1),
                IVec3::new(2, 1, 1),
                IVec3::new(3, 1, 1),
            ],
        );
        let mut selection = Selection {
            cells: None,
            aabb: Some(SelectionAabb::from_corners(
                IVec3::new(1, 1, 1),
                IVec3::new(3, 1, 1),
            )),
        };
        let mut history = History::default();
        assert!(move_selection(
            &mut grid,
            &mut history,
            &mut selection,
            IVec3::new(1, 0, 0)
        ));
    }

    #[test]
    fn move_selection_below_floor_is_noop() {
        // Open world: the only hard bound is the floor at y = 0. A move that
        // would push the AABB below the floor is refused; arbitrary negative
        // X/Z is fine.
        let mut grid = VoxelGrid::default();
        let red = [200, 0, 0, 255];
        grid.set(IVec3::new(0, 0, 0), Some(red));
        let mut selection = Selection {
            cells: None,
            aabb: Some(SelectionAabb::from_corners(
                IVec3::new(0, 0, 0),
                IVec3::new(0, 0, 0),
            )),
        };
        let mut history = History::default();
        assert!(!move_selection(
            &mut grid,
            &mut history,
            &mut selection,
            IVec3::new(0, -1, 0)
        ));
        assert_eq!(grid.get(IVec3::new(0, 0, 0)), Some(red));
        assert_eq!(selection.aabb.unwrap().min, IVec3::new(0, 0, 0));
        assert!(history.undo.is_empty());
    }

    #[test]
    fn move_selection_into_negative_x_succeeds() {
        let mut grid = VoxelGrid::default();
        let red = [200, 0, 0, 255];
        grid.set(IVec3::new(0, 0, 0), Some(red));
        let mut selection = Selection {
            cells: None,
            aabb: Some(SelectionAabb::from_corners(
                IVec3::new(0, 0, 0),
                IVec3::new(0, 0, 0),
            )),
        };
        let mut history = History::default();
        history.begin();
        assert!(move_selection(
            &mut grid,
            &mut history,
            &mut selection,
            IVec3::new(-5, 0, 0)
        ));
        history.end();
        assert_eq!(grid.get(IVec3::new(0, 0, 0)), None);
        assert_eq!(grid.get(IVec3::new(-5, 0, 0)), Some(red));
        assert_eq!(selection.aabb.unwrap().min, IVec3::new(-5, 0, 0));
    }

    #[test]
    fn move_selection_zero_delta_is_noop() {
        let mut grid = VoxelGrid::default();
        grid.set(IVec3::new(1, 1, 1), Some([1, 2, 3, 255]));
        let mut selection = Selection {
            cells: None,
            aabb: Some(SelectionAabb::from_corners(
                IVec3::new(1, 1, 1),
                IVec3::new(1, 1, 1),
            )),
        };
        let mut history = History::default();
        assert!(!move_selection(
            &mut grid,
            &mut history,
            &mut selection,
            IVec3::ZERO
        ));
        assert!(history.undo.is_empty());
    }

    #[test]
    fn move_selection_records_single_undoable_stroke() {
        let mut grid = VoxelGrid::default();
        let red = [200, 0, 0, 255];
        let pts = [
            IVec3::new(1, 1, 1),
            IVec3::new(2, 1, 1),
            IVec3::new(2, 2, 1),
        ];
        fill_grid(&mut grid, red, &pts);
        let mut selection = Selection {
            cells: None,
            aabb: Some(SelectionAabb::from_corners(
                IVec3::new(1, 1, 1),
                IVec3::new(2, 2, 1),
            )),
        };
        let mut history = History::default();
        assert!(move_selection(
            &mut grid,
            &mut history,
            &mut selection,
            IVec3::new(0, 0, 1)
        ));
        assert_eq!(history.undo.len(), 1);
        history.undo(&mut grid);
        for p in &pts {
            assert_eq!(grid.get(*p), Some(red));
        }
        for p in &pts {
            assert!(
                grid.get(*p + IVec3::new(0, 0, 1)).is_none()
                    || pts.contains(&(*p + IVec3::new(0, 0, 1)))
            );
        }
    }

    #[test]
    fn move_selection_empty_selection_just_slides_aabb() {
        let mut grid = VoxelGrid::default();
        let mut selection = Selection {
            cells: None,
            aabb: Some(SelectionAabb::from_corners(
                IVec3::new(0, 0, 0),
                IVec3::new(2, 2, 2),
            )),
        };
        let mut history = History::default();
        assert!(move_selection(
            &mut grid,
            &mut history,
            &mut selection,
            IVec3::new(1, 0, 0)
        ));
        assert!(history.undo.is_empty());
        let aabb = selection.aabb.unwrap();
        assert_eq!(aabb.min, IVec3::new(1, 0, 0));
        assert_eq!(aabb.max, IVec3::new(3, 2, 2));
    }

    #[test]
    fn in_progress_aabb_extrudes_positive_offset_in_normal_direction() {
        // Face-up pick: normal_sign = +1, target_layer = 5.
        let state = SelectState {
            phase: SelectPhase::Extrude,
            anchor: Some(StrokeAnchor {
                axis: 1,
                plane_world: 5.0,
                target_layer: 5,
            }),
            corner1: Some(IVec3::new(0, 5, 0)),
            corner2: Some(IVec3::new(2, 5, 2)),
            normal_sign: 1,
            thickness: 3,
            ..Default::default()
        };
        let aabb = in_progress_aabb(&state).unwrap();
        assert_eq!(aabb.min.y, 5);
        assert_eq!(aabb.max.y, 8);
    }

    #[test]
    fn in_progress_aabb_extrudes_negative_offset_into_surface() {
        // Same pick but user drags the opposite direction.
        let state = SelectState {
            phase: SelectPhase::Extrude,
            anchor: Some(StrokeAnchor {
                axis: 1,
                plane_world: 5.0,
                target_layer: 5,
            }),
            corner1: Some(IVec3::new(0, 5, 0)),
            corner2: Some(IVec3::new(2, 5, 2)),
            normal_sign: 1,
            thickness: -3,
            ..Default::default()
        };
        let aabb = in_progress_aabb(&state).unwrap();
        assert_eq!(aabb.min.y, 2);
        assert_eq!(aabb.max.y, 5);
    }

    #[test]
    fn in_progress_aabb_zero_offset_is_single_cell_thick() {
        let state = SelectState {
            phase: SelectPhase::Extrude,
            anchor: Some(StrokeAnchor {
                axis: 1,
                plane_world: 5.0,
                target_layer: 5,
            }),
            corner1: Some(IVec3::new(0, 5, 0)),
            corner2: Some(IVec3::new(2, 5, 2)),
            normal_sign: 1,
            thickness: 0,
            ..Default::default()
        };
        let aabb = in_progress_aabb(&state).unwrap();
        assert_eq!(aabb.min.y, 5);
        assert_eq!(aabb.max.y, 5);
    }

    #[test]
    fn marching_segments_zero_phase_alternates_white_black() {
        let segs = marching_segments(3.0, 0.0, 1.0);
        assert_eq!(
            segs,
            vec![(0.0, 1.0, true), (1.0, 2.0, false), (2.0, 3.0, true)]
        );
    }

    #[test]
    fn marching_segments_phase_clips_leading_stripe() {
        // phase=0.5 → first white stripe rendered from 0..0.5, then black 0.5..1.5, ...
        let segs = marching_segments(3.0, 0.5, 1.0);
        assert_eq!(segs[0], (0.0, 0.5, true));
        assert_eq!(segs[1], (0.5, 1.5, false));
        assert_eq!(segs[2], (1.5, 2.5, true));
        assert_eq!(segs[3], (2.5, 3.0, false));
    }

    #[test]
    fn marching_segments_phase_cycles_with_period_two_stripes() {
        // Adding 2*stripe to phase yields the same segmentation.
        let a = marching_segments(5.0, 0.5, 1.0);
        let b = marching_segments(5.0, 0.5 + 2.0, 1.0);
        assert_eq!(a, b);
    }

    #[test]
    fn marching_segments_handles_zero_length() {
        assert!(marching_segments(0.0, 1.0, 1.0).is_empty());
    }

    #[test]
    fn marching_segments_handles_zero_stripe() {
        assert!(marching_segments(5.0, 0.0, 0.0).is_empty());
    }

    #[test]
    fn in_progress_aabb_picked_voxel_single_cell_selection() {
        // After select_input's Idle branch on a top-face pick of voxel (3,4,5):
        // target_layer == picked cell.y, corner1 == corner2 == picked cell,
        // thickness=0. AABB must equal the picked voxel exactly.
        let state = SelectState {
            phase: SelectPhase::Footprint,
            anchor: Some(StrokeAnchor {
                axis: 1,
                plane_world: 5.0,
                target_layer: 4,
            }),
            corner1: Some(IVec3::new(3, 4, 5)),
            corner2: Some(IVec3::new(3, 4, 5)),
            normal_sign: 1,
            thickness: 0,
            ..Default::default()
        };
        let aabb = in_progress_aabb(&state).unwrap();
        assert_eq!(aabb.min, IVec3::new(3, 4, 5));
        assert_eq!(aabb.max, IVec3::new(3, 4, 5));
    }

    #[test]
    fn connected_same_color_returns_single_cell_when_isolated() {
        let mut grid = VoxelGrid::default();
        let red = [200, 0, 0, 255];
        grid.set(IVec3::new(5, 5, 5), Some(red));
        let cells = connected_same_color(&grid, IVec3::new(5, 5, 5));
        assert_eq!(cells, vec![IVec3::new(5, 5, 5)]);
    }

    #[test]
    fn connected_same_color_walks_face_neighbors() {
        let mut grid = VoxelGrid::default();
        let red = [200, 0, 0, 255];
        let pts = [
            IVec3::new(0, 0, 0),
            IVec3::new(1, 0, 0),
            IVec3::new(2, 0, 0),
            IVec3::new(2, 1, 0),
            IVec3::new(2, 1, 1),
        ];
        fill_grid(&mut grid, red, &pts);
        let cells: HashSet<IVec3> = connected_same_color(&grid, IVec3::new(0, 0, 0))
            .into_iter()
            .collect();
        let expected: HashSet<IVec3> = pts.iter().copied().collect();
        assert_eq!(cells, expected);
    }

    #[test]
    fn connected_same_color_excludes_diagonal_neighbor() {
        let mut grid = VoxelGrid::default();
        let red = [200, 0, 0, 255];
        // Two same-color voxels sharing only an edge — not face-connected.
        grid.set(IVec3::new(0, 0, 0), Some(red));
        grid.set(IVec3::new(1, 1, 0), Some(red));
        let cells = connected_same_color(&grid, IVec3::new(0, 0, 0));
        assert_eq!(cells, vec![IVec3::new(0, 0, 0)]);
    }

    #[test]
    fn connected_same_color_stops_at_color_boundary() {
        let mut grid = VoxelGrid::default();
        let red = [200, 0, 0, 255];
        let blue = [0, 0, 200, 255];
        grid.set(IVec3::new(0, 0, 0), Some(red));
        grid.set(IVec3::new(1, 0, 0), Some(red));
        grid.set(IVec3::new(2, 0, 0), Some(blue));
        grid.set(IVec3::new(3, 0, 0), Some(red));
        let cells: HashSet<IVec3> = connected_same_color(&grid, IVec3::new(0, 0, 0))
            .into_iter()
            .collect();
        let expected: HashSet<IVec3> = [IVec3::new(0, 0, 0), IVec3::new(1, 0, 0)]
            .into_iter()
            .collect();
        assert_eq!(cells, expected);
    }

    #[test]
    fn connected_same_color_empty_start_returns_empty() {
        let grid = VoxelGrid::default();
        assert!(connected_same_color(&grid, IVec3::new(0, 0, 0)).is_empty());
    }

    #[test]
    fn connected_same_color_spans_negative_coords() {
        let mut grid = VoxelGrid::default();
        let red = [200, 0, 0, 255];
        let pts = [
            IVec3::new(-2, 0, -3),
            IVec3::new(-1, 0, -3),
            IVec3::new(0, 0, -3),
        ];
        fill_grid(&mut grid, red, &pts);
        let cells: HashSet<IVec3> = connected_same_color(&grid, IVec3::new(-1, 0, -3))
            .into_iter()
            .collect();
        let expected: HashSet<IVec3> = pts.iter().copied().collect();
        assert_eq!(cells, expected);
    }

    #[test]
    fn aabb_of_cells_returns_none_for_empty() {
        assert!(aabb_of_cells(&[]).is_none());
    }

    #[test]
    fn aabb_of_cells_bounds_extremes() {
        let cells = [
            IVec3::new(-2, 4, 3),
            IVec3::new(5, 0, -1),
            IVec3::new(1, 2, 7),
        ];
        let aabb = aabb_of_cells(&cells).unwrap();
        assert_eq!(aabb.min, IVec3::new(-2, 0, -1));
        assert_eq!(aabb.max, IVec3::new(5, 4, 7));
    }

    #[test]
    fn in_progress_aabb_respects_negative_normal_sign() {
        // Pick on bottom face: normal_sign = -1, target_layer below the cube.
        let state = SelectState {
            phase: SelectPhase::Extrude,
            anchor: Some(StrokeAnchor {
                axis: 1,
                plane_world: 5.0,
                target_layer: 4,
            }),
            corner1: Some(IVec3::new(0, 4, 0)),
            corner2: Some(IVec3::new(2, 4, 2)),
            normal_sign: -1,
            thickness: 3,
            ..Default::default()
        };
        // Drag in normal direction (downward) extends min below target_layer.
        let aabb = in_progress_aabb(&state).unwrap();
        assert_eq!(aabb.min.y, 1);
        assert_eq!(aabb.max.y, 4);
    }

    #[test]
    fn selection_set_cells_recomputes_aabb_hull() {
        let mut sel = Selection::default();
        let cells: HashSet<IVec3> = [IVec3::new(1, 2, 3), IVec3::new(4, 0, -1)]
            .into_iter()
            .collect();
        sel.set_cells(cells);
        let aabb = sel.aabb.expect("aabb hull");
        assert_eq!(aabb.min, IVec3::new(1, 0, -1));
        assert_eq!(aabb.max, IVec3::new(4, 2, 3));
        assert!(sel.cells.is_some());
    }

    #[test]
    fn selection_set_cells_empty_leaves_no_selection() {
        let mut sel = Selection::default();
        sel.set_cells(HashSet::new());
        assert!(sel.aabb.is_none());
        assert!(sel.cells.is_none());
    }

    #[test]
    fn selection_set_aabb_clears_cells() {
        let mut sel = Selection::default();
        sel.cells = Some([IVec3::new(0, 0, 0)].into_iter().collect());
        sel.set_aabb(SelectionAabb::from_corners(
            IVec3::new(0, 0, 0),
            IVec3::new(2, 2, 2),
        ));
        assert!(sel.cells.is_none());
    }

    #[test]
    fn selection_contains_uses_mask_when_present() {
        let mut sel = Selection::default();
        sel.set_cells(
            [IVec3::new(0, 0, 0), IVec3::new(2, 0, 0)]
                .into_iter()
                .collect(),
        );
        assert!(sel.contains(IVec3::new(0, 0, 0)));
        assert!(sel.contains(IVec3::new(2, 0, 0)));
        // (1,0,0) is inside the AABB hull but not in the mask.
        assert!(!sel.contains(IVec3::new(1, 0, 0)));
    }

    #[test]
    fn clear_selection_with_mask_only_clears_masked_cells() {
        let mut grid = VoxelGrid::default();
        let red = [200, 0, 0, 255];
        let blue = [0, 0, 200, 255];
        grid.set(IVec3::new(0, 0, 0), Some(red));
        grid.set(IVec3::new(1, 0, 0), Some(blue));
        grid.set(IVec3::new(2, 0, 0), Some(red));
        let mut sel = Selection::default();
        sel.set_cells(
            [IVec3::new(0, 0, 0), IVec3::new(2, 0, 0)]
                .into_iter()
                .collect(),
        );
        let mut history = History::default();
        clear_selection(&mut grid, &mut history, &sel);
        assert!(grid.get(IVec3::new(0, 0, 0)).is_none());
        assert_eq!(grid.get(IVec3::new(1, 0, 0)), Some(blue));
        assert!(grid.get(IVec3::new(2, 0, 0)).is_none());
    }

    #[test]
    fn recolor_selection_with_mask_only_recolors_masked_cells() {
        let mut grid = VoxelGrid::default();
        let red = [200, 0, 0, 255];
        let blue = [0, 0, 200, 255];
        let green = [0, 200, 0, 255];
        grid.set(IVec3::new(0, 0, 0), Some(red));
        grid.set(IVec3::new(1, 0, 0), Some(blue));
        grid.set(IVec3::new(2, 0, 0), Some(red));
        let mut sel = Selection::default();
        sel.set_cells(
            [IVec3::new(0, 0, 0), IVec3::new(2, 0, 0)]
                .into_iter()
                .collect(),
        );
        let mut history = History::default();
        recolor_selection(&mut grid, &mut history, &sel, &[green]);
        assert_eq!(grid.get(IVec3::new(0, 0, 0)), Some(green));
        assert_eq!(grid.get(IVec3::new(1, 0, 0)), Some(blue));
        assert_eq!(grid.get(IVec3::new(2, 0, 0)), Some(green));
    }

    #[test]
    fn silhouette_edges_single_cell_emits_twelve_edges() {
        let mask: HashSet<IVec3> = [IVec3::new(0, 0, 0)].into_iter().collect();
        let edges = silhouette_edges(&mask);
        assert_eq!(edges.len(), 12);
    }

    #[test]
    fn silhouette_edges_two_face_adjacent_cells_drop_shared_face_perimeter() {
        // Two X-adjacent cubes share a face perpendicular to X. The 4 edges
        // bounding that shared face are coplanar boundaries on +Y / -Y / +Z /
        // -Z faces of the merged shape — they're not silhouette edges.
        let mask: HashSet<IVec3> = [IVec3::new(0, 0, 0), IVec3::new(1, 0, 0)]
            .into_iter()
            .collect();
        let edges = silhouette_edges(&mask);
        // Two cubes dedupe to 20 raw lattice edges; the 4 edges on the shared
        // face plane (x=1) are K=2 same-column → dropped; remaining = 16.
        assert_eq!(edges.len(), 16);
        // Specifically the X-axis seam edges between the cubes shouldn't
        // appear (they were the only "inside seam" but X-axis edges live on
        // the +Y/+Z corners — actually the dropped edges are the 4 Y/Z edges
        // along the shared face plane at x=1).
        assert!(!edges.contains(&(1, IVec3::new(1, 0, 0))));
        assert!(!edges.contains(&(1, IVec3::new(1, 0, 1))));
        assert!(!edges.contains(&(2, IVec3::new(1, 0, 0))));
        assert!(!edges.contains(&(2, IVec3::new(1, 1, 0))));
    }

    #[test]
    fn silhouette_edges_interior_cell_in_solid_block_has_no_edges_through_it() {
        // 3x3x3 block: the center cell at (1,1,1) sits fully inside. Every
        // edge of that cell has K=4 around it and must be dropped.
        let mut mask: HashSet<IVec3> = HashSet::new();
        for x in 0..3 {
            for y in 0..3 {
                for z in 0..3 {
                    mask.insert(IVec3::new(x, y, z));
                }
            }
        }
        let edges = silhouette_edges(&mask);
        // The 3x3x3 silhouette is the perimeter of each face = 8 corner edges
        // along each face → 12 outer edges of the bounding cube, each split
        // into 3 unit segments = 36 edges.
        assert_eq!(edges.len(), 36);
    }

    #[test]
    fn silhouette_edges_diagonal_only_pair_keeps_saddle_edge() {
        // Two cells touching only at a single shared edge → K=2 diagonal at
        // that edge, K=1 elsewhere. Saddle edge must be drawn (twice, once
        // per cell, but deduped) — assert it appears.
        let mask: HashSet<IVec3> = [IVec3::new(0, 0, 0), IVec3::new(1, 1, 0)]
            .into_iter()
            .collect();
        let edges = silhouette_edges(&mask);
        // The shared edge is the Z-axis edge at (1, 1, 0).
        assert!(edges.contains(&(2, IVec3::new(1, 1, 0))));
    }

    #[test]
    fn move_selection_with_mask_shifts_only_masked_cells_and_mask() {
        let mut grid = VoxelGrid::default();
        let red = [200, 0, 0, 255];
        let blue = [0, 0, 200, 255];
        grid.set(IVec3::new(0, 0, 0), Some(red));
        grid.set(IVec3::new(1, 0, 0), Some(blue));
        grid.set(IVec3::new(2, 0, 0), Some(red));
        let mut sel = Selection::default();
        sel.set_cells(
            [IVec3::new(0, 0, 0), IVec3::new(2, 0, 0)]
                .into_iter()
                .collect(),
        );
        let mut history = History::default();
        assert!(move_selection(
            &mut grid,
            &mut history,
            &mut sel,
            IVec3::new(0, 1, 0)
        ));
        // Originals cleared, destinations lit, neighbor blue untouched.
        assert!(grid.get(IVec3::new(0, 0, 0)).is_none());
        assert_eq!(grid.get(IVec3::new(1, 0, 0)), Some(blue));
        assert!(grid.get(IVec3::new(2, 0, 0)).is_none());
        assert_eq!(grid.get(IVec3::new(0, 1, 0)), Some(red));
        assert_eq!(grid.get(IVec3::new(2, 1, 0)), Some(red));
        // Mask shifted with the move.
        let cells = sel.cells.expect("mask retained");
        assert!(cells.contains(&IVec3::new(0, 1, 0)));
        assert!(cells.contains(&IVec3::new(2, 1, 0)));
        assert!(!cells.contains(&IVec3::new(0, 0, 0)));
    }
}
