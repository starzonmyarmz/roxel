use bevy::math::IVec3;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub enum ShapePrimitive {
    #[default]
    Rectangle,
    Ellipse,
    Line,
}

pub(crate) fn other_axes(axis: usize) -> (usize, usize) {
    match axis {
        0 => (1, 2),
        1 => (0, 2),
        _ => (0, 1),
    }
}

fn cell_from(axis: usize, u_axis: usize, v_axis: usize, w: i32, u: i32, v: i32) -> IVec3 {
    let mut a = [0i32; 3];
    a[axis] = w;
    a[u_axis] = u;
    a[v_axis] = v;
    IVec3::new(a[0], a[1], a[2])
}

pub fn rect_cells(c1: IVec3, c2: IVec3, axis: usize, filled: bool) -> Vec<IVec3> {
    let (u_axis, v_axis) = other_axes(axis);
    let a1 = c1.to_array();
    let a2 = c2.to_array();
    let umin = a1[u_axis].min(a2[u_axis]);
    let umax = a1[u_axis].max(a2[u_axis]);
    let vmin = a1[v_axis].min(a2[v_axis]);
    let vmax = a1[v_axis].max(a2[v_axis]);
    let w = a1[axis];
    let mut out = Vec::new();
    for u in umin..=umax {
        for v in vmin..=vmax {
            let on_edge = u == umin || u == umax || v == vmin || v == vmax;
            if filled || on_edge {
                out.push(cell_from(axis, u_axis, v_axis, w, u, v));
            }
        }
    }
    out
}

pub fn ellipse_cells(c1: IVec3, c2: IVec3, axis: usize, filled: bool) -> Vec<IVec3> {
    let (u_axis, v_axis) = other_axes(axis);
    let a1 = c1.to_array();
    let a2 = c2.to_array();
    let umin = a1[u_axis].min(a2[u_axis]);
    let umax = a1[u_axis].max(a2[u_axis]);
    let vmin = a1[v_axis].min(a2[v_axis]);
    let vmax = a1[v_axis].max(a2[v_axis]);
    let w = a1[axis];

    let cu = (umin as f32 + umax as f32 + 1.0) * 0.5;
    let cv = (vmin as f32 + vmax as f32 + 1.0) * 0.5;
    let ru = ((umax - umin) as f32 * 0.5 + 0.5).max(0.5);
    let rv = ((vmax - vmin) as f32 * 0.5 + 0.5).max(0.5);

    let inside = |u: i32, v: i32| -> bool {
        let du = (u as f32 + 0.5) - cu;
        let dv = (v as f32 + 0.5) - cv;
        (du / ru).powi(2) + (dv / rv).powi(2) <= 1.0
    };

    let mut out = Vec::new();
    for u in umin..=umax {
        for v in vmin..=vmax {
            if !inside(u, v) {
                continue;
            }
            if filled {
                out.push(cell_from(axis, u_axis, v_axis, w, u, v));
            } else {
                let edge = !inside(u - 1, v)
                    || !inside(u + 1, v)
                    || !inside(u, v - 1)
                    || !inside(u, v + 1);
                if edge {
                    out.push(cell_from(axis, u_axis, v_axis, w, u, v));
                }
            }
        }
    }
    out
}

pub fn line2d_cells(c1: IVec3, c2: IVec3, axis: usize) -> Vec<IVec3> {
    let (u_axis, v_axis) = other_axes(axis);
    let a1 = c1.to_array();
    let a2 = c2.to_array();
    let mut u0 = a1[u_axis];
    let mut v0 = a1[v_axis];
    let u1 = a2[u_axis];
    let v1 = a2[v_axis];
    let w = a1[axis];
    let du = (u1 - u0).abs();
    let dv = -(v1 - v0).abs();
    let su = if u0 < u1 { 1 } else { -1 };
    let sv = if v0 < v1 { 1 } else { -1 };
    let mut err = du + dv;
    let mut out = Vec::new();
    loop {
        out.push(cell_from(axis, u_axis, v_axis, w, u0, v0));
        if u0 == u1 && v0 == v1 {
            break;
        }
        let e2 = 2 * err;
        if e2 >= dv {
            err += dv;
            u0 += su;
        }
        if e2 <= du {
            err += du;
            v0 += sv;
        }
    }
    out
}

pub fn extrude(cells: &[IVec3], axis: usize, thickness: i32, sign: i32) -> Vec<IVec3> {
    let t = thickness.max(1);
    let s = if sign == 0 { 1 } else { sign };
    let mut out = Vec::with_capacity(cells.len() * t as usize);
    for k in 0..t {
        let off = k * s;
        for c in cells {
            let mut a = c.to_array();
            a[axis] += off;
            out.push(IVec3::new(a[0], a[1], a[2]));
        }
    }
    out
}

