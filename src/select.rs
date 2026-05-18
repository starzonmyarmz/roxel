use crate::grid::{Color8, VoxelGrid};
use crate::history::History;
use crate::tools::{StrokeAnchor, Tool, ToolState};
use bevy::prelude::*;

/// Gizmo group for selection visuals: marching-ants AABB outline + per-cell
/// wireframe markers. Configured with `depth_bias = -1.0` so the overlay
/// x-rays through voxels — users need to see which cells are selected inside
/// a solid block.
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

#[derive(Resource, Default)]
pub struct Selection {
    pub aabb: Option<SelectionAabb>,
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
pub fn clear_aabb(grid: &mut VoxelGrid, history: &mut History, aabb: &SelectionAabb) {
    history.begin();
    for cell in aabb.iter_cells() {
        if grid.in_bounds(cell) && grid.get(cell).is_some() {
            history.record(grid, cell, None);
        }
    }
    history.end();
}

/// Recolor every non-empty cell inside the AABB with `color`. Empty cells stay
/// empty — Paint must not materialize new voxels. One history stroke.
pub fn recolor_aabb(
    grid: &mut VoxelGrid,
    history: &mut History,
    aabb: &SelectionAabb,
    color: Color8,
) {
    history.begin();
    for cell in aabb.iter_cells() {
        if grid.in_bounds(cell) && grid.get(cell).is_some() {
            history.record(grid, cell, Some(color));
        }
    }
    history.end();
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
    // destination-writes overlap cleanly inside one stroke.
    let occupied: Vec<(IVec3, Color8)> = aabb
        .iter_cells()
        .filter_map(|p| grid.get(p).map(|c| (p, c)))
        .collect();

    if occupied.is_empty() {
        selection.aabb = Some(SelectionAabb {
            min: new_min,
            max: new_max,
        });
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
    true
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
    _grid: Res<VoxelGrid>,
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
    let phase = time.elapsed_secs() * STRIPE_SPEED;
    for (a, b) in edges {
        draw_marching_edge(&mut gizmos, corners[a], corners[b], phase);
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
            selection.aabb = None;
        }
        return;
    }
    let cmd = keys.pressed(KeyCode::SuperLeft)
        || keys.pressed(KeyCode::SuperRight)
        || keys.pressed(KeyCode::ControlLeft)
        || keys.pressed(KeyCode::ControlRight);
    if cmd && keys.just_pressed(KeyCode::KeyD) && selection.aabb.is_some() {
        selection.aabb = None;
        return;
    }
    if cmd
        && keys.just_pressed(KeyCode::KeyA)
        && let Some((min, max)) = grid.bounding_box()
    {
        selection.aabb = Some(SelectionAabb { min, max });
        return;
    }
    if (keys.just_pressed(KeyCode::Backspace) || keys.just_pressed(KeyCode::Delete))
        && let Some(aabb) = selection.aabb
        && select_state.phase == SelectPhase::Idle
    {
        clear_aabb(&mut grid, &mut history, &aabb);
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
    fn select_all_uses_grid_bounding_box() {
        let mut grid = VoxelGrid::default();
        let c = [10, 20, 30, 255];
        grid.set(IVec3::new(-2, 0, 3), Some(c));
        grid.set(IVec3::new(5, 4, -1), Some(c));
        let (min, max) = grid.bounding_box().expect("bb");
        let sel = SelectionAabb { min, max };
        assert!(sel.contains(IVec3::new(-2, 0, 3)));
        assert!(sel.contains(IVec3::new(5, 4, -1)));
        assert_eq!(sel.min, IVec3::new(-2, 0, -1));
        assert_eq!(sel.max, IVec3::new(5, 4, 3));
    }

    #[test]
    fn select_all_on_empty_grid_returns_none() {
        let grid = VoxelGrid::default();
        assert!(grid.bounding_box().is_none());
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
        recolor_aabb(&mut grid, &mut history, &s, blue);
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
        recolor_aabb(&mut grid, &mut history, &s, blue);
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
        recolor_aabb(&mut grid, &mut history, &s, blue);
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
        recolor_aabb(&mut grid, &mut history, &s, blue);
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
        };
        let aabb = in_progress_aabb(&state).unwrap();
        assert_eq!(aabb.min, IVec3::new(3, 4, 5));
        assert_eq!(aabb.max, IVec3::new(3, 4, 5));
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
        };
        // Drag in normal direction (downward) extends min below target_layer.
        let aabb = in_progress_aabb(&state).unwrap();
        assert_eq!(aabb.min.y, 1);
        assert_eq!(aabb.max.y, 4);
    }
}
