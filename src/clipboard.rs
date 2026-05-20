use crate::grid::{Color8, VoxelGrid};
use crate::history::History;
use crate::picking::{cursor_ray, pick};
use crate::select::{Selection, SelectionAabb, clear_selection};
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_panorbit_camera::PanOrbitCamera;
use std::collections::HashSet;

/// A captured region of voxels with the original AABB min as its anchor.
/// `cells` holds the occupied voxels in absolute coordinates as they were at
/// copy time; `origin` is the AABB min so paste offsets resolve as
/// `target_anchor + (cell - origin)`.
#[derive(Clone, Debug)]
pub struct Stamp {
    pub cells: Vec<(IVec3, Color8)>,
    pub origin: IVec3,
    pub aabb: SelectionAabb,
}

impl Stamp {
    pub fn voxel_count(&self) -> usize {
        self.cells.len()
    }
}

#[derive(Resource, Default)]
pub struct Clipboard {
    pub stamp: Option<Stamp>,
}

impl Clipboard {
    pub fn has_stamp(&self) -> bool {
        self.stamp.is_some()
    }
}

/// Snapshot the occupied voxels inside the active selection. Returns `None`
/// when no selection or no occupied cells. Respects the per-cell mask when
/// present so connected-region selections only copy their voxels.
pub fn copy_selection(grid: &VoxelGrid, selection: &Selection) -> Option<Stamp> {
    let aabb = selection.aabb?;
    let cells: Vec<(IVec3, Color8)> = match &selection.cells {
        Some(mask) => mask
            .iter()
            .filter_map(|p| grid.get(*p).map(|c| (*p, c)))
            .collect(),
        None => aabb
            .iter_cells()
            .filter_map(|p| grid.get(p).map(|c| (p, c)))
            .collect(),
    };
    if cells.is_empty() {
        return None;
    }
    Some(Stamp {
        cells,
        origin: aabb.min,
        aabb,
    })
}

/// Copy then clear the selection in a single undoable stroke. Returns the
/// stamp on success.
pub fn cut_selection(
    grid: &mut VoxelGrid,
    history: &mut History,
    selection: &Selection,
) -> Option<Stamp> {
    let stamp = copy_selection(grid, selection)?;
    clear_selection(grid, history, selection);
    Some(stamp)
}

/// Write the stamp's voxels into `grid` anchored at `target_anchor` (which
/// maps to the stamp's `origin`). Destination cells are overwritten in one
/// history stroke. Returns the new AABB of the pasted region, or `None` if
/// any destination would land below the floor (whole paste refused — keeps
/// stamp shape intact).
pub fn paste_stamp(
    grid: &mut VoxelGrid,
    history: &mut History,
    stamp: &Stamp,
    target_anchor: IVec3,
) -> Option<SelectionAabb> {
    let delta = target_anchor - stamp.origin;
    let new_min = stamp.aabb.min + delta;
    let new_max = stamp.aabb.max + delta;
    if new_min.y < 0 {
        return None;
    }
    history.begin();
    for (src, color) in &stamp.cells {
        let dst = *src + delta;
        if dst.y < 0 {
            continue;
        }
        history.record(grid, dst, Some(*color));
    }
    history.end();
    Some(SelectionAabb {
        min: new_min,
        max: new_max,
    })
}

/// Build the cell mask of a pasted region — used to update `Selection.cells`
/// so a connected-mask selection survives a paste.
pub fn pasted_mask(stamp: &Stamp, target_anchor: IVec3) -> HashSet<IVec3> {
    let delta = target_anchor - stamp.origin;
    stamp.cells.iter().map(|(p, _)| *p + delta).collect()
}

/// Resolve the paste anchor for a stamp. Priority:
/// 1. Cursor pick — if the cursor ray hits a voxel, anchor at `hit.cell +
///    hit.normal` (paste sits on top of the hovered face, like the Brush
///    tool). If the ray falls through to the floor, anchor at the floor cell.
/// 2. Active selection — anchor at `selection.aabb.min` so a deliberately
///    placed selection takes precedence over an out-of-viewport cursor.
/// 3. Stamp origin — paste-in-place fallback when there is no cursor and no
///    selection (e.g. invoked from the command palette with focus elsewhere).
pub fn resolve_paste_anchor(
    stamp: &Stamp,
    cursor_hit: Option<IVec3>,
    selection: &Selection,
) -> IVec3 {
    if let Some(cell) = cursor_hit {
        return cell;
    }
    if let Some(aabb) = selection.aabb {
        return aabb.min;
    }
    stamp.origin
}

