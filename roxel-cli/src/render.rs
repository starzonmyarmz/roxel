use anyhow::{Result, bail};
use glam::{Mat4, Vec2, Vec3, Vec4};
use roxel::grid::VoxelGrid;
use roxel::mesh_util::FACES;
use roxel::projection::{ProjectedQuad, collect_cell_quads, project_quads, shade_color};
use std::path::Path;

pub const DEFAULT_SCALE: u32 = 8;
pub const MAX_SCALE: u32 = 256;
pub const VIEWS: u8 = 4;
const MAX_DIMENSION: u32 = 16_384;
const MAX_PIXELS: u64 = 64 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RenderOptions {
    pub view: u8,
    pub scale: u32,
    pub background: [u8; 4],
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self {
            view: 0,
            scale: DEFAULT_SCALE,
            background: [0, 0, 0, 0],
        }
    }
}

pub struct Image {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

#[cfg(test)]
impl Image {
    pub fn pixel(&self, x: u32, y: u32) -> [u8; 4] {
        let i = ((y * self.width + x) * 4) as usize;
        self.rgba[i..i + 4].try_into().unwrap()
    }
}

/// Unit vector from the model toward the camera: the classic isometric angle
/// (pitch atan(1/√2)), stepped 90° around the Y axis per view.
pub fn view_dir(view: u8) -> Vec3 {
    let yaw = (45.0 + 90.0 * f32::from(view % VIEWS)).to_radians();
    let pitch = (1.0 / 2f32.sqrt()).atan();
    Vec3::new(
        pitch.cos() * yaw.sin(),
        pitch.sin(),
        pitch.cos() * yaw.cos(),
    )
}

pub struct Camera {
    pub eye: Vec3,
    pub view: Mat4,
    pub view_proj: Mat4,
    pub size: Vec2,
}

/// Orthographic camera framing the AABB `[lo, hi]` at `scale` pixels per
/// world unit.
pub fn camera(lo: Vec3, hi: Vec3, view: u8, scale: u32) -> Camera {
    let center = (lo + hi) * 0.5;
    let radius = (hi - lo).length() * 0.5;
    // Every component of view_dir is ±1/√3, so at this distance the eye sits
    // outside each face plane of the AABB and collect_cell_quads' per-face
    // camera test agrees with the ortho view direction.
    let distance = 2.0 * radius + 1.0;
    let eye = center + view_dir(view) * distance;
    let view_m = glam::camera::rh::view::look_at_mat4(eye, center, Vec3::Y);

    let mut vmin = Vec2::splat(f32::INFINITY);
    let mut vmax = Vec2::splat(f32::NEG_INFINITY);
    for i in 0..8 {
        let corner = Vec3::new(
            if i & 1 == 0 { lo.x } else { hi.x },
            if i & 2 == 0 { lo.y } else { hi.y },
            if i & 4 == 0 { lo.z } else { hi.z },
        );
        let v = (view_m * Vec4::new(corner.x, corner.y, corner.z, 1.0)).truncate();
        vmin = vmin.min(Vec2::new(v.x, v.y));
        vmax = vmax.max(Vec2::new(v.x, v.y));
    }
    let s = scale as f32;
    let size = ((vmax - vmin) * s).ceil();
    let proj = glam::camera::rh::proj::opengl::orthographic(
        vmin.x,
        vmin.x + size.x / s,
        vmin.y,
        vmin.y + size.y / s,
        0.01,
        distance + radius + 1.0,
    );
    Camera {
        eye,
        view: view_m,
        view_proj: proj * view_m,
        size,
    }
}

pub fn render(grid: &VoxelGrid, opts: RenderOptions) -> Result<Image> {
    let Some((min, max)) = grid.bounding_box() else {
        bail!("nothing to render: the scene is empty");
    };
    let cam = camera(min.as_vec3(), (max + 1).as_vec3(), opts.view, opts.scale);
    let upper = cam.size + 2.0 * opts.scale as f32;
    if upper.max_element() > MAX_DIMENSION as f32 || upper.x as u64 * upper.y as u64 > MAX_PIXELS {
        bail!(
            "image would be about {}×{} px, over the {MAX_DIMENSION} px side or {} megapixel limit; lower --scale",
            upper.x,
            upper.y,
            MAX_PIXELS >> 20,
        );
    }
    let quads = collect_cell_quads(grid, cam.eye);
    let mut projected = project_quads(&quads, cam.view, cam.view_proj, cam.size);
    let size = crop(&mut projected, opts.scale as f32);
    let (width, height) = (size.x as u32, size.y as u32);
    Ok(rasterize(&projected, width, height, opts.background))
}

/// Shifts quads by whole pixels so their bounds start `margin` px from the
/// top-left, and returns the image size that leaves `margin` on every side.
fn crop(quads: &mut [ProjectedQuad], margin: f32) -> Vec2 {
    let mut min = Vec2::splat(f32::INFINITY);
    let mut max = Vec2::splat(f32::NEG_INFINITY);
    for p in quads.iter().flat_map(|q| q.pts) {
        min = min.min(p);
        max = max.max(p);
    }
    let offset = (min - margin).floor();
    for q in quads.iter_mut() {
        q.pts = q.pts.map(|p| p - offset);
    }
    (max - offset + margin).ceil()
}

/// Fills each quad's pixels whose centers it covers, keeping the nearest
/// surface per pixel.
pub fn rasterize(quads: &[ProjectedQuad], width: u32, height: u32, bg: [u8; 4]) -> Image {
    let (w, h) = (width as usize, height as usize);
    let mut rgba = bg.repeat(w * h);
    let mut depth = vec![f32::INFINITY; w * h];
    for q in quads {
        let rgb = shade_color(q.color, FACES[q.face_idx].normal);
        let color = [rgb[0], rgb[1], rgb[2], 255];
        let dist = q.depths.map(|z| -z);
        for [a, b, c] in [[0, 1, 2], [0, 2, 3]] {
            fill_triangle(
                [q.pts[a], q.pts[b], q.pts[c]],
                [dist[a], dist[b], dist[c]],
                (w, h),
                |i, d| {
                    if d < depth[i] {
                        depth[i] = d;
                        rgba[i * 4..i * 4 + 4].copy_from_slice(&color);
                    }
                },
            );
        }
    }
    Image {
        width,
        height,
        rgba,
    }
}

fn fill_triangle(
    p: [Vec2; 3],
    d: [f32; 3],
    (w, h): (usize, usize),
    mut plot: impl FnMut(usize, f32),
) {
    let edge = |a: Vec2, b: Vec2, c: Vec2| (b - a).perp_dot(c - a);
    let area = edge(p[0], p[1], p[2]);
    if area.abs() < f32::EPSILON {
        return;
    }
    let lo = p[0].min(p[1]).min(p[2]).floor().max(Vec2::ZERO);
    let hi = p[0].max(p[1]).max(p[2]).ceil();
    let (x1, y1) = ((hi.x as usize).min(w), (hi.y as usize).min(h));
    for y in lo.y as usize..y1 {
        for x in lo.x as usize..x1 {
            let c = Vec2::new(x as f32 + 0.5, y as f32 + 0.5);
            let w0 = edge(p[1], p[2], c) / area;
            let w1 = edge(p[2], p[0], c) / area;
            let w2 = edge(p[0], p[1], c) / area;
            if w0 < 0.0 || w1 < 0.0 || w2 < 0.0 {
                continue;
            }
            plot(y * w + x, w0 * d[0] + w1 * d[1] + w2 * d[2]);
        }
    }
}

pub fn write_png(path: &Path, img: &Image) -> Result<()> {
    let mut bytes = Vec::new();
    image::write_buffer_with_format(
        &mut std::io::Cursor::new(&mut bytes),
        &img.rgba,
        img.width,
        img.height,
        image::ExtendedColorType::Rgba8,
        image::ImageFormat::Png,
    )?;
    roxel::io::atomic::write(path, bytes)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::IVec3;

    fn opts(view: u8) -> RenderOptions {
        RenderOptions {
            view,
            ..Default::default()
        }
    }

    fn opaque_pixels(img: &Image) -> Vec<[u8; 4]> {
        img.rgba
            .chunks_exact(4)
            .filter(|p| p[3] != 0)
            .map(|p| p.try_into().unwrap())
            .collect()
    }

    #[test]
    fn view_zero_looks_down_the_main_diagonal() {
        let d = view_dir(0);
        let k = 1.0 / 3f32.sqrt();
        assert!((d - Vec3::splat(k)).length() < 1e-5, "{d:?}");
    }

    #[test]
    fn views_step_a_quarter_turn_around_y() {
        for v in 0..VIEWS {
            let a = view_dir(v);
            let b = view_dir(v + 1);
            assert!((a.y - b.y).abs() < 1e-6);
            assert!(Vec2::new(a.x, a.z).dot(Vec2::new(b.x, b.z)).abs() < 1e-6);
        }
        assert_eq!(view_dir(4), view_dir(0));
    }

    #[test]
    fn empty_grid_errors() {
        let err = render(&VoxelGrid::default(), opts(0)).err().unwrap();
        assert!(err.to_string().contains("empty"));
    }

    #[test]
    fn single_voxel_shows_three_shaded_faces() {
        let mut g = VoxelGrid::default();
        g.set(IVec3::ZERO, Some([200, 100, 50, 255]));
        let img = render(&g, opts(0)).unwrap();
        let mut shades: Vec<[u8; 4]> = opaque_pixels(&img);
        shades.sort();
        shades.dedup();
        let expected: Vec<[u8; 4]> = [0, 2, 4]
            .map(|f| {
                let c = shade_color([200, 100, 50, 255], FACES[f].normal);
                [c[0], c[1], c[2], 255]
            })
            .into_iter()
            .collect();
        for e in &expected {
            assert!(shades.contains(e), "missing face shade {e:?} in {shades:?}");
        }
        assert_eq!(shades.len(), 3, "{shades:?}");
    }

    #[test]
    fn image_size_scales_with_scale() {
        let mut g = VoxelGrid::default();
        g.set(IVec3::ZERO, Some([255; 4]));
        let small = render(
            &g,
            RenderOptions {
                scale: 4,
                ..opts(0)
            },
        )
        .unwrap();
        let big = render(
            &g,
            RenderOptions {
                scale: 8,
                ..opts(0)
            },
        )
        .unwrap();
        assert!(big.width > small.width && big.height > small.height);
        assert!(opaque_pixels(&big).len() > 3 * opaque_pixels(&small).len());
    }

    #[test]
    fn margin_keeps_the_border_background() {
        let mut g = VoxelGrid::default();
        g.set(IVec3::ZERO, Some([255; 4]));
        let bg = [1, 2, 3, 255];
        let img = render(
            &g,
            RenderOptions {
                background: bg,
                ..opts(0)
            },
        )
        .unwrap();
        for x in 0..img.width {
            assert_eq!(img.pixel(x, 0), bg);
            assert_eq!(img.pixel(x, img.height - 1), bg);
        }
        for y in 0..img.height {
            assert_eq!(img.pixel(0, y), bg);
            assert_eq!(img.pixel(img.width - 1, y), bg);
        }
    }

    #[test]
    fn crops_to_the_drawn_faces_plus_margin() {
        let mut g = VoxelGrid::default();
        g.set(IVec3::ZERO, Some([255; 4]));
        g.set(IVec3::new(0, 20, 0), Some([255; 4]));
        let scale = 8;
        let img = render(&g, RenderOptions { scale, ..opts(0) }).unwrap();
        let opaque: Vec<(u32, u32)> = (0..img.height)
            .flat_map(|y| (0..img.width).map(move |x| (x, y)))
            .filter(|&(x, y)| img.pixel(x, y)[3] != 0)
            .collect();
        let near = |v: u32| (scale..=scale + 1).contains(&v);
        assert!(near(opaque.iter().map(|p| p.0).min().unwrap()));
        assert!(near(opaque.iter().map(|p| p.1).min().unwrap()));
        assert!(near(
            img.width - 1 - opaque.iter().map(|p| p.0).max().unwrap()
        ));
        assert!(near(
            img.height - 1 - opaque.iter().map(|p| p.1).max().unwrap()
        ));
    }

    #[test]
    fn refuses_images_over_the_size_limits_before_rasterizing() {
        let mut g = VoxelGrid::default();
        g.set(IVec3::ZERO, Some([255; 4]));
        g.set(IVec3::new(2000, 0, 0), Some([255; 4]));
        let err = render(
            &g,
            RenderOptions {
                scale: 16,
                ..opts(0)
            },
        )
        .err()
        .unwrap();
        assert!(err.to_string().contains("lower --scale"), "{err}");

        let mut g = VoxelGrid::default();
        g.set(IVec3::ZERO, Some([255; 4]));
        g.set(IVec3::new(60, 60, 60), Some([255; 4]));
        let err = render(
            &g,
            RenderOptions {
                scale: 256,
                ..opts(0)
            },
        )
        .err()
        .unwrap();
        assert!(err.to_string().contains("megapixel"), "{err}");
    }

    #[test]
    fn voxel_alpha_is_ignored() {
        let mut g = VoxelGrid::default();
        g.set(IVec3::ZERO, Some([255, 0, 0, 255]));
        g.set(IVec3::ONE, Some([0, 0, 255, 0]));
        let img = render(&g, opts(0)).unwrap();
        assert!(opaque_pixels(&img).iter().all(|p| p[3] == 255));
        assert!(count_color(&img, [0, 0, 255]) > 0);
        assert_eq!(count_color(&img, [255, 0, 0]), 0);
    }

    #[test]
    fn flat_surfaces_have_no_pinholes() {
        let mut g = VoxelGrid::default();
        for x in 0..24 {
            for z in 0..24 {
                g.set(IVec3::new(x, 0, z), Some([255; 4]));
            }
        }
        for scale in [1, 3, 5, 7, 8] {
            for view in 0..VIEWS {
                let img = render(
                    &g,
                    RenderOptions {
                        scale,
                        ..opts(view)
                    },
                )
                .unwrap();
                let mut holes = 0;
                for y in 1..img.height - 1 {
                    for x in 1..img.width - 1 {
                        let around = [(x - 1, y), (x + 1, y), (x, y - 1), (x, y + 1)];
                        if img.pixel(x, y)[3] == 0
                            && around.iter().all(|&(a, b)| img.pixel(a, b)[3] != 0)
                        {
                            holes += 1;
                        }
                    }
                }
                assert_eq!(holes, 0, "scale {scale} view {view}");
            }
        }
    }

    fn count_color(img: &Image, rgb: [u8; 3]) -> usize {
        opaque_pixels(img)
            .iter()
            .filter(|p| {
                FACES
                    .iter()
                    .any(|f| p[..3] == shade_color([rgb[0], rgb[1], rgb[2], 255], f.normal))
            })
            .count()
    }

    #[test]
    fn nearer_voxel_hides_the_one_behind_it() {
        // (1,1,1) lies straight toward the view-0 camera from the origin, so it
        // projects onto exactly the same hexagon and hides it completely.
        let mut g = VoxelGrid::default();
        g.set(IVec3::ZERO, Some([255, 0, 0, 255]));
        g.set(IVec3::ONE, Some([0, 0, 255, 255]));
        let front = render(&g, opts(0)).unwrap();
        assert_eq!(count_color(&front, [255, 0, 0]), 0);
        assert!(count_color(&front, [0, 0, 255]) > 0);

        let side = render(&g, opts(2)).unwrap();
        assert!(count_color(&side, [255, 0, 0]) > 0);
        assert!(count_color(&side, [0, 0, 255]) > 0);
    }

    fn square(depth: f32, color: [u8; 4]) -> ProjectedQuad {
        ProjectedQuad {
            pts: [
                Vec2::new(0.0, 0.0),
                Vec2::new(4.0, 0.0),
                Vec2::new(4.0, 4.0),
                Vec2::new(0.0, 4.0),
            ],
            cell_view_z: depth,
            depths: [depth; 4],
            color,
            face_idx: 2,
        }
    }

    #[test]
    fn depth_test_beats_draw_order() {
        let blue = [0, 0, 255, 255];
        let red = [255, 0, 0, 255];
        let shaded = shade_color(blue, FACES[2].normal);
        for quads in [
            [square(-1.0, blue), square(-5.0, red)],
            [square(-5.0, red), square(-1.0, blue)],
        ] {
            let img = rasterize(&quads, 4, 4, [0; 4]);
            for y in 0..4 {
                for x in 0..4 {
                    assert_eq!(img.pixel(x, y), [shaded[0], shaded[1], shaded[2], 255]);
                }
            }
        }
    }

    #[test]
    fn writes_a_decodable_png() {
        let mut g = VoxelGrid::default();
        g.set(IVec3::new(0, 0, 0), Some([10, 20, 30, 255]));
        let img = render(&g, opts(1)).unwrap();
        let path = std::env::temp_dir().join(format!("rox-render-{}.png", std::process::id()));
        write_png(&path, &img).unwrap();
        let decoded = image::open(&path).unwrap().to_rgba8();
        let _ = std::fs::remove_file(&path);
        assert_eq!(decoded.dimensions(), (img.width, img.height));
        assert_eq!(decoded.as_raw(), &img.rgba);
    }
}
