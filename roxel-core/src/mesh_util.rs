use glam::IVec3;

pub struct Face {
    pub normal: [f32; 3],
    pub d: IVec3,
    pub corners: [[i32; 3]; 4],
    pub axis: usize,
    pub u_axis: usize,
    pub v_axis: usize,
    pub plane_offset: i32,
}

pub const FACES: [Face; 6] = [
    Face {
        normal: [1.0, 0.0, 0.0],
        d: IVec3::new(1, 0, 0),
        corners: [[1, 0, 0], [1, 0, 1], [1, 1, 1], [1, 1, 0]],
        axis: 0,
        u_axis: 1,
        v_axis: 2,
        plane_offset: 1,
    },
    Face {
        normal: [-1.0, 0.0, 0.0],
        d: IVec3::new(-1, 0, 0),
        corners: [[0, 0, 0], [0, 1, 0], [0, 1, 1], [0, 0, 1]],
        axis: 0,
        u_axis: 1,
        v_axis: 2,
        plane_offset: 0,
    },
    Face {
        normal: [0.0, 1.0, 0.0],
        d: IVec3::new(0, 1, 0),
        corners: [[0, 1, 0], [1, 1, 0], [1, 1, 1], [0, 1, 1]],
        axis: 1,
        u_axis: 0,
        v_axis: 2,
        plane_offset: 1,
    },
    Face {
        normal: [0.0, -1.0, 0.0],
        d: IVec3::new(0, -1, 0),
        corners: [[0, 0, 0], [0, 0, 1], [1, 0, 1], [1, 0, 0]],
        axis: 1,
        u_axis: 0,
        v_axis: 2,
        plane_offset: 0,
    },
    Face {
        normal: [0.0, 0.0, 1.0],
        d: IVec3::new(0, 0, 1),
        corners: [[0, 0, 1], [0, 1, 1], [1, 1, 1], [1, 0, 1]],
        axis: 2,
        u_axis: 0,
        v_axis: 1,
        plane_offset: 1,
    },
    Face {
        normal: [0.0, 0.0, -1.0],
        d: IVec3::new(0, 0, -1),
        corners: [[0, 0, 0], [1, 0, 0], [1, 1, 0], [0, 1, 0]],
        axis: 2,
        u_axis: 0,
        v_axis: 1,
        plane_offset: 0,
    },
];

pub fn for_each_exposed_face(
    grid: &crate::grid::VoxelGrid,
    mut f: impl FnMut(IVec3, &Face, crate::grid::Color8),
) {
    for (cell, rgba) in grid.iter_occupied() {
        for face in &FACES {
            if grid.get(cell + face.d).is_none() {
                f(cell, face, rgba);
            }
        }
    }
}

pub fn face_shade(normal: [f32; 3]) -> f32 {
    if normal[1] > 0.5 {
        1.0
    } else if normal[1] < -0.5 {
        0.45
    } else if normal[0].abs() > 0.5 {
        0.78
    } else {
        0.62
    }
}

pub fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

pub fn linear_to_srgb(c: f32) -> f32 {
    if c <= 0.003_130_8 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}
