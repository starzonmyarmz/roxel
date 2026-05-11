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
