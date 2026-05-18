use crate::grid::{Color8, VoxelGrid};
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_panorbit_camera::PanOrbitCamera;

/// Maximum number of DDA steps before declaring a miss. In the bounded grid
/// this used to be `size * 3 + 16`; in the open world we cap at a fixed
/// budget so an open-air ray terminates even with no occupied voxels in
/// sight. 1024 voxels is far past any reasonable orbit radius.
pub const MAX_DDA_STEPS: usize = 1024;

#[derive(Clone, Copy, Debug)]
pub struct Hit {
    pub cell: IVec3,
    pub normal: IVec3,
    pub hit_voxel: bool,
}

pub fn cursor_ray(
    cameras: &Query<(&Camera, &GlobalTransform), With<PanOrbitCamera>>,
    windows: &Query<&Window, With<PrimaryWindow>>,
) -> Option<(Vec3, Vec3)> {
    let window = windows.single().ok()?;
    let cursor = window.cursor_position()?;
    let (camera, tf) = cameras.single().ok()?;
    let ray = camera.viewport_to_world(tf, cursor).ok()?;
    Some((ray.origin, *ray.direction))
}

pub fn pick(grid: &VoxelGrid, origin: Vec3, dir: Vec3) -> Option<Hit> {
    pick_with(|p| grid.get(p), origin, dir)
}

