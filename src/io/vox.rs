use crate::grid::{Color8, VoxelGrid, snap_to_allowed_size};
use anyhow::Result;
use bevy::math::IVec3;
use dot_vox::{Color, DotVoxData, Model, Size, Voxel};
use std::collections::HashMap;
use std::path::Path;

// MagicaVoxel is Z-up; Roxel is Y-up. Both import and export remap
// (x, y_roxel, z_roxel) <-> (x_vox, z_vox, y_vox) so foreign .vox files load
// upright and Roxel-exported files open upright in MagicaVoxel.

pub fn export(path: &Path, grid: &VoxelGrid) -> Result<()> {
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
                    nearest(&palette, c)
                };
                // Y-up → Z-up: emit (x, z_roxel, y_roxel) so columns point along MV's +Z.
                voxels.push(Voxel {
                    x: x as u8,
                    y: z as u8,
                    z: y as u8,
                    i: idx,
                });
            }
        }
    }

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

pub fn import(path: &Path, grid: &mut VoxelGrid) -> Result<()> {
    let path_str = path
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("non-UTF-8 path"))?;
    let data = dot_vox::load(path_str).map_err(|e| anyhow::anyhow!("{e}"))?;
    if data.models.is_empty() {
        anyhow::bail!("file contains no models");
    }
    if data.models.len() > 1 {
        eprintln!(
            "Import .vox: {} models found, using first only",
            data.models.len()
        );
    }
    let model = &data.models[0];

    // Compute max occupied extent on each remapped axis. MV stores extents in
    // model.size but the actual voxel cluster can be smaller, so use voxels.
    let mut max_extent = 0u32;
    for v in &model.voxels {
        // After remap: roxel.x = v.x, roxel.y = v.z, roxel.z = v.y.
        max_extent = max_extent.max(v.x as u32).max(v.z as u32).max(v.y as u32);
    }
    // +1 because extent is inclusive index.
    let needed = (max_extent + 1) as usize;
    grid.resize(snap_to_allowed_size(needed));

    let mut dropped = 0usize;
    for v in &model.voxels {
        let rx = v.x as i32;
        let ry = v.z as i32;
        let rz = v.y as i32;
        if rx < 0
            || ry < 0
            || rz < 0
            || rx >= grid.size as i32
            || ry >= grid.size as i32
            || rz >= grid.size as i32
        {
            dropped += 1;
            continue;
        }
        let col = data.palette.get(v.i as usize).copied().unwrap_or(Color {
            r: 255,
            g: 255,
            b: 255,
            a: 255,
        });
        grid.set(IVec3::new(rx, ry, rz), Some([col.r, col.g, col.b, 255]));
    }
    if dropped > 0 {
        eprintln!(
            "Import .vox: dropped {dropped} voxels outside {sz}³",
            sz = grid.size
        );
    }
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
    use crate::io::test_util::tmp_path as raw_tmp_path;
    use bevy::math::IVec3;
    use std::path::PathBuf;

    fn tmp_path(name: &str) -> PathBuf {
        raw_tmp_path(name, "vox")
    }

    #[test]
    fn export_remaps_y_up_to_z_up() {
        // A voxel at Roxel (0, 5, 0) should land at MV (0, 0, 5).
        let mut g = VoxelGrid::default();
        g.resize(32);
        g.set(IVec3::new(0, 5, 0), Some([10, 20, 30, 255]));
        let path = tmp_path("axis-export");
        export(&path, &g).expect("export");
        let data = dot_vox::load(path.to_str().unwrap()).expect("load");
        let v = &data.models[0].voxels[0];
        assert_eq!((v.x, v.y, v.z), (0, 0, 5));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn import_remaps_z_up_to_y_up() {
        // Hand-build a MV-style file with one voxel at MV (0, 0, 5) and import it.
        // Expect Roxel grid to have the voxel at (0, 5, 0).
        let path = tmp_path("axis-import");
        let mut palette = vec![Color {
            r: 99,
            g: 88,
            b: 77,
            a: 255,
        }];
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
                    x: 32,
                    y: 32,
                    z: 32,
                },
                voxels: vec![Voxel {
                    x: 0,
                    y: 0,
                    z: 5,
                    i: 0,
                }],
            }],
            palette,
            materials: vec![],
            scenes: vec![],
            layers: vec![],
        };
        let mut f = std::fs::File::create(&path).expect("create");
        data.write_vox(&mut f).expect("write");

        let mut g = VoxelGrid::default();
        import(&path, &mut g).expect("import");
        assert_eq!(g.get(IVec3::new(0, 5, 0)), Some([99, 88, 77, 255]));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn import_roundtrips_through_export() {
        let mut g = VoxelGrid::default();
        g.resize(32);
        let pts: [(IVec3, [u8; 4]); 3] = [
            (IVec3::new(0, 0, 0), [255, 0, 0, 255]),
            (IVec3::new(5, 12, 7), [0, 255, 0, 255]),
            (IVec3::new(31, 31, 31), [0, 0, 255, 255]),
        ];
        for (p, c) in pts {
            g.set(p, Some(c));
        }
        let path = tmp_path("roundtrip");
        export(&path, &g).expect("export");

        let mut loaded = VoxelGrid::default();
        loaded.set(IVec3::new(1, 1, 1), Some([9, 9, 9, 255]));
        import(&path, &mut loaded).expect("import");

        for (p, c) in pts {
            assert_eq!(loaded.get(p), Some(c));
        }
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn import_snaps_to_smallest_fitting_size() {
        // 40³ source → grid resizes up to 64.
        let path = tmp_path("snap");
        let mut palette = vec![Color {
            r: 1,
            g: 2,
            b: 3,
            a: 255,
        }];
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
                    x: 40,
                    y: 40,
                    z: 40,
                },
                voxels: vec![Voxel {
                    x: 39,
                    y: 0,
                    z: 0,
                    i: 0,
                }],
            }],
            palette,
            materials: vec![],
            scenes: vec![],
            layers: vec![],
        };
        let mut f = std::fs::File::create(&path).unwrap();
        data.write_vox(&mut f).unwrap();
        let mut g = VoxelGrid::default();
        import(&path, &mut g).expect("import");
        assert_eq!(g.size, 64);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn import_uses_first_model_only() {
        let path = tmp_path("multi");
        let mut palette = vec![
            Color {
                r: 10,
                g: 10,
                b: 10,
                a: 255,
            },
            Color {
                r: 200,
                g: 200,
                b: 200,
                a: 255,
            },
        ];
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
            models: vec![
                Model {
                    size: Size {
                        x: 32,
                        y: 32,
                        z: 32,
                    },
                    voxels: vec![Voxel {
                        x: 0,
                        y: 0,
                        z: 0,
                        i: 0,
                    }],
                },
                Model {
                    size: Size {
                        x: 32,
                        y: 32,
                        z: 32,
                    },
                    voxels: vec![Voxel {
                        x: 10,
                        y: 0,
                        z: 0,
                        i: 1,
                    }],
                },
            ],
            palette,
            materials: vec![],
            scenes: vec![],
            layers: vec![],
        };
        let mut f = std::fs::File::create(&path).unwrap();
        data.write_vox(&mut f).unwrap();
        let mut g = VoxelGrid::default();
        import(&path, &mut g).expect("import");
        assert_eq!(g.get(IVec3::new(0, 0, 0)), Some([10, 10, 10, 255]));
        assert_eq!(g.get(IVec3::new(10, 0, 0)), None);
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
        assert_eq!(nearest(&palette, [250, 10, 10, 255]), 1);
        assert_eq!(nearest(&palette, [5, 5, 5, 255]), 0);
    }
}
