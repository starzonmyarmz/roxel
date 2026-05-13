use crate::grid::{Color8, VoxelGrid};
use crate::history::History;
use crate::shape_preview::build_cubes_mesh;
use crate::theme::Theme;
use crate::tools::{StrokeAnchor, Tool, ToolState};
use bevy::asset::RenderAssetUsages;
use bevy::ecs::system::SystemParam;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;

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
        p.x >= self.min.x && p.x <= self.max.x
            && p.y >= self.min.y && p.y <= self.max.y
            && p.z >= self.min.z && p.z <= self.max.z
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

/// Build an in-progress AABB from the current `SelectState` corners + thickness
/// along the anchor axis. Returns None during `Idle`.
pub fn in_progress_aabb(state: &SelectState) -> Option<SelectionAabb> {
    let (Some(anchor), Some(c1), Some(c2)) = (state.anchor, state.corner1, state.corner2)
    else {
        return None;
    };
    let depth_end = anchor.target_layer + (state.thickness.max(1) - 1) * state.normal_sign;
    let mut a = c1.to_array();
    let mut b = c2.to_array();
    a[anchor.axis] = anchor.target_layer;
    b[anchor.axis] = depth_end;
    Some(SelectionAabb::from_corners(
        IVec3::from_array(a),
        IVec3::from_array(b),
    ))
}

#[derive(Component)]
pub struct SelectionPreview;

#[derive(Resource)]
pub struct SelectionPreviewHandles {
    pub mesh: Handle<Mesh>,
    pub material: Handle<StandardMaterial>,
}

fn empty_mesh() -> Mesh {
    let mut m = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    m.insert_attribute(Mesh::ATTRIBUTE_POSITION, Vec::<[f32; 3]>::new());
    m.insert_attribute(Mesh::ATTRIBUTE_NORMAL, Vec::<[f32; 3]>::new());
    m.insert_indices(Indices::U32(Vec::new()));
    m
}

pub fn spawn_selection_preview(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
) {
    let mesh = meshes.add(empty_mesh());
    let material = materials.add(StandardMaterial {
        base_color: Color::srgba(0.35, 0.55, 1.0, 0.28),
        unlit: true,
        alpha_mode: AlphaMode::Blend,
        cull_mode: None,
        ..default()
    });
    commands.spawn((
        Mesh3d(mesh.clone()),
        MeshMaterial3d(material.clone()),
        Transform::default(),
        Visibility::Hidden,
        SelectionPreview,
    ));
    commands.insert_resource(SelectionPreviewHandles { mesh, material });
}

#[derive(SystemParam)]
pub struct SelectionRenderParams<'w, 's> {
    pub selection: Res<'w, Selection>,
    pub state: Res<'w, SelectState>,
    pub grid: Res<'w, VoxelGrid>,
    pub theme: Res<'w, Theme>,
    pub handles: Res<'w, SelectionPreviewHandles>,
    pub meshes: ResMut<'w, Assets<Mesh>>,
    pub materials: ResMut<'w, Assets<StandardMaterial>>,
    pub preview_q: Query<'w, 's, &'static mut Visibility, With<SelectionPreview>>,
}

pub fn selection_render_system(mut p: SelectionRenderParams, mut gizmos: Gizmos) {
    // In-progress drag takes priority over a committed selection so the user
    // sees the new region they're drawing.
    let active_aabb = if p.state.phase != SelectPhase::Idle {
        in_progress_aabb(&p.state).or(p.selection.aabb)
    } else {
        p.selection.aabb
    };

    let Ok(mut vis) = p.preview_q.single_mut() else { return; };

    let accent = p.theme.accent;
    // Refresh material color so theme switches recolor the overlay.
    if let Some(mat) = p.materials.get_mut(&p.handles.material) {
        mat.base_color = Color::srgba(
            accent.r() as f32 / 255.0,
            accent.g() as f32 / 255.0,
            accent.b() as f32 / 255.0,
            0.25,
        );
    }

    let Some(aabb) = active_aabb else {
        *vis = Visibility::Hidden;
        if let Some(mesh) = p.meshes.get_mut(&p.handles.mesh) {
            *mesh = empty_mesh();
        }
        return;
    };

    let mut filled_cells: Vec<IVec3> = Vec::new();
    for cell in aabb.iter_cells() {
        if p.grid.in_bounds(cell) && p.grid.get(cell).is_some() {
            filled_cells.push(cell);
        }
    }

    if let Some(mesh) = p.meshes.get_mut(&p.handles.mesh) {
        let (pos, nor, idx) = build_cubes_mesh(&filled_cells);
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, pos);
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, nor);
        mesh.insert_indices(Indices::U32(idx));
    }
    *vis = Visibility::Visible;

    // AABB outline. Slightly inflated so it sits outside cell faces.
    let pad = 0.01;
    let min = Vec3::new(aabb.min.x as f32 - pad, aabb.min.y as f32 - pad, aabb.min.z as f32 - pad);
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
        (0, 1), (1, 2), (2, 3), (3, 0),
        (4, 5), (5, 6), (6, 7), (7, 4),
        (0, 4), (1, 5), (2, 6), (3, 7),
    ];
    let line_color = Color::srgba(
        accent.r() as f32 / 255.0,
        accent.g() as f32 / 255.0,
        accent.b() as f32 / 255.0,
        1.0,
    );
    for (a, b) in edges {
        gizmos.line(corners[a], corners[b], line_color);
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
    if (keys.just_pressed(KeyCode::Backspace) || keys.just_pressed(KeyCode::Delete))
        && let Some(aabb) = selection.aabb
        && select_state.phase == SelectPhase::Idle
    {
        clear_aabb(&mut grid, &mut history, &aabb);
    }
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
        fill_grid(&mut grid, red, &[
            IVec3::new(1, 1, 1), IVec3::new(2, 1, 1), IVec3::new(5, 5, 5),
        ]);
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
        let touched = [IVec3::new(1, 1, 1), IVec3::new(2, 1, 1), IVec3::new(3, 1, 1)];
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
            assert!(grid.get(cell).is_none(), "cell {cell:?} should remain empty");
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
        let touched = [IVec3::new(1, 1, 1), IVec3::new(2, 2, 2), IVec3::new(3, 1, 1)];
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
}
