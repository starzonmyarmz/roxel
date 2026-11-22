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
use crate::mesh_util::FACES;
use crate::projection::{ProjectedQuad, collect_cell_quads, project_quads, shade_color};
use anyhow::Result;
use glam::{Mat4, Vec2, Vec3};
use std::cmp::Ordering;
use std::fmt::Write as FmtWrite;
use std::io::Write;
use std::path::Path;

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

    super::atomic::write_with(path, |file| -> Result<()> {
        writeln!(file, r#"<?xml version="1.0" encoding="UTF-8"?>"#)?;
        writeln!(
            file,
            r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="{:.3} {:.3} {:.3} {:.3}" width="{:.0}" height="{:.0}" shape-rendering="geometricPrecision">"#,
            min.x, min.y, size.x, size.y, size.x, size.y,
        )?;
        file.write_all(body.as_bytes())?;
        writeln!(file, "</svg>")?;
        Ok(())
    })
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
        let view = glam::camera::rh::view::look_at_mat4(eye, center, Vec3::Y);
        let proj = glam::camera::rh::proj::opengl::perspective(
            std::f32::consts::FRAC_PI_4,
            1.0,
            0.1,
            1000.0,
        );
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

    fn fills_in_paint_order(svg: &str) -> Vec<[u8; 3]> {
        svg.split("<path fill=\"#")
            .skip(1)
            .map(|rest| {
                let hex = &rest[..6];
                let ch = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).unwrap();
                [ch(0), ch(2), ch(4)]
            })
            .collect()
    }

    #[test]
    fn painter_order_draws_far_cells_before_near_ones() {
        let mut g = crate::grid::VoxelGrid::default();
        // On the line from the origin toward the camera at (50, 50, 50).
        g.set(IVec3::new(0, 0, 0), Some([255, 0, 0, 255]));
        g.set(IVec3::new(6, 6, 6), Some([0, 255, 0, 255]));
        g.set(IVec3::new(12, 12, 12), Some([0, 0, 255, 255]));
        let (view, view_proj, cam_pos) = test_camera();
        let path = tmp_path("painter");
        export(&path, &g, view, view_proj, cam_pos, Vec2::new(800.0, 600.0)).expect("export");
        let s = std::fs::read_to_string(&path).expect("read");
        let _ = std::fs::remove_file(&path);

        let rank = |rgb: &[u8; 3]| match rgb {
            [_, 0, 0] => 0,
            [0, _, 0] => 1,
            [0, 0, _] => 2,
            other => panic!("unexpected fill {other:?}"),
        };
        let order: Vec<u8> = fills_in_paint_order(&s).iter().map(rank).collect();
        assert!(!order.is_empty());
        assert!(
            order.windows(2).all(|w| w[0] <= w[1]),
            "far-to-near order broken: {order:?}",
        );
        assert_eq!(order.first(), Some(&0));
        assert_eq!(order.last(), Some(&2));
    }

    #[test]
    fn painter_order_puts_an_occluder_after_the_cell_it_hides() {
        let mut g = crate::grid::VoxelGrid::default();
        // A cell directly in front (toward the camera) of another, touching
        // only at an edge, so both keep their camera-facing faces.
        g.set(IVec3::new(4, 4, 4), Some([255, 0, 0, 255]));
        g.set(IVec3::new(5, 5, 4), Some([0, 0, 255, 255]));
        let (view, view_proj, cam_pos) = test_camera();
        let path = tmp_path("occluder");
        export(&path, &g, view, view_proj, cam_pos, Vec2::new(800.0, 600.0)).expect("export");
        let s = std::fs::read_to_string(&path).expect("read");
        let _ = std::fs::remove_file(&path);

        let fills = fills_in_paint_order(&s);
        let last_red = fills.iter().rposition(|c| c[1] == 0 && c[2] == 0).unwrap();
        let first_blue = fills.iter().position(|c| c[0] == 0 && c[1] == 0).unwrap();
        assert!(last_red < first_blue, "fills: {fills:?}");
    }
}
