use crate::grid::{Color8, GRID, VoxelGrid};
use anyhow::Result;
use dot_vox::{Color, DotVoxData, Model, Size, Voxel};
use std::collections::HashMap;
use std::path::Path;

pub fn export(path: &Path, grid: &VoxelGrid) -> Result<()> {
    // Build palette from unique colors (capped at 255 — 0 reserved for empty).
    let mut palette_map: HashMap<Color8, u8> = HashMap::new();
    let mut palette: Vec<Color> = Vec::new();
    let mut voxels: Vec<Voxel> = Vec::new();

    for x in 0..GRID {
        for y in 0..GRID {
            for z in 0..GRID {
                let Some(c) = grid.cells[x][y][z] else { continue; };
                let idx = if let Some(&i) = palette_map.get(&c) {
                    i
                } else if palette.len() < 255 {
                    let i = palette.len() as u8;
                    palette.push(Color { r: c[0], g: c[1], b: c[2], a: 255 });
                    palette_map.insert(c, i);
                    i
                } else {
                    // Palette full — fall back to nearest existing color.
                    nearest(&palette, c)
                };
                voxels.push(Voxel {
                    x: x as u8,
                    y: y as u8,
                    z: z as u8,
                    i: idx,
                });
            }
        }
    }

    // Pad palette to 256 entries.
    while palette.len() < 256 {
        palette.push(Color { r: 0, g: 0, b: 0, a: 0 });
    }

    let data = DotVoxData {
        version: 150,
        index_map: (0u8..=255u8).collect(),
        models: vec![Model {
            size: Size {
                x: GRID as u32,
                y: GRID as u32,
                z: GRID as u32,
            },
            voxels,
        }],
        palette,
        materials: vec![],
        scenes: vec![],
        layers: vec![],
    };
    let mut file = std::fs::File::create(path)?;
    data.write_vox(&mut file)?;
    Ok(())
}

fn nearest(palette: &[Color], c: Color8) -> u8 {
    let mut best = 0u8;
    let mut best_d = i32::MAX;
    for (i, p) in palette.iter().enumerate() {
        let dr = p.r as i32 - c[0] as i32;
        let dg = p.g as i32 - c[1] as i32;
        let db = p.b as i32 - c[2] as i32;
        let d = dr * dr + dg * dg + db * db;
        if d < best_d {
            best_d = d;
            best = i as u8;
        }
    }
    best
}
