use bevy::prelude::*;
use std::collections::HashMap;

use crate::ui::Toasts;
use roxel::grid::{Color8, VoxelGrid};
use roxel::history::History;

#[derive(Clone, Copy, Debug)]
pub enum ResampleOp {
    Double,
    Halve,
}

pub struct ResamplePlan {
    pub cells_to_clear: Vec<IVec3>,
    pub cells_to_write: Vec<(IVec3, Color8)>,
    pub source_count: usize,
    pub result_count: usize,
    pub lossy: bool,
}

/// Build the cell-level rewrite plan for a density change. Pure over
/// `&VoxelGrid` so it's testable without spinning up a Bevy app.
///
/// `Double`: each source voxel emits a 2×2×2 block of the same color at
/// `(p*2 .. p*2+2)`. Always lossless.
///
/// `Halve`: source cells are grouped by `p.div_euclid(2)` and each block
/// collapses to one voxel using the majority color (ties resolved by
/// first-seen iteration order — deterministic per call but not stable
/// across runs since `VoxelGrid::iter_occupied` walks a `HashMap`). Sets
/// `lossy = true` if any block contained more than one distinct color.
pub fn plan_resample(grid: &VoxelGrid, op: ResampleOp) -> ResamplePlan {
    let cells_to_clear: Vec<IVec3> = grid.iter_occupied().map(|(p, _)| p).collect();
    let source_count = cells_to_clear.len();

    match op {
        ResampleOp::Double => {
            let mut cells_to_write = Vec::with_capacity(source_count * 8);
            for (p, c) in grid.iter_occupied() {
                let base = p * 2;
                for dx in 0..2 {
                    for dy in 0..2 {
                        for dz in 0..2 {
                            cells_to_write.push((base + IVec3::new(dx, dy, dz), c));
                        }
                    }
                }
            }
            let result_count = cells_to_write.len();
            ResamplePlan {
                cells_to_clear,
                cells_to_write,
                source_count,
                result_count,
                lossy: false,
            }
        }
        ResampleOp::Halve => {
            // block -> (first_color_seen, color -> count, distinct_count).
            // We track first-seen to make ties deterministic per call.
            struct Bucket {
                first: Color8,
                counts: HashMap<Color8, u32>,
            }
            let mut blocks: HashMap<IVec3, Bucket> = HashMap::new();
            for (p, c) in grid.iter_occupied() {
                let block = IVec3::new(p.x.div_euclid(2), p.y.div_euclid(2), p.z.div_euclid(2));
                blocks
                    .entry(block)
                    .and_modify(|b| {
                        *b.counts.entry(c).or_insert(0) += 1;
                    })
                    .or_insert_with(|| {
                        let mut counts = HashMap::new();
                        counts.insert(c, 1);
                        Bucket { first: c, counts }
                    });
            }
            let mut lossy = false;
            let mut cells_to_write = Vec::with_capacity(blocks.len());
            for (block, bucket) in blocks {
                if bucket.counts.len() > 1 {
                    lossy = true;
                }
                // Majority: max count, tie-break by first-seen.
                let first_count = *bucket.counts.get(&bucket.first).unwrap_or(&0);
                let mut best = bucket.first;
                let mut best_count = first_count;
                for (&c, &n) in &bucket.counts {
                    if n > best_count {
                        best = c;
                        best_count = n;
                    }
                }
                cells_to_write.push((block, best));
            }
            let result_count = cells_to_write.len();
            ResamplePlan {
                cells_to_clear,
                cells_to_write,
                source_count,
                result_count,
                lossy,
            }
        }
    }
}

/// Apply a resample plan to the grid as a single undo stroke and fire a
/// summary toast. Per-stroke dedup in `History::record` collapses any
/// overlapping clear+rewrite into one delta.
pub fn apply_resample(
    grid: &mut VoxelGrid,
    history: &mut History,
    toasts: &mut Toasts,
    op: ResampleOp,
) {
    if grid.count() == 0 {
        return;
    }
    let plan = plan_resample(grid, op);
    history.begin();
    for p in &plan.cells_to_clear {
        history.record(grid, *p, None);
    }
    for (p, c) in &plan.cells_to_write {
        history.record(grid, *p, Some(*c));
    }
    history.end();

    let msg = match op {
        ResampleOp::Double => format!(
            "Doubled density: {} → {} voxels",
            plan.source_count, plan.result_count
        ),
        ResampleOp::Halve if plan.lossy => format!(
            "Halved density: {} → {} voxels (detail merged)",
            plan.source_count, plan.result_count
        ),
        ResampleOp::Halve => format!(
            "Halved density: {} → {} voxels",
            plan.source_count, plan.result_count
        ),
    };
    toasts.info(msg);
}