/// Run the full paste flow shared by the keyboard shortcut, the macOS menu,
/// and the command palette. `cursor_hit` is the cell the paste should anchor
/// on (top of a hovered face) when available; menu / palette callers pass
/// `None` and the anchor falls back to the active selection / stamp origin.
/// On success the selection updates to the pasted region (preserving mask vs
/// AABB shape) and a toast is emitted; below-floor pastes are refused with an
/// error toast.
pub fn execute_paste(
    grid: &mut VoxelGrid,
    history: &mut History,
    selection: &mut Selection,
    toasts: &mut crate::ui::Toasts,
    stamp: &Stamp,
    cursor_hit: Option<IVec3>,
) {
    let anchor = resolve_paste_anchor(stamp, cursor_hit, selection);
    let had_mask = selection.cells.is_some();
    let pasted_set = pasted_mask(stamp, anchor);
    match paste_stamp(grid, history, stamp, anchor) {
        Some(new_aabb) => {
            if had_mask {
                selection.set_cells(pasted_set);
            } else {
                selection.set_aabb(new_aabb);
            }
            toasts.info(format!("Pasted {} voxels", stamp.voxel_count()));
        }
        None => toasts.error("Paste blocked: would land below floor"),
    }
}

/// Cmd+C / Cmd+X / Cmd+V handler. Paste anchors at the cell the cursor is
/// hovering (`hit.cell + hit.normal`), falling back to the active selection's
/// AABB min, then to the stamp's original origin. After paste, the selection
/// updates to the pasted region so the user can nudge with the Move tool.
pub fn clipboard_key_system(
    mut contexts: bevy_egui::EguiContexts,
    keys: Res<ButtonInput<KeyCode>>,
    mut clipboard: ResMut<Clipboard>,
    mut grid: ResMut<VoxelGrid>,
    mut history: ResMut<History>,
    mut selection: ResMut<Selection>,
    mut toasts: ResMut<crate::ui::Toasts>,
    cameras: Query<(&Camera, &GlobalTransform), With<PanOrbitCamera>>,
    windows: Query<&Window, With<PrimaryWindow>>,
) {
    let egui_wants = contexts
        .ctx_mut()
        .map(|c| c.wants_keyboard_input())
        .unwrap_or(false);
    if egui_wants {
        return;
    }
    let cmd = keys.pressed(KeyCode::SuperLeft)
        || keys.pressed(KeyCode::SuperRight)
        || keys.pressed(KeyCode::ControlLeft)
        || keys.pressed(KeyCode::ControlRight);
    if !cmd {
        return;
    }
    if keys.just_pressed(KeyCode::KeyC) {
        if let Some(stamp) = copy_selection(&grid, &selection) {
            let n = stamp.voxel_count();
            clipboard.stamp = Some(stamp);
            toasts.info(format!("Copied {n} voxels"));
        }
        return;
    }
    if keys.just_pressed(KeyCode::KeyX) {
        if let Some(stamp) = cut_selection(&mut grid, &mut history, &selection) {
            let n = stamp.voxel_count();
            clipboard.stamp = Some(stamp);
            toasts.info(format!("Cut {n} voxels"));
        }
        return;
    }
    if keys.just_pressed(KeyCode::KeyV) {
        let Some(stamp) = clipboard.stamp.clone() else {
            return;
        };
        let cursor_hit = cursor_ray(&cameras, &windows)
            .and_then(|(o, d)| pick(&grid, o, d))
            .map(|h| h.cell + h.normal);
        execute_paste(
            &mut grid,
            &mut history,
            &mut selection,
            &mut toasts,
            &stamp,
            cursor_hit,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fill(grid: &mut VoxelGrid, cells: &[(IVec3, Color8)]) {
        for (p, c) in cells {
            grid.set(*p, Some(*c));
        }
    }

    fn red() -> Color8 {
        [255, 0, 0, 255]
    }

    fn green() -> Color8 {
        [0, 255, 0, 255]
    }

    fn sample_stamp() -> Stamp {
        Stamp {
            cells: vec![(IVec3::new(0, 0, 0), red())],
            origin: IVec3::new(0, 0, 0),
            aabb: SelectionAabb::from_corners(IVec3::new(0, 0, 0), IVec3::new(0, 0, 0)),
        }
    }

    #[test]
    fn anchor_prefers_cursor_over_selection_and_origin() {
        let stamp = sample_stamp();
        let mut sel = Selection::default();
        sel.set_aabb(SelectionAabb::from_corners(
            IVec3::new(10, 0, 10),
            IVec3::new(10, 0, 10),
        ));
        let anchor = resolve_paste_anchor(&stamp, Some(IVec3::new(4, 2, 6)), &sel);
        assert_eq!(anchor, IVec3::new(4, 2, 6));
    }

    #[test]
    fn anchor_falls_back_to_selection_when_no_cursor() {
        let stamp = sample_stamp();
        let mut sel = Selection::default();
        sel.set_aabb(SelectionAabb::from_corners(
            IVec3::new(7, 0, 7),
            IVec3::new(8, 0, 8),
        ));
        let anchor = resolve_paste_anchor(&stamp, None, &sel);
        assert_eq!(anchor, IVec3::new(7, 0, 7));
    }

    #[test]
    fn anchor_falls_back_to_stamp_origin_with_no_cursor_or_selection() {
        let stamp = Stamp {
            cells: vec![(IVec3::new(3, 4, 5), red())],
            origin: IVec3::new(3, 4, 5),
            aabb: SelectionAabb::from_corners(IVec3::new(3, 4, 5), IVec3::new(3, 4, 5)),
        };
        let sel = Selection::default();
        let anchor = resolve_paste_anchor(&stamp, None, &sel);
        assert_eq!(anchor, IVec3::new(3, 4, 5));
    }

    #[test]
    fn copy_returns_none_when_no_selection() {
        let grid = VoxelGrid::default();
        let sel = Selection::default();
        assert!(copy_selection(&grid, &sel).is_none());
    }

    #[test]
    fn copy_returns_none_when_selection_empty_of_voxels() {
        let grid = VoxelGrid::default();
        let mut sel = Selection::default();
        sel.set_aabb(SelectionAabb::from_corners(
            IVec3::new(0, 0, 0),
            IVec3::new(2, 2, 2),
        ));
        assert!(copy_selection(&grid, &sel).is_none());
    }

    #[test]
    fn copy_captures_aabb_voxels_with_origin_at_min() {
        let mut grid = VoxelGrid::default();
        fill(
            &mut grid,
            &[(IVec3::new(2, 0, 2), red()), (IVec3::new(3, 0, 2), green())],
        );
        let mut sel = Selection::default();
        sel.set_aabb(SelectionAabb::from_corners(
            IVec3::new(2, 0, 2),
            IVec3::new(3, 0, 2),
        ));
        let stamp = copy_selection(&grid, &sel).expect("stamp");
        assert_eq!(stamp.origin, IVec3::new(2, 0, 2));
        assert_eq!(stamp.cells.len(), 2);
    }

    #[test]
    fn copy_respects_cell_mask() {
        let mut grid = VoxelGrid::default();
        fill(
            &mut grid,
            &[
                (IVec3::new(0, 0, 0), red()),
                (IVec3::new(1, 0, 0), red()),
                (IVec3::new(2, 0, 0), red()),
            ],
        );
        let mut sel = Selection::default();
        let mask: HashSet<IVec3> = [IVec3::new(0, 0, 0), IVec3::new(2, 0, 0)]
            .into_iter()
            .collect();
        sel.set_cells(mask);
        let stamp = copy_selection(&grid, &sel).expect("stamp");
        assert_eq!(stamp.cells.len(), 2);
        let coords: HashSet<IVec3> = stamp.cells.iter().map(|(p, _)| *p).collect();
        assert!(coords.contains(&IVec3::new(0, 0, 0)));
        assert!(coords.contains(&IVec3::new(2, 0, 0)));
        assert!(!coords.contains(&IVec3::new(1, 0, 0)));
    }

    #[test]
    fn cut_returns_stamp_and_clears_source() {
        let mut grid = VoxelGrid::default();
        let mut history = History::default();
        fill(
            &mut grid,
            &[(IVec3::new(0, 0, 0), red()), (IVec3::new(1, 0, 0), red())],
        );
        let mut sel = Selection::default();
        sel.set_aabb(SelectionAabb::from_corners(
            IVec3::new(0, 0, 0),
            IVec3::new(1, 0, 0),
        ));
        let stamp = cut_selection(&mut grid, &mut history, &sel).expect("stamp");
        assert_eq!(stamp.cells.len(), 2);
        assert!(grid.get(IVec3::new(0, 0, 0)).is_none());
        assert!(grid.get(IVec3::new(1, 0, 0)).is_none());
    }

    #[test]
    fn cut_then_undo_restores_voxels() {
        let mut grid = VoxelGrid::default();
        let mut history = History::default();
        fill(&mut grid, &[(IVec3::new(0, 0, 0), red())]);
        let mut sel = Selection::default();
        sel.set_aabb(SelectionAabb::from_corners(
            IVec3::new(0, 0, 0),
            IVec3::new(0, 0, 0),
        ));
        cut_selection(&mut grid, &mut history, &sel).expect("stamp");
        history.undo(&mut grid);
        assert_eq!(grid.get(IVec3::new(0, 0, 0)), Some(red()));
    }

    #[test]
    fn paste_offsets_voxels_by_anchor_delta() {
        let mut grid = VoxelGrid::default();
        let mut history = History::default();
        let stamp = Stamp {
            cells: vec![(IVec3::new(0, 0, 0), red()), (IVec3::new(1, 0, 0), green())],
            origin: IVec3::new(0, 0, 0),
            aabb: SelectionAabb::from_corners(IVec3::new(0, 0, 0), IVec3::new(1, 0, 0)),
        };
        let new =
            paste_stamp(&mut grid, &mut history, &stamp, IVec3::new(5, 0, 5)).expect("new aabb");
        assert_eq!(new.min, IVec3::new(5, 0, 5));
        assert_eq!(grid.get(IVec3::new(5, 0, 5)), Some(red()));
        assert_eq!(grid.get(IVec3::new(6, 0, 5)), Some(green()));
    }

    #[test]
    fn paste_in_place_is_identity() {
        let mut grid = VoxelGrid::default();
        let mut history = History::default();
        fill(&mut grid, &[(IVec3::new(3, 0, 3), red())]);
        let mut sel = Selection::default();
        sel.set_aabb(SelectionAabb::from_corners(
            IVec3::new(3, 0, 3),
            IVec3::new(3, 0, 3),
        ));
        let stamp = copy_selection(&grid, &sel).expect("stamp");
        paste_stamp(&mut grid, &mut history, &stamp, stamp.origin).expect("ok");
        assert_eq!(grid.get(IVec3::new(3, 0, 3)), Some(red()));
    }

    #[test]
    fn paste_below_floor_is_refused() {
        let mut grid = VoxelGrid::default();
        let mut history = History::default();
        let stamp = Stamp {
            cells: vec![(IVec3::new(0, 0, 0), red())],
            origin: IVec3::new(0, 0, 0),
            aabb: SelectionAabb::from_corners(IVec3::new(0, 0, 0), IVec3::new(0, 0, 0)),
        };
        assert!(paste_stamp(&mut grid, &mut history, &stamp, IVec3::new(0, -1, 0)).is_none());
        assert!(grid.get(IVec3::new(0, 0, 0)).is_none());
    }

    #[test]
    fn paste_overwrites_destination() {
        let mut grid = VoxelGrid::default();
        let mut history = History::default();
        fill(&mut grid, &[(IVec3::new(5, 0, 5), green())]);
        let stamp = Stamp {
            cells: vec![(IVec3::new(0, 0, 0), red())],
            origin: IVec3::new(0, 0, 0),
            aabb: SelectionAabb::from_corners(IVec3::new(0, 0, 0), IVec3::new(0, 0, 0)),
        };
        paste_stamp(&mut grid, &mut history, &stamp, IVec3::new(5, 0, 5)).expect("ok");
        assert_eq!(grid.get(IVec3::new(5, 0, 5)), Some(red()));
    }

    #[test]
    fn paste_then_undo_clears_pasted_voxels() {
        let mut grid = VoxelGrid::default();
        let mut history = History::default();
        let stamp = Stamp {
            cells: vec![(IVec3::new(0, 0, 0), red()), (IVec3::new(1, 0, 0), red())],
            origin: IVec3::new(0, 0, 0),
            aabb: SelectionAabb::from_corners(IVec3::new(0, 0, 0), IVec3::new(1, 0, 0)),
        };
        paste_stamp(&mut grid, &mut history, &stamp, IVec3::new(2, 0, 0)).expect("ok");
        history.undo(&mut grid);
        assert!(grid.get(IVec3::new(2, 0, 0)).is_none());
        assert!(grid.get(IVec3::new(3, 0, 0)).is_none());
    }

    #[test]
    fn execute_paste_updates_aabb_selection_and_emits_toast() {
        let mut grid = VoxelGrid::default();
        let mut history = History::default();
        let mut selection = Selection::default();
        let mut toasts = crate::ui::Toasts::default();
        let stamp = Stamp {
            cells: vec![(IVec3::new(0, 0, 0), red()), (IVec3::new(1, 0, 0), red())],
            origin: IVec3::new(0, 0, 0),
            aabb: SelectionAabb::from_corners(IVec3::new(0, 0, 0), IVec3::new(1, 0, 0)),
        };
        execute_paste(
            &mut grid,
            &mut history,
            &mut selection,
            &mut toasts,
            &stamp,
            Some(IVec3::new(5, 0, 5)),
        );
        assert_eq!(grid.get(IVec3::new(5, 0, 5)), Some(red()));
        assert_eq!(grid.get(IVec3::new(6, 0, 5)), Some(red()));
        assert_eq!(selection.aabb.unwrap().min, IVec3::new(5, 0, 5));
        assert!(selection.cells.is_none());
        assert_eq!(toasts.0.len(), 1);
    }

    #[test]
    fn execute_paste_preserves_mask_shape() {
        let mut grid = VoxelGrid::default();
        let mut history = History::default();
        let mut selection = Selection::default();
        let mut toasts = crate::ui::Toasts::default();
        selection.set_cells([IVec3::new(0, 0, 0)].into_iter().collect());
        let stamp = Stamp {
            cells: vec![(IVec3::new(0, 0, 0), red())],
            origin: IVec3::new(0, 0, 0),
            aabb: SelectionAabb::from_corners(IVec3::new(0, 0, 0), IVec3::new(0, 0, 0)),
        };
        execute_paste(
            &mut grid,
            &mut history,
            &mut selection,
            &mut toasts,
            &stamp,
            Some(IVec3::new(2, 0, 2)),
        );
        let mask = selection.cells.expect("mask retained");
        assert!(mask.contains(&IVec3::new(2, 0, 2)));
        assert_eq!(mask.len(), 1);
    }

    #[test]
    fn execute_paste_below_floor_emits_error_toast() {
        let mut grid = VoxelGrid::default();
        let mut history = History::default();
        let mut selection = Selection::default();
        let mut toasts = crate::ui::Toasts::default();
        let stamp = Stamp {
            cells: vec![(IVec3::new(0, 0, 0), red())],
            origin: IVec3::new(0, 0, 0),
            aabb: SelectionAabb::from_corners(IVec3::new(0, 0, 0), IVec3::new(0, 0, 0)),
        };
        execute_paste(
            &mut grid,
            &mut history,
            &mut selection,
            &mut toasts,
            &stamp,
            Some(IVec3::new(0, -1, 0)),
        );
        assert!(grid.get(IVec3::new(0, 0, 0)).is_none());
        assert_eq!(toasts.0.len(), 1);
        assert_eq!(toasts.0[0].kind, crate::ui::toast::ToastKind::Error);
    }

    #[test]
    fn pasted_mask_offsets_cells() {
        let stamp = Stamp {
            cells: vec![(IVec3::new(0, 0, 0), red()), (IVec3::new(2, 0, 0), red())],
            origin: IVec3::new(0, 0, 0),
            aabb: SelectionAabb::from_corners(IVec3::new(0, 0, 0), IVec3::new(2, 0, 0)),
        };
        let mask = pasted_mask(&stamp, IVec3::new(5, 0, 5));
        assert!(mask.contains(&IVec3::new(5, 0, 5)));
        assert!(mask.contains(&IVec3::new(7, 0, 5)));
        assert_eq!(mask.len(), 2);
    }
}
