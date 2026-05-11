use crate::grid::{GRID_I, VoxelGrid};
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_panorbit_camera::PanOrbitCamera;

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
    let step = IVec3::new(
        if dir.x > 0.0 { 1 } else if dir.x < 0.0 { -1 } else { 0 },
        if dir.y > 0.0 { 1 } else if dir.y < 0.0 { -1 } else { 0 },
        if dir.z > 0.0 { 1 } else if dir.z < 0.0 { -1 } else { 0 },
    );

    let mut cell = IVec3::new(
        origin.x.floor() as i32,
        origin.y.floor() as i32,
        origin.z.floor() as i32,
    );

    let next_boundary = |c: i32, s: i32| -> f32 {
        if s > 0 { (c + 1) as f32 } else { c as f32 }
    };

    let inf = f32::INFINITY;
    let mut t_max = Vec3::new(
        if dir.x != 0.0 { (next_boundary(cell.x, step.x) - origin.x) / dir.x } else { inf },
        if dir.y != 0.0 { (next_boundary(cell.y, step.y) - origin.y) / dir.y } else { inf },
        if dir.z != 0.0 { (next_boundary(cell.z, step.z) - origin.z) / dir.z } else { inf },
    );
    let t_delta = Vec3::new(
        if dir.x != 0.0 { (1.0 / dir.x).abs() } else { inf },
        if dir.y != 0.0 { (1.0 / dir.y).abs() } else { inf },
        if dir.z != 0.0 { (1.0 / dir.z).abs() } else { inf },
    );

    let mut normal = IVec3::ZERO;
    let max_steps = GRID_I as usize * 3 + 16;

    for _ in 0..max_steps {
        if VoxelGrid::in_bounds(cell) && grid.get(cell).is_some() {
            return Some(Hit { cell, normal, hit_voxel: true });
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

        // Early-out: walked off the grid and moving further away.
        if (cell.x < 0 && step.x <= 0)
            || (cell.x >= GRID_I && step.x >= 0)
            || (cell.y < 0 && step.y <= 0)
            || (cell.y >= GRID_I && step.y >= 0)
            || (cell.z < 0 && step.z <= 0)
            || (cell.z >= GRID_I && step.z >= 0)
        {
            break;
        }
    }

    // No voxel hit — try the floor plane y=0 for first-voxel placement.
    if dir.y < 0.0 {
        let t = -origin.y / dir.y;
        if t > 0.0 {
            let p = origin + dir * t;
            let floor_cell = IVec3::new(p.x.floor() as i32, 0, p.z.floor() as i32);
            if VoxelGrid::in_bounds(floor_cell) {
                return Some(Hit {
                    cell: floor_cell + IVec3::new(0, -1, 0),
                    normal: IVec3::Y,
                    hit_voxel: false,
                });
            }
        }
    }
    None
}