#[cfg(test)]
mod tests {
    use super::*;

    const RED: Color8 = [200, 0, 0, 255];
    const BLUE: Color8 = [0, 0, 200, 255];
    const GREEN: Color8 = [0, 200, 0, 255];

    #[test]
    fn double_single_cell_becomes_eight() {
        let mut g = VoxelGrid::default();
        g.set(IVec3::new(1, 2, 3), Some(RED));
        let plan = plan_resample(&g, ResampleOp::Double);
        assert_eq!(plan.source_count, 1);
        assert_eq!(plan.result_count, 8);
        assert!(!plan.lossy);
        let mut writes: Vec<IVec3> = plan.cells_to_write.iter().map(|(p, _)| *p).collect();
        writes.sort_by_key(|p| (p.x, p.y, p.z));
        let mut expected: Vec<IVec3> = (0..2)
            .flat_map(|dx| (0..2).flat_map(move |dy| (0..2).map(move |dz| IVec3::new(dx, dy, dz))))
            .map(|d| IVec3::new(2, 4, 6) + d)
            .collect();
        expected.sort_by_key(|p| (p.x, p.y, p.z));
        assert_eq!(writes, expected);
        for (_, c) in &plan.cells_to_write {
            assert_eq!(*c, RED);
        }
        assert_eq!(plan.cells_to_clear, vec![IVec3::new(1, 2, 3)]);
    }

    #[test]
    fn double_then_halve_round_trip_lossless() {
        let mut g = VoxelGrid::default();
        // Solid 2×2×2 block of RED at origin.
        for x in 0..2 {
            for y in 0..2 {
                for z in 0..2 {
                    g.set(IVec3::new(x, y, z), Some(RED));
                }
            }
        }
        // Double via apply path so we exercise history-driven application.
        let mut h = History::default();
        let mut toasts = Toasts::default();
        apply_resample(&mut g, &mut h, &mut toasts, ResampleOp::Double);
        assert_eq!(g.count(), 64); // 8 cells × 8 = 64

        apply_resample(&mut g, &mut h, &mut toasts, ResampleOp::Halve);
        assert_eq!(g.count(), 8);
        for x in 0..2 {
            for y in 0..2 {
                for z in 0..2 {
                    assert_eq!(g.get(IVec3::new(x, y, z)), Some(RED));
                }
            }
        }
    }

    #[test]
    fn halve_uniform_2x2x2_block_collapses_to_single_cell() {
        let mut g = VoxelGrid::default();
        for x in 0..2 {
            for y in 0..2 {
                for z in 0..2 {
                    g.set(IVec3::new(x, y, z), Some(RED));
                }
            }
        }
        let plan = plan_resample(&g, ResampleOp::Halve);
        assert_eq!(plan.source_count, 8);
        assert_eq!(plan.result_count, 1);
        assert!(!plan.lossy);
        assert_eq!(plan.cells_to_write, vec![(IVec3::ZERO, RED)]);
    }

    #[test]
    fn halve_mixed_block_picks_majority() {
        let mut g = VoxelGrid::default();
        // 5 reds + 3 blues in a single block at (0,0,0).
        let cells = [
            (IVec3::new(0, 0, 0), RED),
            (IVec3::new(1, 0, 0), RED),
            (IVec3::new(0, 1, 0), RED),
            (IVec3::new(0, 0, 1), RED),
            (IVec3::new(1, 1, 0), RED),
            (IVec3::new(1, 0, 1), BLUE),
            (IVec3::new(0, 1, 1), BLUE),
            (IVec3::new(1, 1, 1), BLUE),
        ];
        for (p, c) in cells {
            g.set(p, Some(c));
        }
        let plan = plan_resample(&g, ResampleOp::Halve);
        assert_eq!(plan.result_count, 1);
        assert!(plan.lossy);
        assert_eq!(plan.cells_to_write, vec![(IVec3::ZERO, RED)]);
    }

