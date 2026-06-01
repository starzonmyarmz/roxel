use crate::grid::{Color8, VoxelGrid};
use anyhow::Result;
use bevy::math::IVec3;
use dot_vox::{Color, DotVoxData, Model, Size, Voxel};
use std::collections::{BTreeSet, HashMap};
use std::path::Path;

/// `.vox` format hard cap on each axis: voxel coords are stored in a `u8`,
/// so the largest extent any one axis can carry is 256.
const VOX_MAX_EXTENT: i32 = 256;

// MagicaVoxel is Z-up; Roxel is Y-up. Both import and export remap
// (x, y_roxel, z_roxel) <-> (x_vox, z_vox, y_vox) so foreign .vox files load
// upright and Roxel-exported files open upright in MagicaVoxel.

pub fn export(path: &Path, grid: &VoxelGrid) -> Result<()> {
    // Open-world grids can carry negative coords; `.vox` coords are unsigned
    // and start at origin. Shift the whole model by `-min` so its min corner
    // lands at (0, 0, 0) in MagicaVoxel space. The shift is per-axis on the
    // *remapped* axes so the user sees the model upright in MV.
    let Some((min, max)) = grid.bounding_box() else {
        anyhow::bail!("nothing to export — grid is empty");
    };
    let extent = max - min;
    // After remap to MV (x, z_roxel, y_roxel) the size axes are still bounded
    // by the same per-axis extents — check each.
    if extent.x + 1 > VOX_MAX_EXTENT
        || extent.y + 1 > VOX_MAX_EXTENT
        || extent.z + 1 > VOX_MAX_EXTENT
    {
        anyhow::bail!(
            ".vox supports a maximum extent of {VOX_MAX_EXTENT} per axis; \
             this scene measures {}×{}×{}",
            extent.x + 1,
            extent.y + 1,
            extent.z + 1
        );
    }

    let mut palette_map: HashMap<Color8, u8> = HashMap::new();
    let mut palette: Vec<Color> = Vec::new();
    let mut voxels: Vec<Voxel> = Vec::new();

    for (p, c) in grid.iter_occupied() {
        let shifted = p - min;
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
            x: shifted.x as u8,
            y: shifted.z as u8,
            z: shifted.y as u8,
            i: idx,
        });
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
                x: (extent.x + 1) as u32,
                y: (extent.z + 1) as u32,
                z: (extent.y + 1) as u32,
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

