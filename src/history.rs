use crate::grid::{Color8, VoxelGrid};
use bevy::prelude::*;
use std::collections::HashSet;

#[derive(Clone, Copy, Debug)]
pub struct CellDelta {
    pub pos: IVec3,
    pub before: Option<Color8>,
    pub after: Option<Color8>,
}

#[derive(Default)]
pub struct Stroke {
    pub deltas: Vec<CellDelta>,
    pub touched: HashSet<(i32, i32, i32)>,
}

#[derive(Resource, Default)]
pub struct History {
    pub undo: Vec<Stroke>,
    pub redo: Vec<Stroke>,
    pub current: Option<Stroke>,
}

const MAX_UNDO: usize = 200;

impl History {
    pub fn begin(&mut self) {
        self.current = Some(Stroke::default());
    }

    pub fn record(&mut self, grid: &mut VoxelGrid, pos: IVec3, after: Option<Color8>) {
        let Some(stroke) = self.current.as_mut() else { return; };
        let key = (pos.x, pos.y, pos.z);
        if stroke.touched.contains(&key) {
            // Already touched this stroke — overwrite without doubling history.
            grid.set(pos, after);
            if let Some(d) = stroke.deltas.iter_mut().find(|d| d.pos == pos) {
                d.after = after;
            }
            return;
        }
        let before = grid.get(pos);
        if before == after {
            return;
        }
        grid.set(pos, after);
        stroke.touched.insert(key);
        stroke.deltas.push(CellDelta { pos, before, after });
    }

    pub fn end(&mut self) {
        let Some(stroke) = self.current.take() else { return; };
        if stroke.deltas.is_empty() {
            return;
        }
        self.undo.push(stroke);
        if self.undo.len() > MAX_UNDO {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    pub fn undo(&mut self, grid: &mut VoxelGrid) {
        let Some(stroke) = self.undo.pop() else { return; };
        for d in stroke.deltas.iter().rev() {
            grid.set(d.pos, d.before);
        }
        self.redo.push(stroke);
    }

    pub fn redo(&mut self, grid: &mut VoxelGrid) {
        let Some(stroke) = self.redo.pop() else { return; };
        for d in &stroke.deltas {
            grid.set(d.pos, d.after);
        }
        self.undo.push(stroke);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(h: &mut History, g: &mut VoxelGrid, p: IVec3, c: Option<Color8>) {
        h.record(g, p, c);
    }

    #[test]
    fn record_undo_redo_round_trip() {
        let mut g = VoxelGrid::default();
        let mut h = History::default();
        let p = IVec3::new(1, 2, 3);
        h.begin();
        rec(&mut h, &mut g, p, Some([10, 20, 30, 255]));
        h.end();
        assert_eq!(g.get(p), Some([10, 20, 30, 255]));
        h.undo(&mut g);
        assert_eq!(g.get(p), None);
        h.redo(&mut g);
        assert_eq!(g.get(p), Some([10, 20, 30, 255]));
    }

    #[test]
    fn record_without_begin_is_noop() {
        let mut g = VoxelGrid::default();
        let mut h = History::default();
        h.record(&mut g, IVec3::new(0, 0, 0), Some([1, 1, 1, 255]));
        assert_eq!(g.get(IVec3::ZERO), None);
        assert!(h.undo.is_empty());
    }

    #[test]
    fn duplicate_cell_in_stroke_dedupes_to_one_delta() {
        let mut g = VoxelGrid::default();
        let mut h = History::default();
        let p = IVec3::new(0, 0, 0);
        h.begin();
        rec(&mut h, &mut g, p, Some([1, 1, 1, 255]));
        rec(&mut h, &mut g, p, Some([2, 2, 2, 255]));
        rec(&mut h, &mut g, p, Some([3, 3, 3, 255]));
        h.end();
        assert_eq!(h.undo.len(), 1);
        assert_eq!(h.undo[0].deltas.len(), 1);
        assert_eq!(g.get(p), Some([3, 3, 3, 255]));
        h.undo(&mut g);
        assert_eq!(g.get(p), None);
    }

    #[test]
    fn no_op_record_is_skipped() {
        let mut g = VoxelGrid::default();
        let mut h = History::default();
        h.begin();
        rec(&mut h, &mut g, IVec3::ZERO, None);
        h.end();
        assert!(h.undo.is_empty());
    }

    #[test]
    fn empty_stroke_not_pushed() {
        let mut h = History::default();
        h.begin();
        h.end();
        assert!(h.undo.is_empty());
    }

    #[test]
    fn new_stroke_clears_redo() {
        let mut g = VoxelGrid::default();
        let mut h = History::default();
        h.begin();
        rec(&mut h, &mut g, IVec3::new(0, 0, 0), Some([1, 1, 1, 255]));
        h.end();
        h.undo(&mut g);
        assert_eq!(h.redo.len(), 1);
        h.begin();
        rec(&mut h, &mut g, IVec3::new(1, 1, 1), Some([2, 2, 2, 255]));
        h.end();
        assert!(h.redo.is_empty());
    }

    #[test]
    fn undo_cap_at_200() {
        let mut g = VoxelGrid::default();
        let mut h = History::default();
        for i in 0..205 {
            h.begin();
            rec(&mut h, &mut g, IVec3::new(i % 60, 0, 0), Some([i as u8, 0, 0, 255]));
            h.end();
        }
        assert_eq!(h.undo.len(), 200);
    }

    #[test]
    fn multi_cell_undo_restores_all() {
        let mut g = VoxelGrid::default();
        let mut h = History::default();
        let pts = [IVec3::new(0,0,0), IVec3::new(1,0,0), IVec3::new(2,0,0)];
        h.begin();
        for p in pts {
            rec(&mut h, &mut g, p, Some([9, 9, 9, 255]));
        }
        h.end();
        for p in pts { assert_eq!(g.get(p), Some([9,9,9,255])); }
        h.undo(&mut g);
        for p in pts { assert_eq!(g.get(p), None); }
    }
}