    #[test]
    fn halve_marks_lossy_when_any_block_mixed() {
        let mut g = VoxelGrid::default();
        // Block (0,0,0): uniform red (4 cells).
        for x in 0..2 {
            for z in 0..2 {
                g.set(IVec3::new(x, 0, z), Some(RED));
            }
        }
        // Block (1,0,0): mixed — one green, three blue.
        g.set(IVec3::new(2, 0, 0), Some(GREEN));
        g.set(IVec3::new(3, 0, 0), Some(BLUE));
        g.set(IVec3::new(2, 0, 1), Some(BLUE));
        g.set(IVec3::new(3, 0, 1), Some(BLUE));
        let plan = plan_resample(&g, ResampleOp::Halve);
        assert!(plan.lossy);
        assert_eq!(plan.result_count, 2);
    }

    #[test]
    fn halve_odd_dim_source_consumes_all_cells() {
        let mut g = VoxelGrid::default();
        // 3-wide line on x at y=0,z=0. Coord 0,1 → block 0; coord 2 → block 1.
        for x in 0..3 {
            g.set(IVec3::new(x, 0, 0), Some(RED));
        }
        let plan = plan_resample(&g, ResampleOp::Halve);
        assert_eq!(plan.source_count, 3);
        // Two output blocks, all source cells accounted for.
        assert_eq!(plan.result_count, 2);
        let mut blocks: Vec<IVec3> = plan.cells_to_write.iter().map(|(p, _)| *p).collect();
        blocks.sort_by_key(|p| (p.x, p.y, p.z));
        assert_eq!(blocks, vec![IVec3::new(0, 0, 0), IVec3::new(1, 0, 0)]);
    }

    #[test]
    fn halve_negative_coords_round_down_correctly() {
        let mut g = VoxelGrid::default();
        // Negative x with y >= 0 is legal in the open-world grid.
        g.set(IVec3::new(-1, 0, 0), Some(RED));
        let plan = plan_resample(&g, ResampleOp::Halve);
        // div_euclid: -1 / 2 = -1, not 0.
        assert_eq!(plan.cells_to_write, vec![(IVec3::new(-1, 0, 0), RED)]);
    }

    #[test]
    fn double_preserves_y_zero_floor() {
        let mut g = VoxelGrid::default();
        g.set(IVec3::new(0, 0, 0), Some(RED));
        let plan = plan_resample(&g, ResampleOp::Double);
        // All writes have y in {0, 1}; none negative.
        for (p, _) in &plan.cells_to_write {
            assert!(p.y >= 0, "double produced negative y: {p:?}");
        }
        let ys: std::collections::HashSet<i32> =
            plan.cells_to_write.iter().map(|(p, _)| p.y).collect();
        assert!(ys.contains(&0));
        assert!(ys.contains(&1));
    }

    #[test]
    fn empty_grid_produces_empty_plan() {
        let g = VoxelGrid::default();
        let p1 = plan_resample(&g, ResampleOp::Double);
        assert_eq!(p1.source_count, 0);
        assert_eq!(p1.result_count, 0);
        assert!(p1.cells_to_clear.is_empty());
        assert!(p1.cells_to_write.is_empty());
        let p2 = plan_resample(&g, ResampleOp::Halve);
        assert_eq!(p2.source_count, 0);
        assert_eq!(p2.result_count, 0);
        assert!(!p2.lossy);
    }

    #[test]
    fn apply_resample_creates_single_undo_stroke() {
        let mut g = VoxelGrid::default();
        for x in 0..2 {
            for y in 0..2 {
                for z in 0..2 {
                    g.set(IVec3::new(x, y, z), Some(RED));
                }
            }
        }
        let mut h = History::default();
        let mut toasts = Toasts::default();
        apply_resample(&mut g, &mut h, &mut toasts, ResampleOp::Double);
        assert_eq!(h.undo.len(), 1, "single stroke for resample");
        // Undo restores the original 8-cell block.
        h.undo(&mut g);
        assert_eq!(g.count(), 8);
        for x in 0..2 {
            for y in 0..2 {
                for z in 0..2 {
                    assert_eq!(g.get(IVec3::new(x, y, z)), Some(RED));
                }
            }
        }
    }

    #[test]
    fn apply_resample_empty_grid_does_nothing() {
        let mut g = VoxelGrid::default();
        let mut h = History::default();
        let mut toasts = Toasts::default();
        apply_resample(&mut g, &mut h, &mut toasts, ResampleOp::Double);
        assert_eq!(g.count(), 0);
        assert!(h.undo.is_empty());
    }
}