/// Imports a `.vox` model into `grid` and returns the model's palette as the
/// distinct colors actually referenced by placed voxels, in ascending palette
/// index order. The caller turns this into a swatch palette; the full 256-entry
/// MagicaVoxel ramp is deliberately *not* returned — unused default entries
/// would be palette noise.
pub fn import(path: &Path, grid: &mut VoxelGrid) -> Result<Vec<Color8>> {
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

    let mut dropped = 0usize;
    let mut used: BTreeSet<u8> = BTreeSet::new();
    for v in &model.voxels {
        let rx = v.x as i32;
        let ry = v.z as i32;
        let rz = v.y as i32;
        if ry < 0 {
            dropped += 1;
            continue;
        }
        used.insert(v.i);
        let col = data.palette.get(v.i as usize).copied().unwrap_or(Color {
            r: 255,
            g: 255,
            b: 255,
            a: 255,
        });
        grid.set(IVec3::new(rx, ry, rz), Some([col.r, col.g, col.b, 255]));
    }
    if dropped > 0 {
        eprintln!("Import .vox: dropped {dropped} voxels below the floor");
    }

    let palette: Vec<Color8> = used
        .into_iter()
        .map(|i| {
            let c = data.palette.get(i as usize).copied().unwrap_or(Color {
                r: 255,
                g: 255,
                b: 255,
                a: 255,
            });
            [c.r, c.g, c.b, 255]
        })
        .collect();
    Ok(palette)
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

    fn make_palette(rgb: &[(u8, u8, u8)]) -> Vec<Color> {
        let mut palette: Vec<Color> = rgb
            .iter()
            .map(|&(r, g, b)| Color { r, g, b, a: 255 })
            .collect();
        while palette.len() < 256 {
            palette.push(Color {
                r: 0,
                g: 0,
                b: 0,
                a: 0,
            });
        }
        palette
    }

    #[test]
    fn export_remaps_y_up_to_z_up() {
        let mut g = VoxelGrid::default();
        g.set(IVec3::new(0, 5, 0), Some([10, 20, 30, 255]));
        let path = tmp_path("axis-export");
        export(&path, &g).expect("export");
        let data = dot_vox::load(path.to_str().unwrap()).expect("load");
        let v = &data.models[0].voxels[0];
        // After AABB-shift, (0, 5, 0) is at the model's min corner → (0, 0, 0)
        // in Roxel space, which becomes (0, 0, 0) in MV after axis remap.
        assert_eq!((v.x, v.y, v.z), (0, 0, 0));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn export_shifts_to_origin_for_negative_coords() {
        let mut g = VoxelGrid::default();
        g.set(IVec3::new(-5, 0, -10), Some([1, 2, 3, 255]));
        g.set(IVec3::new(5, 7, 0), Some([4, 5, 6, 255]));
        let path = tmp_path("shift");
        export(&path, &g).expect("export");
        let data = dot_vox::load(path.to_str().unwrap()).expect("load");
        // Model size is the AABB extent (max - min + 1) after axis remap:
        // x = 5 - (-5) + 1 = 11, y_mv = z_roxel extent = 0 - (-10) + 1 = 11,
        // z_mv = y_roxel extent = 7 - 0 + 1 = 8.
        let model = &data.models[0];
        assert_eq!(model.size.x, 11);
        assert_eq!(model.size.y, 11);
        assert_eq!(model.size.z, 8);
        // First voxel was at Roxel min → MV (0, 0, 0) after both shift and remap.
        let zero = model.voxels.iter().any(|v| (v.x, v.y, v.z) == (0, 0, 0));
        assert!(zero, "voxels = {:?}", model.voxels);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn export_refuses_when_extent_exceeds_256() {
        let mut g = VoxelGrid::default();
        g.set(IVec3::new(0, 0, 0), Some([1, 2, 3, 255]));
        g.set(IVec3::new(300, 0, 0), Some([4, 5, 6, 255]));
        let path = tmp_path("too-big");
        let err = export(&path, &g).expect_err("must refuse");
        assert!(err.to_string().contains("256"), "msg={err}");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn import_remaps_z_up_to_y_up() {
        let path = tmp_path("axis-import");
        let palette = make_palette(&[(99, 88, 77)]);
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

        // After roundtrip the model is AABB-shifted to min=(0,0,0), so the
        // input pts land at (p - (0,0,0)) — same coords.
        for (p, c) in pts {
            assert_eq!(loaded.get(p), Some(c));
        }
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn import_does_not_resize_grid() {
        // Open world has no resize; the importer just `grid.set`s each cell.
        let path = tmp_path("noresize");
        let palette = make_palette(&[(1, 2, 3)]);
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
        // Voxel landed at (39, 0, 0) — well outside the old 32³ default,
        // proving the open-world grid took it without resizing.
        assert_eq!(g.get(IVec3::new(39, 0, 0)), Some([1, 2, 3, 255]));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn import_uses_first_model_only() {
        let path = tmp_path("multi");
        let palette = make_palette(&[(10, 10, 10), (200, 200, 200)]);
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
    fn import_returns_used_colors_as_palette() {
        // Palette index order, deduped, only colors actually placed. Index 2 is
        // referenced twice (one dedup) and index 1 is unused → excluded.
        let path = tmp_path("palette");
        let palette = make_palette(&[(10, 10, 10), (20, 20, 20), (30, 30, 30)]);
        let data = DotVoxData {
            version: 150,
            index_map: (0u8..=255u8).collect(),
            models: vec![Model {
                size: Size {
                    x: 32,
                    y: 32,
                    z: 32,
                },
                voxels: vec![
                    Voxel {
                        x: 1,
                        y: 0,
                        z: 0,
                        i: 2,
                    },
                    Voxel {
                        x: 0,
                        y: 0,
                        z: 0,
                        i: 0,
                    },
                    Voxel {
                        x: 2,
                        y: 0,
                        z: 0,
                        i: 2,
                    },
                ],
            }],
            palette,
            materials: vec![],
            scenes: vec![],
            layers: vec![],
        };
        let mut f = std::fs::File::create(&path).unwrap();
        data.write_vox(&mut f).unwrap();
        let mut g = VoxelGrid::default();
        let colors = import(&path, &mut g).expect("import");
        // Ascending index order (0 then 2), deduped, index 1 absent.
        assert_eq!(colors, vec![[10, 10, 10, 255], [30, 30, 30, 255]]);
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
