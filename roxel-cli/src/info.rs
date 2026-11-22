use crate::color::to_hex;
use roxel::grid::VoxelGrid;
use serde::Serialize;
use std::collections::HashMap;

#[derive(Debug, PartialEq, Serialize)]
pub struct Info {
    pub voxels: usize,
    pub bounds: Option<Bounds>,
    pub colors: Vec<ColorCount>,
}

#[derive(Debug, PartialEq, Serialize)]
pub struct Bounds {
    pub min: [i32; 3],
    pub max: [i32; 3],
    pub size: [i32; 3],
}

#[derive(Debug, PartialEq, Serialize)]
pub struct ColorCount {
    pub color: String,
    pub count: usize,
}

pub fn info(grid: &VoxelGrid) -> Info {
    let bounds = grid.bounding_box().map(|(min, max)| Bounds {
        min: min.to_array(),
        max: max.to_array(),
        size: (max - min + 1).to_array(),
    });

    let mut counts: HashMap<[u8; 4], usize> = HashMap::new();
    for (_, c) in grid.iter_occupied() {
        *counts.entry(c).or_default() += 1;
    }
    let mut colors: Vec<ColorCount> = counts
        .into_iter()
        .map(|(c, count)| ColorCount {
            color: to_hex(c),
            count,
        })
        .collect();
    colors.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.color.cmp(&b.color)));

    Info {
        voxels: grid.count(),
        bounds,
        colors,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::IVec3;

    #[test]
    fn empty_grid_has_no_bounds() {
        let i = info(&VoxelGrid::default());
        assert_eq!(i.voxels, 0);
        assert_eq!(i.bounds, None);
        assert!(i.colors.is_empty());
    }

    #[test]
    fn bounds_are_inclusive_with_size() {
        let mut g = VoxelGrid::default();
        g.set(IVec3::new(-2, 0, 5), Some([0, 0, 0, 255]));
        g.set(IVec3::new(3, 4, 5), Some([0, 0, 0, 255]));
        let b = info(&g).bounds.unwrap();
        assert_eq!(b.min, [-2, 0, 5]);
        assert_eq!(b.max, [3, 4, 5]);
        assert_eq!(b.size, [6, 5, 1]);
    }

    #[test]
    fn colors_sorted_by_count_then_hex() {
        let mut g = VoxelGrid::default();
        let red = [255, 0, 0, 255];
        let blue = [0, 0, 255, 255];
        let green = [0, 255, 0, 255];
        g.set(IVec3::new(0, 0, 0), Some(red));
        g.set(IVec3::new(1, 0, 0), Some(green));
        g.set(IVec3::new(2, 0, 0), Some(green));
        g.set(IVec3::new(3, 0, 0), Some(blue));
        let colors: Vec<(String, usize)> = info(&g)
            .colors
            .into_iter()
            .map(|c| (c.color, c.count))
            .collect();
        assert_eq!(
            colors,
            vec![
                ("#00ff00".into(), 2),
                ("#0000ff".into(), 1),
                ("#ff0000".into(), 1),
            ]
        );
    }

    #[test]
    fn serializes_to_expected_json() {
        let mut g = VoxelGrid::default();
        g.set(IVec3::new(1, 2, 3), Some([18, 52, 86, 255]));
        let json = serde_json::to_value(info(&g)).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "voxels": 1,
                "bounds": { "min": [1, 2, 3], "max": [1, 2, 3], "size": [1, 1, 1] },
                "colors": [{ "color": "#123456", "count": 1 }],
            })
        );
    }
}
