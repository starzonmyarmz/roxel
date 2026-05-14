use crate::grid::{Color8, VoxelGrid};
use anyhow::Result;
use dot_vox::{Color, DotVoxData, Model, Size, Voxel};
use std::collections::HashMap;
use std::path::Path;

pub fn export(path: &Path, grid: &VoxelGrid) -> Result<()> {
    // Build palette from unique colors (capped at 255 — 0 reserved for empty).
    let mut palette_map: HashMap<Color8, u8> = HashMap::new();
    let mut palette: Vec<Color> = Vec::new();
    let mut voxels: Vec<Voxel> = Vec::new();

    for x in 0..grid.size {
        for y in 0..grid.size {
            for z in 0..grid.size {
                let Some(c) = grid.cell(x, y, z) else {
                    continue;
                };
                let idx = if let Some(&i) = palette_map.get(&c) {
                    i
                } else if palette.len() < 255 {
                    let i = palette.len() as u8;
                    palette.push(Color {
                        r: c[0],
                        g: c[1],
                        b: c[2],
                        a: 255,
                    });
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
        palette.push(Color {
            r: 0,
            g: 0,
            b: 0,
            a: 0,
        });
    }

    let data = DotVoxData {
        version: 150,
        index_map: (0u8..=255u8).collect(),
        models: vec![Model {
            size: Size {
                x: grid.size as u32,
                y: grid.size as u32,
                z: grid.size as u32,
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

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::math::IVec3;
    use std::path::PathBuf;

    fn tmp_path(name: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        let pid = std::process::id();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        p.push(format!("roxel-test-{pid}-{nanos}-{name}.vox"));
        p
    }

    #[test]
    fn export_roundtrips_through_dot_vox_loader() {
        let mut g = VoxelGrid::default();
        g.resize(32);
        let pts: [(IVec3, [u8; 4]); 3] = [
            (IVec3::new(0, 0, 0), [255, 0, 0, 255]),
            (IVec3::new(5, 5, 5), [0, 255, 0, 255]),
            (IVec3::new(31, 31, 31), [0, 0, 255, 255]),
        ];
        for (p, c) in pts {
            g.set(p, Some(c));
        }
        let path = tmp_path("roundtrip");
        export(&path, &g).expect("export");

        let data = dot_vox::load(path.to_str().unwrap()).expect("dot_vox load");
        assert_eq!(data.models.len(), 1);
        let model = &data.models[0];
        assert_eq!(model.size.x, 32);
        assert_eq!(model.size.y, 32);
        assert_eq!(model.size.z, 32);
        assert_eq!(model.voxels.len(), 3);
        assert_eq!(data.palette.len(), 256);

        // Verify each placed cell is present with the right RGB.
        for (p, c) in pts {
            let v = model
                .voxels
                .iter()
                .find(|v| v.x as i32 == p.x && v.y as i32 == p.y && v.z as i32 == p.z)
                .expect("voxel present");
            let col = &data.palette[v.i as usize];
            assert_eq!([col.r, col.g, col.b], [c[0], c[1], c[2]]);
        }
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn export_dedupes_palette_entries() {
        let mut g = VoxelGrid::default();
        g.resize(32);
        let red: [u8; 4] = [200, 0, 0, 255];
        g.set(IVec3::new(0, 0, 0), Some(red));
        g.set(IVec3::new(1, 0, 0), Some(red));
        g.set(IVec3::new(2, 0, 0), Some(red));
        let path = tmp_path("dedupe");
        export(&path, &g).expect("export");
        let data = dot_vox::load(path.to_str().unwrap()).expect("load");
        let used: std::collections::HashSet<u8> =
            data.models[0].voxels.iter().map(|v| v.i).collect();
        assert_eq!(used.len(), 1);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn nearest_picks_closest_rgb() {
        let palette = vec![
            Color {
                r: 0,
                g: 0,
                b: 0,
                a: 255,
            },
            Color {
                r: 255,
                g: 0,
                b: 0,
                a: 255,
            },
            Color {
                r: 0,
                g: 255,
                b: 0,
                a: 255,
            },
        ];
        // Closest to pure red.
        assert_eq!(nearest(&palette, [250, 10, 10, 255]), 1);
        // Closest to black.
        assert_eq!(nearest(&palette, [5, 5, 5, 255]), 0);
    }
}
