//! Camera-facing voxel faces, projected to screen space and shaded. Shared by
//! the SVG exporter and the CLI's PNG renderer.

use crate::grid::VoxelGrid;
use crate::mesh_util::{FACES, face_shade, linear_to_srgb, srgb_to_linear};
use glam::{Mat4, Vec2, Vec3, Vec4};

const NEAR_W_EPSILON: f32 = 1e-3;

pub struct CellQuad {
    pub corners: [Vec3; 4],
    pub color: [u8; 4],
    pub face_idx: usize,
    pub cell_center: Vec3,
}

pub struct ProjectedQuad {
    pub pts: [Vec2; 4],
    /// Cell-center view-space z (negative in front of the camera). Smaller
    /// (more negative) = further along the camera forward direction.
    pub cell_view_z: f32,
    /// View-space z of each corner in `pts`.
    pub depths: [f32; 4],
    pub color: [u8; 4],
    pub face_idx: usize,
}

/// One unit quad per exposed voxel face that points toward `camera_pos`.
pub fn collect_cell_quads(grid: &VoxelGrid, camera_pos: Vec3) -> Vec<CellQuad> {
    let mut out = Vec::new();
    for (p, rgba) in grid.iter_occupied() {
        let cell_center = Vec3::new(p.x as f32 + 0.5, p.y as f32 + 0.5, p.z as f32 + 0.5);
        for (face_idx, face) in FACES.iter().enumerate() {
            let neighbor = p + face.d;
            if grid.get(neighbor).is_some() {
                continue;
            }
            let normal = Vec3::new(face.normal[0], face.normal[1], face.normal[2]);
            let face_center = cell_center + 0.5 * normal;
            if (face_center - camera_pos).dot(normal) > 0.0 {
                continue;
            }

            let mut corners = [Vec3::ZERO; 4];
            for (idx, c) in face.corners.iter().enumerate() {
                corners[idx] = Vec3::new(
                    (p.x + c[0]) as f32,
                    (p.y + c[1]) as f32,
                    (p.z + c[2]) as f32,
                );
            }
            out.push(CellQuad {
                corners,
                color: rgba,
                face_idx,
                cell_center,
            });
        }
    }
    out
}

/// Projects quads to pixel coordinates (y down) in a `viewport`-sized image.
/// Quads with a corner behind the near plane are dropped.
pub fn project_quads(
    quads: &[CellQuad],
    view: Mat4,
    view_proj: Mat4,
    viewport: Vec2,
) -> Vec<ProjectedQuad> {
    let half = viewport * 0.5;
    let mut out = Vec::with_capacity(quads.len());
    for q in quads {
        let mut pts = [Vec2::ZERO; 4];
        let mut depths = [0.0; 4];
        let mut behind = false;
        for (i, c) in q.corners.iter().enumerate() {
            let clip_pos = view_proj * Vec4::new(c.x, c.y, c.z, 1.0);
            if clip_pos.w <= NEAR_W_EPSILON {
                behind = true;
                break;
            }
            let ndc_x = clip_pos.x / clip_pos.w;
            let ndc_y = clip_pos.y / clip_pos.w;
            pts[i] = Vec2::new(half.x + ndc_x * half.x, half.y - ndc_y * half.y);
            depths[i] = (view * Vec4::new(c.x, c.y, c.z, 1.0)).z;
        }
        if behind {
            continue;
        }
        let view_pos = view * Vec4::new(q.cell_center.x, q.cell_center.y, q.cell_center.z, 1.0);
        out.push(ProjectedQuad {
            pts,
            cell_view_z: view_pos.z,
            depths,
            color: q.color,
            face_idx: q.face_idx,
        });
    }
    out
}

/// The face's flat shade applied to an sRGB color.
pub fn shade_color(rgba: [u8; 4], normal: [f32; 3]) -> [u8; 3] {
    let shade = face_shade(normal);
    let mut out = [0u8; 3];
    for i in 0..3 {
        let s = rgba[i] as f32 / 255.0 * shade;
        let lin = srgb_to_linear(s);
        let s2 = linear_to_srgb(lin.clamp(0.0, 1.0));
        out[i] = (s2.clamp(0.0, 1.0) * 255.0).round() as u8;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shade_color_darkens_with_face_normal() {
        let rgb = [200u8, 100u8, 50u8, 255u8];
        // Top face (y+) is brightest; bottom (y-) darker per face_shade().
        let top = shade_color(rgb, [0.0, 1.0, 0.0]);
        let bottom = shade_color(rgb, [0.0, -1.0, 0.0]);
        assert!(
            top[0] >= bottom[0] && top[1] >= bottom[1] && top[2] >= bottom[2],
            "top {top:?} should be >= bottom {bottom:?}",
        );
    }
}
