use crate::grid::VoxelGrid;
use crate::mesh::FACES;
use anyhow::Result;
use std::io::Write;
use std::path::Path;

// Naive face-emit identical to renderer mesher; writes OBJ with per-vertex colors
// (Blender-style "v x y z r g b" extension).
pub fn export(path: &Path, grid: &VoxelGrid) -> Result<()> {
    let mut file = std::io::BufWriter::new(std::fs::File::create(path)?);
    writeln!(file, "# Exported by Roxel")?;

    let mut vert_idx: u32 = 1; // OBJ indices are 1-based.
    let mut normal_idx: u32 = 1;
    let mut faces_buf: Vec<(u32, u32, u32, u32, u32)> = Vec::new();

    let size_i = grid.size_i();
    for x in 0..grid.size {
        for y in 0..grid.size {
            for z in 0..grid.size {
                let Some(rgba) = grid.cell(x, y, z) else { continue; };
                let cx = x as i32;
                let cy = y as i32;
                let cz = z as i32;
                let r = rgba[0] as f32 / 255.0;
                let g = rgba[1] as f32 / 255.0;
                let b = rgba[2] as f32 / 255.0;

                for f in &FACES {
                    let nx = cx + f.d.x;
                    let ny = cy + f.d.y;
                    let nz = cz + f.d.z;
                    let neighbor_filled = nx >= 0 && nx < size_i
                        && ny >= 0 && ny < size_i
                        && nz >= 0 && nz < size_i
                        && grid.cell(nx as usize, ny as usize, nz as usize).is_some();
                    if neighbor_filled {
                        continue;
                    }
                    writeln!(file, "vn {} {} {}", f.d.x, f.d.y, f.d.z)?;
                    let n_id = normal_idx;
                    normal_idx += 1;

                    let mut quad = [0u32; 4];
                    for (i, c) in f.corners.iter().enumerate() {
                        writeln!(
                            file,
                            "v {} {} {} {:.3} {:.3} {:.3}",
                            cx + c[0],
                            cy + c[1],
                            cz + c[2],
                            r, g, b
                        )?;
                        quad[i] = vert_idx;
                        vert_idx += 1;
                    }
                    faces_buf.push((quad[0], quad[1], quad[2], quad[3], n_id));
                }
            }
        }
    }

    for (a, b, c, d, n) in faces_buf {
        writeln!(file, "f {a}//{n} {b}//{n} {c}//{n} {d}//{n}")?;
    }
    Ok(())
}
