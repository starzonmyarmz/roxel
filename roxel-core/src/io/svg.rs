// SVG export of the current 3D view.
//
// One <polygon> per visible voxel face. Sorting axis-aligned voxel quads by
// their cell's distance from the camera (descending — furthest cells drawn
// first) is a provably correct painter's order: any two visible exterior
// faces of axis-aligned unit voxels are either coplanar-disjoint or one is
// strictly closer to the camera. Greedy-merged quads break that invariant
// at concave junctions, so this path stays per-cell.
//
// Consecutive same-color quads are emitted into one <path> element with
// multiple `M ... Z` subpaths so the tag overhead doesn't dominate the file
// size. The viewBox is trimmed to the projected voxel bounds.

use crate::grid::VoxelGrid;
use crate::mesh_util::{FACES, face_shade, linear_to_srgb, srgb_to_linear};
use anyhow::Result;
use glam::{Mat4, Vec2, Vec3, Vec4};
use std::cmp::Ordering;
use std::fmt::Write as FmtWrite;
use std::io::Write;
use std::path::Path;

const NEAR_W_EPSILON: f32 = 1e-3;

struct CellQuad {
    corners: [Vec3; 4],
    color: [u8; 4],
    face_idx: usize,
    cell_center: Vec3,
}

struct ProjectedQuad {
    pts: [Vec2; 4],
    /// Cell-center view-space z (negative in front of the camera). Smaller
    /// (more negative) = further along the camera forward direction.
    cell_view_z: f32,
    color: [u8; 4],
    face_idx: usize,
}

pub fn export(
    path: &Path,
    grid: &VoxelGrid,
    view: Mat4,
    view_proj: Mat4,
    camera_pos: Vec3,
    viewport_size: Vec2,
) -> Result<()> {
    let quads = collect_cell_quads(grid, camera_pos);
    let mut projected = project_quads(&quads, view, view_proj, viewport_size);
    // Most-negative cell view-z first — furthest along the camera forward
    // direction is drawn first; closer cells overdraw. Using view-space z
    // (not Euclidean distance) accounts for perspective correctly: an
    // off-axis cell at the same length from the camera as an on-axis cell
    // is still less deep along the forward direction.
    projected.sort_by(|a, b| {
        a.cell_view_z
            .partial_cmp(&b.cell_view_z)
            .unwrap_or(Ordering::Equal)
            .then(a.face_idx.cmp(&b.face_idx))
    });

    write_svg(path, &projected)
}

fn collect_cell_quads(grid: &VoxelGrid, camera_pos: Vec3) -> Vec<CellQuad> {
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

fn project_quads(
    quads: &[CellQuad],
    view: Mat4,
    view_proj: Mat4,
    viewport: Vec2,
) -> Vec<ProjectedQuad> {
    let half = viewport * 0.5;
    let mut out = Vec::with_capacity(quads.len());
    for q in quads {
        let mut pts = [Vec2::ZERO; 4];
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
        }
        if behind {
            continue;
        }
        let view_pos = view * Vec4::new(q.cell_center.x, q.cell_center.y, q.cell_center.z, 1.0);
        out.push(ProjectedQuad {
            pts,
            cell_view_z: view_pos.z,
            color: q.color,
            face_idx: q.face_idx,
        });
    }
    out
}