/// DDA voxel raycaster parameterised by an arbitrary cell-reader. Used by
/// the stroke path to overlay pre-stroke values from `StrokeShadow` over
/// the live grid, so voxels placed earlier in the same stroke are invisible
/// to the picker.
///
/// Terminates on: hit, `MAX_DDA_STEPS` exhausted, or ray exits below the
/// floor going further down (no point continuing — there's nothing there).
pub fn pick_with<F>(read: F, origin: Vec3, dir: Vec3) -> Option<Hit>
where
    F: Fn(IVec3) -> Option<Color8>,
{
    let step = IVec3::new(
        if dir.x > 0.0 {
            1
        } else if dir.x < 0.0 {
            -1
        } else {
            0
        },
        if dir.y > 0.0 {
            1
        } else if dir.y < 0.0 {
            -1
        } else {
            0
        },
        if dir.z > 0.0 {
            1
        } else if dir.z < 0.0 {
            -1
        } else {
            0
        },
    );

    let mut cell = IVec3::new(
        origin.x.floor() as i32,
        origin.y.floor() as i32,
        origin.z.floor() as i32,
    );

    let next_boundary = |c: i32, s: i32| -> f32 { if s > 0 { (c + 1) as f32 } else { c as f32 } };

    let inf = f32::INFINITY;
    let mut t_max = Vec3::new(
        if dir.x != 0.0 {
            (next_boundary(cell.x, step.x) - origin.x) / dir.x
        } else {
            inf
        },
        if dir.y != 0.0 {
            (next_boundary(cell.y, step.y) - origin.y) / dir.y
        } else {
            inf
        },
        if dir.z != 0.0 {
            (next_boundary(cell.z, step.z) - origin.z) / dir.z
        } else {
            inf
        },
    );
    let t_delta = Vec3::new(
        if dir.x != 0.0 {
            (1.0 / dir.x).abs()
        } else {
            inf
        },
        if dir.y != 0.0 {
            (1.0 / dir.y).abs()
        } else {
            inf
        },
        if dir.z != 0.0 {
            (1.0 / dir.z).abs()
        } else {
            inf
        },
    );

    let mut normal = IVec3::ZERO;

    for _ in 0..MAX_DDA_STEPS {
        if cell.y >= 0 && read(cell).is_some() {
            return Some(Hit {
                cell,
                normal,
                hit_voxel: true,
            });
        }
        if t_max.x < t_max.y && t_max.x < t_max.z {
            cell.x += step.x;
            t_max.x += t_delta.x;
            normal = IVec3::new(-step.x, 0, 0);
        } else if t_max.y < t_max.z {
            cell.y += step.y;
            t_max.y += t_delta.y;
            normal = IVec3::new(0, -step.y, 0);
        } else {
            cell.z += step.z;
            t_max.z += t_delta.z;
            normal = IVec3::new(0, 0, -step.z);
        }

        // Ray has exited below the floor and is still heading down — there
        // is nothing below y=0 to ever hit.
        if cell.y < 0 && step.y <= 0 {
            break;
        }
    }

    // No voxel hit — try the floor plane y=0 for first-voxel placement.
    if dir.y < 0.0 {
        let t = -origin.y / dir.y;
        if t > 0.0 {
            let p = origin + dir * t;
            let floor_cell = IVec3::new(p.x.floor() as i32, 0, p.z.floor() as i32);
            return Some(Hit {
                cell: floor_cell + IVec3::new(0, -1, 0),
                normal: IVec3::Y,
                hit_voxel: false,
            });
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ray_into_single_voxel_from_negative_x() {
        let mut g = VoxelGrid::default();
        g.set(IVec3::new(5, 0, 0), Some([1, 1, 1, 255]));
        let hit = pick(&g, Vec3::new(-1.0, 0.5, 0.5), Vec3::X).expect("hit");
        assert_eq!(hit.cell, IVec3::new(5, 0, 0));
        assert_eq!(hit.normal, IVec3::new(-1, 0, 0));
        assert!(hit.hit_voxel);
    }

    #[test]
    fn ray_into_single_voxel_from_positive_y() {
        let mut g = VoxelGrid::default();
        g.set(IVec3::new(2, 3, 2), Some([1, 1, 1, 255]));
        let hit = pick(&g, Vec3::new(2.5, 10.0, 2.5), Vec3::NEG_Y).expect("hit");
        assert_eq!(hit.cell, IVec3::new(2, 3, 2));
        assert_eq!(hit.normal, IVec3::Y);
    }

    #[test]
    fn ray_misses_voxel_falls_to_floor() {
        let g = VoxelGrid::default();
        let hit = pick(&g, Vec3::new(5.5, 10.0, 5.5), Vec3::new(0.0, -1.0, 0.0)).expect("floor");
        assert!(!hit.hit_voxel);
        assert_eq!(hit.normal, IVec3::Y);
        assert_eq!(hit.cell, IVec3::new(5, -1, 5));
    }

    #[test]
    fn floor_fallback_works_at_negative_world_coords() {
        // Open-world floor: clicking at negative-XZ on empty space should still
        // produce a floor cell. The bounded version of this test required the
        // cursor inside [0, size); we drop that.
        let g = VoxelGrid::default();
        let hit = pick(
            &g,
            Vec3::new(-200.0, 10.0, -300.0),
            Vec3::new(0.0, -1.0, 0.0),
        )
        .expect("floor");
        assert!(!hit.hit_voxel);
        assert_eq!(hit.cell, IVec3::new(-200, -1, -300));
    }

    #[test]
    fn dda_terminates_at_step_cap_in_empty_world() {
        // Horizontal ray in an empty world — no voxels, no floor under the
        // ray. Without a step cap this would loop until i32 overflow.
        let g = VoxelGrid::default();
        assert!(pick(&g, Vec3::new(0.5, 5.0, 0.5), Vec3::X).is_none());
    }

    #[test]
    fn ray_aimed_straight_down_below_floor_misses() {
        // Origin already below the floor, aimed further down. No voxels and
        // no floor fallback (dir.y < 0 but t = -origin.y/dir.y is negative
        // because origin.y < 0).
        let g = VoxelGrid::default();
        assert!(pick(&g, Vec3::new(0.5, -5.0, 0.5), Vec3::NEG_Y).is_none());
    }

    #[test]
    fn pick_with_overlay_hides_pre_stroke_cell() {
        let mut g = VoxelGrid::default();
        g.set(IVec3::new(5, 0, 0), Some([1, 1, 1, 255]));
        let read = |p: IVec3| -> Option<Color8> {
            if p == IVec3::new(5, 0, 0) {
                None
            } else {
                g.get(p)
            }
        };
        let hit = pick_with(read, Vec3::new(-1.0, 0.5, 0.5), Vec3::X);
        // Ray is purely +X so no floor fallback; no voxel hit either.
        assert!(hit.is_none());
    }

    #[test]
    fn pick_with_overlay_reveals_pre_stroke_cell() {
        let g = VoxelGrid::default();
        let read = |p: IVec3| -> Option<Color8> {
            if p == IVec3::new(5, 0, 0) {
                Some([1, 1, 1, 255])
            } else {
                g.get(p)
            }
        };
        let hit = pick_with(read, Vec3::new(-1.0, 0.5, 0.5), Vec3::X).expect("hit");
        assert_eq!(hit.cell, IVec3::new(5, 0, 0));
    }

    #[test]
    fn pick_walks_past_empty_to_filled_voxel() {
        let mut g = VoxelGrid::default();
        g.set(IVec3::new(10, 0, 0), Some([1, 1, 1, 255]));
        let hit = pick(&g, Vec3::new(-1.0, 0.5, 0.5), Vec3::X).expect("hit");
        assert_eq!(hit.cell, IVec3::new(10, 0, 0));
    }
}