/// Compute the full set of cells that a shape occupies given the current
/// drawing state. Returns all voxel positions the shape would write on commit.
pub fn compute_shape_cells(
    primitive: ShapePrimitive,
    c1: IVec3,
    c2: IVec3,
    axis: usize,
    thickness: i32,
    normal_sign: i32,
) -> Vec<IVec3> {
    let base = match primitive {
        ShapePrimitive::Rectangle => rect_cells(c1, c2, axis, true),
        ShapePrimitive::Ellipse => ellipse_cells(c1, c2, axis, true),
        ShapePrimitive::Line => line2d_cells(c1, c2, axis),
    };
    let base_sign = if normal_sign == 0 { 1 } else { normal_sign };
    let count = thickness.unsigned_abs() as i32 + 1;
    let dir_sign = if thickness >= 0 {
        base_sign
    } else {
        -base_sign
    };
    extrude(&base, axis, count, dir_sign)
}

/// Bounding-box extents of a set of cells, or `None` for an empty set.
pub fn cell_bounds(cells: &[IVec3]) -> Option<IVec3> {
    let first = cells.first()?;
    let mut min = *first;
    let mut max = *first;
    for c in &cells[1..] {
        min = min.min(*c);
        max = max.max(*c);
    }
    Some(max - min + IVec3::ONE)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn set(v: Vec<IVec3>) -> HashSet<(i32, i32, i32)> {
        v.into_iter().map(|p| (p.x, p.y, p.z)).collect()
    }

    #[test]
    fn rect_filled_3x3_y_axis_emits_9_cells_on_plane() {
        let cells = rect_cells(IVec3::new(0, 5, 0), IVec3::new(2, 5, 2), 1, true);
        assert_eq!(cells.len(), 9);
        for c in &cells {
            assert_eq!(c.y, 5);
        }
        let s = set(cells);
        for u in 0..=2 {
            for v in 0..=2 {
                assert!(s.contains(&(u, 5, v)));
            }
        }
    }

    #[test]
    fn rect_outline_3x3_emits_8_edge_cells() {
        let cells = rect_cells(IVec3::new(0, 5, 0), IVec3::new(2, 5, 2), 1, false);
        assert_eq!(cells.len(), 8);
        assert!(!set(cells).contains(&(1, 5, 1)));
    }

    #[test]
    fn rect_axis_orientation_x_axis_constant_x() {
        let cells = rect_cells(IVec3::new(3, 0, 0), IVec3::new(3, 4, 4), 0, true);
        for c in &cells {
            assert_eq!(c.x, 3);
        }
        assert_eq!(cells.len(), 25);
    }

    #[test]
    fn rect_handles_inverted_corners() {
        let a = rect_cells(IVec3::new(2, 0, 2), IVec3::new(0, 0, 0), 1, true);
        let b = rect_cells(IVec3::new(0, 0, 0), IVec3::new(2, 0, 2), 1, true);
        assert_eq!(set(a), set(b));
    }

    #[test]
    fn ellipse_inscribed_in_rect() {
        let cells = ellipse_cells(IVec3::new(0, 0, 0), IVec3::new(4, 0, 4), 1, true);
        assert!(!cells.is_empty());
        let s = set(cells);
        assert!(s.contains(&(2, 0, 2)));
        // Corners of bounding rect lie outside an inscribed ellipse.
        assert!(!s.contains(&(0, 0, 0)));
        assert!(!s.contains(&(4, 0, 4)));
    }

    #[test]
    fn ellipse_outline_subset_of_filled() {
        let filled = set(ellipse_cells(
            IVec3::new(0, 0, 0),
            IVec3::new(6, 0, 6),
            1,
            true,
        ));
        let outline = set(ellipse_cells(
            IVec3::new(0, 0, 0),
            IVec3::new(6, 0, 6),
            1,
            false,
        ));
        assert!(outline.is_subset(&filled));
        assert!(outline.len() < filled.len());
    }

    #[test]
    fn line2d_endpoints_present() {
        let cells = line2d_cells(IVec3::new(0, 0, 0), IVec3::new(5, 0, 2), 1);
        let s = set(cells);
        assert!(s.contains(&(0, 0, 0)));
        assert!(s.contains(&(5, 0, 2)));
    }

    #[test]
    fn line2d_horizontal_count() {
        let cells = line2d_cells(IVec3::new(0, 0, 0), IVec3::new(4, 0, 0), 1);
        assert_eq!(cells.len(), 5);
    }

    #[test]
    fn line2d_single_point() {
        let cells = line2d_cells(IVec3::new(2, 0, 2), IVec3::new(2, 0, 2), 1);
        assert_eq!(cells.len(), 1);
        assert_eq!(cells[0], IVec3::new(2, 0, 2));
    }

    #[test]
    fn extrude_thickness_1_returns_input() {
        let base = vec![IVec3::new(0, 5, 0), IVec3::new(1, 5, 0)];
        let out = extrude(&base, 1, 1, 1);
        assert_eq!(out.len(), 2);
        assert_eq!(set(out), set(base));
    }

    #[test]
    fn extrude_positive_sign_advances_axis() {
        let base = vec![IVec3::new(0, 0, 0)];
        let out = extrude(&base, 1, 3, 1);
        assert_eq!(
            out,
            vec![
                IVec3::new(0, 0, 0),
                IVec3::new(0, 1, 0),
                IVec3::new(0, 2, 0)
            ]
        );
    }

    #[test]
    fn extrude_negative_sign_retreats_axis() {
        let base = vec![IVec3::new(0, 5, 0)];
        let out = extrude(&base, 1, 3, -1);
        assert_eq!(
            out,
            vec![
                IVec3::new(0, 5, 0),
                IVec3::new(0, 4, 0),
                IVec3::new(0, 3, 0)
            ]
        );
    }

    #[test]
    fn extrude_zero_sign_treated_as_positive() {
        let base = vec![IVec3::new(0, 0, 0)];
        let out = extrude(&base, 1, 2, 0);
        assert_eq!(out, vec![IVec3::new(0, 0, 0), IVec3::new(0, 1, 0)]);
    }

    #[test]
    fn extrude_thickness_clamped_to_1() {
        let base = vec![IVec3::new(0, 0, 0)];
        let out = extrude(&base, 1, 0, 1);
        assert_eq!(out, vec![IVec3::new(0, 0, 0)]);
    }

    #[test]
    fn compute_shape_rect_3x3_thickness_1() {
        let cells = compute_shape_cells(
            ShapePrimitive::Rectangle,
            IVec3::new(0, 5, 0),
            IVec3::new(2, 5, 2),
            1,
            0,
            1,
        );
        assert_eq!(cells.len(), 9);
        for c in &cells {
            assert_eq!(c.y, 5);
        }
    }

    #[test]
    fn compute_shape_rect_3x3_thickness_3() {
        let cells = compute_shape_cells(
            ShapePrimitive::Rectangle,
            IVec3::new(0, 5, 0),
            IVec3::new(2, 5, 2),
            1,
            2,
            1,
        );
        assert_eq!(cells.len(), 27);
        let ys: HashSet<i32> = cells.iter().map(|c| c.y).collect();
        let mut ys: Vec<_> = ys.into_iter().collect();
        ys.sort();
        assert_eq!(ys, vec![5, 6, 7]);
    }

    #[test]
    fn compute_shape_ellipse_5x5() {
        let cells = compute_shape_cells(
            ShapePrimitive::Ellipse,
            IVec3::new(0, 0, 0),
            IVec3::new(4, 0, 4),
            1,
            0,
            1,
        );
        let s: HashSet<(i32, i32, i32)> = cells.iter().map(|p| (p.x, p.y, p.z)).collect();
        assert!(s.contains(&(2, 0, 2)));
        // Corners of bounding 5×5 rect should be outside an inscribed ellipse.
        assert!(!s.contains(&(0, 0, 0)));
        assert!(!s.contains(&(4, 0, 4)));
    }

    #[test]
    fn compute_shape_line_endpoints() {
        let cells = compute_shape_cells(
            ShapePrimitive::Line,
            IVec3::new(0, 0, 0),
            IVec3::new(5, 0, 2),
            1,
            0,
            1,
        );
        let s: HashSet<(i32, i32, i32)> = cells.iter().map(|p| (p.x, p.y, p.z)).collect();
        assert!(s.contains(&(0, 0, 0)));
        assert!(s.contains(&(5, 0, 2)));
    }

    #[test]
    fn cell_bounds_empty_returns_none() {
        assert_eq!(cell_bounds(&[]), None);
    }

    #[test]
    fn cell_bounds_single_cell() {
        let cells = vec![IVec3::new(3, 5, 7)];
        assert_eq!(cell_bounds(&cells), Some(IVec3::new(1, 1, 1)));
    }

    #[test]
    fn cell_bounds_multiple_cells() {
        let cells = vec![IVec3::new(1, 2, 3), IVec3::new(5, 6, 7)];
        // extents = max - min + 1
        assert_eq!(cell_bounds(&cells), Some(IVec3::new(5, 5, 5)));
    }

    #[test]
    fn cell_bounds_negative_coords() {
        let cells = vec![IVec3::new(-3, -2, -1), IVec3::new(1, 2, 3)];
        assert_eq!(cell_bounds(&cells), Some(IVec3::new(5, 5, 5)));
    }
}