fn shade_color(rgba: [u8; 4], normal: [f32; 3]) -> [u8; 3] {
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

fn write_svg(path: &Path, quads: &[ProjectedQuad]) -> Result<()> {
    if quads.is_empty() {
        anyhow::bail!("Nothing to export: no visible voxel faces");
    }

    let mut min = Vec2::splat(f32::INFINITY);
    let mut max = Vec2::splat(f32::NEG_INFINITY);
    for q in quads {
        for p in &q.pts {
            min = min.min(*p);
            max = max.max(*p);
        }
    }
    let pad = ((max - min).max_element() * 0.005).max(0.5);
    min -= Vec2::splat(pad);
    max += Vec2::splat(pad);
    let size = max - min;

    // Build the body. Group consecutive same-color quads into one <path>
    // element with multiple `M ... Z` subpaths so we don't pay the tag and
    // fill="…" overhead per cell.
    let mut body = String::new();
    let mut current_color: Option<[u8; 3]> = None;
    let mut in_path = false;

    let close_path = |body: &mut String, in_path: &mut bool| {
        if *in_path {
            body.push_str("\"/>");
            *in_path = false;
        }
    };

    for q in quads {
        let normal = FACES[q.face_idx].normal;
        let rgb = shade_color(q.color, normal);

        if current_color != Some(rgb) {
            close_path(&mut body, &mut in_path);
            let _ = write!(
                &mut body,
                "<path fill=\"#{:02X}{:02X}{:02X}\" d=\"",
                rgb[0], rgb[1], rgb[2],
            );
            in_path = true;
            current_color = Some(rgb);
        }

        let _ = write!(
            &mut body,
            "M{:.3},{:.3}L{:.3},{:.3}L{:.3},{:.3}L{:.3},{:.3}Z",
            q.pts[0].x,
            q.pts[0].y,
            q.pts[1].x,
            q.pts[1].y,
            q.pts[2].x,
            q.pts[2].y,
            q.pts[3].x,
            q.pts[3].y,
        );
    }
    close_path(&mut body, &mut in_path);

    let mut file = std::io::BufWriter::new(std::fs::File::create(path)?);
    writeln!(file, r#"<?xml version="1.0" encoding="UTF-8"?>"#)?;
    writeln!(
        file,
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="{:.3} {:.3} {:.3} {:.3}" width="{:.0}" height="{:.0}" shape-rendering="geometricPrecision">"#,
        min.x, min.y, size.x, size.y, size.x, size.y,
    )?;
    file.write_all(body.as_bytes())?;
    writeln!(file, "</svg>")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io::test_util::tmp_path as raw_tmp_path;
    use glam::IVec3;
    use std::path::PathBuf;

    fn tmp_path(name: &str) -> PathBuf {
        raw_tmp_path(name, "svg")
    }

    fn test_camera() -> (Mat4, Mat4, Vec3) {
        let eye = Vec3::new(50.0, 50.0, 50.0);
        let center = Vec3::new(16.0, 16.0, 16.0);
        let view = Mat4::look_at_rh(eye, center, Vec3::Y);
        let proj = Mat4::perspective_rh_gl(std::f32::consts::FRAC_PI_4, 1.0, 0.1, 1000.0);
        (view, proj * view, eye)
    }

    #[test]
    fn empty_grid_returns_error() {
        let g = crate::grid::VoxelGrid::default();
        let (view, view_proj, cam_pos) = test_camera();
        let path = tmp_path("empty");
        let err = export(&path, &g, view, view_proj, cam_pos, Vec2::new(800.0, 600.0))
            .expect_err("expected error");
        assert!(err.to_string().contains("Nothing to export"));
    }

    #[test]
    fn single_voxel_writes_valid_svg() {
        let mut g = crate::grid::VoxelGrid::default();
        g.set(IVec3::new(1, 1, 1), Some([255, 0, 0, 255]));
        let (view, view_proj, cam_pos) = test_camera();
        let path = tmp_path("single");
        export(&path, &g, view, view_proj, cam_pos, Vec2::new(800.0, 600.0)).expect("export");
        let s = std::fs::read_to_string(&path).expect("read");
        assert!(s.starts_with("<?xml"));
        assert!(s.contains("<svg "));
        assert!(s.ends_with("</svg>\n"));
        // 3 of 6 faces visible toward (+x, +y, +z) corner from positive-octant camera.
        let path_count = s.matches("<path ").count();
        assert!(path_count >= 1, "expected at least one path element");
        // Each visible face emits one `M ... Z` subpath.
        assert_eq!(s.matches("Z").count(), 3);
        let _ = std::fs::remove_file(&path);
    }

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
