// Social-media "shot" export: a polished, framed PNG of the model — a
// hero-color background, real directional lighting for face shading, a real
// cast shadow tied to the scene, a subtle vignette, and a contrast-tinted
// roxel wordmark watermark.
//
// Unlike `snapshot.rs` (which captures the *live* unlit editor scene at the
// user's current camera with a transparent background), the shot builds a
// throwaway scene on its own render layer: a lit copy of the model mesh, a lit
// ground plane (albedo = background, exposure-calibrated so it renders ≈ the
// background while the model's real shadow darkens it), a shadow-casting key
// light + a fill light, and a dedicated orthographic camera framed
// isometrically. Everything is despawned after the capture. The captured image
// then runs a CPU post pass (vignette + watermark; gradient/dither are the
// next phase) before it is written to disk.
//
// Render race: newly spawned meshes/lights must be extracted before the
// screenshot reads back, so the system warms up for a couple of frames between
// building the scene and requesting the capture.

use std::path::PathBuf;

use bevy::asset::RenderAssetUsages;
use bevy::camera::visibility::RenderLayers;
use bevy::camera::{ClearColorConfig, RenderTarget, ScalingMode};
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::image::Image;
use bevy::light::{CascadeShadowConfigBuilder, ShadowFilteringMethod};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured};

use crate::GridResource;
use crate::mesh::build_lit_mesh;
use crate::ui::Toasts;
use roxel::grid::{Color8, VoxelGrid};

/// Render layer the throwaway shot scene lives on, isolating it from the
/// editor scene (layer 0) and the gizmo overlay (layer 1).
pub const SHOT_LAYER: usize = 2;

/// Output resolution (4:3 landscape, matching the reference dribbble look).
pub const SHOT_WIDTH: u32 = 2000;
pub const SHOT_HEIGHT: u32 = 1500;

/// Fraction of empty margin around the framed model.
const FRAME_MARGIN: f32 = 0.18;

/// Isometric view direction (camera sits at focus + this).
fn iso_dir() -> Vec3 {
    Vec3::new(1.0, 0.85, 1.0).normalize()
}

/// Frames to wait between spawning the scene and requesting the screenshot,
/// so the new meshes/lights are extracted into the render world first.
const WARMUP_FRAMES: u32 = 2;

/// White wordmark silhouette (alpha = coverage); tinted at composite time.
const WORDMARK_PNG: &[u8] = include_bytes!("../assets/branding/roxel-wordmark.png");

/// Albedo saturation for the lit model. 1.0 = full palette color (the voxels
/// render at their true saturation, matching the editor).
const SHOT_SATURATION: f32 = 1.0;

/// Key light (casts the real shadow) and front fill, in lux. Calibrated under
/// `Tonemapping::None` + the global ambient so a fully-lit up-facing surface
/// (the ground) renders ≈ its albedo (the background color) rather than
/// clipping to white, while shadowed ground darkens to a colored shadow.
const KEY_LUX: f32 = 2_600.0;
const FILL_LUX: f32 = 700.0;

#[derive(Resource, Default)]
pub struct ShotRequest(pub Option<PathBuf>);

#[derive(Default)]
enum ShotPhase {
    #[default]
    Idle,
    Warmup(u32),
    Capturing,
}

#[derive(Resource, Default)]
pub struct ShotSession {
    phase: ShotPhase,
    path: Option<PathBuf>,
    image: Option<Handle<Image>>,
    entities: Vec<Entity>,
    meshes: Vec<Handle<Mesh>>,
    materials: Vec<Handle<StandardMaterial>>,
    textures: Vec<Handle<Image>>,
}

impl ShotSession {
    fn reset(&mut self) {
        self.phase = ShotPhase::Idle;
        self.path = None;
        self.image = None;
        self.entities.clear();
        self.meshes.clear();
        self.materials.clear();
        self.textures.clear();
    }
}

#[allow(clippy::too_many_arguments)]
pub fn shot_system(
    mut commands: Commands,
    mut request: ResMut<ShotRequest>,
    mut session: ResMut<ShotSession>,
    grid: Res<GridResource>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut toasts: ResMut<Toasts>,
) {
    match session.phase {
        ShotPhase::Idle => {
            let Some(path) = request.0.take() else {
                return;
            };
            let Some((min, max)) = grid.bounding_box() else {
                toasts.error("Nothing to export — the scene is empty");
                return;
            };
            build_scene(
                &mut commands,
                &mut session,
                &mut meshes,
                &mut materials,
                &mut images,
                &grid,
                min,
                max,
            );
            session.path = Some(path);
            session.phase = ShotPhase::Warmup(WARMUP_FRAMES);
        }
        ShotPhase::Warmup(n) => {
            if n > 0 {
                session.phase = ShotPhase::Warmup(n - 1);
                return;
            }
            let Some(handle) = session.image.clone() else {
                session.reset();
                return;
            };
            commands
                .spawn(Screenshot::image(handle))
                .observe(on_shot_captured);
            session.phase = ShotPhase::Capturing;
        }
        ShotPhase::Capturing => {}
    }
}

#[allow(clippy::too_many_arguments)]
fn build_scene(
    commands: &mut Commands,
    session: &mut ShotSession,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
    grid: &VoxelGrid,
    min: IVec3,
    max: IVec3,
) {
    let layer = RenderLayers::layer(SHOT_LAYER);

    // Background derived from the model's hero color (most prominent vivid
    // hue) so a multicolor scene gets real color, not a muddy average. No
    // auto-plinth — the creator builds their own base.
    let cells: Vec<(IVec3, Color8)> = grid.iter_occupied().collect();
    let bg = auto_background(hero_color(&cells));

    // World-space AABB of the model: voxel p occupies [p, p+1].
    let model_min = min.as_vec3();
    let model_max = (max + IVec3::ONE).as_vec3();

    // ---- Model (lit albedo mesh) ----
    let model_mesh = meshes.add(build_lit_mesh(grid, SHOT_SATURATION));
    let model_mat = materials.add(matte_material(Color::WHITE));
    let model_entity = commands
        .spawn((
            Mesh3d(model_mesh.clone()),
            MeshMaterial3d(model_mat.clone()),
            Transform::IDENTITY,
            layer.clone(),
        ))
        .id();
    session.meshes.push(model_mesh);
    session.materials.push(model_mat);
    session.entities.push(model_entity);

    // Ground / shadow / framing all sit at the model's bottom.
    let base_y = model_min.y;

    let scene_size = (model_max - model_min).length().max(8.0);

    // ---- Ground plane (lit, receives the real cast shadow) ----
    // Albedo = background. Lighting (below) is calibrated so the fully-lit
    // ground renders ≈ the background color, while the model's real cast shadow
    // darkens it — a shadow genuinely tied to the scene, not a faked quad.
    let ground_half = scene_size * 12.0;
    let ground_mesh = meshes.add(Mesh::from(Plane3d::new(Vec3::Y, Vec2::splat(ground_half))));
    let ground_mat = materials.add(matte_material(color_from_rgb(bg)));
    let cx = (model_min.x + model_max.x) * 0.5;
    let cz = (model_min.z + model_max.z) * 0.5;
    let ground_entity = commands
        .spawn((
            Mesh3d(ground_mesh.clone()),
            MeshMaterial3d(ground_mat.clone()),
            Transform::from_xyz(cx, base_y, cz),
            layer.clone(),
        ))
        .id();
    session.meshes.push(ground_mesh);
    session.materials.push(ground_mat);
    session.entities.push(ground_entity);

    // ---- Lighting + real cast shadow ----
    // Key from upper back-right so the camera-facing front-left face is the
    // shaded side and the shadow falls front-left (the reference look).
    let key_dir = Vec3::new(-0.5, -1.0, 0.32).normalize();
    let cascade = CascadeShadowConfigBuilder {
        num_cascades: 1,
        maximum_distance: scene_size * 4.0 + 50.0,
        minimum_distance: 0.05,
        ..default()
    }
    .build();
    let key = commands
        .spawn((
            DirectionalLight {
                illuminance: KEY_LUX,
                shadows_enabled: true,
                ..default()
            },
            Transform::IDENTITY.looking_to(key_dir, Vec3::Y),
            cascade,
            layer.clone(),
        ))
        .id();
    session.entities.push(key);
    // Soft front fill, no shadow, so the shaded faces don't crush to black.
    // Kept low so it doesn't wash out the cast shadow on the ground.
    let fill = commands
        .spawn((
            DirectionalLight {
                illuminance: FILL_LUX,
                shadows_enabled: false,
                ..default()
            },
            Transform::IDENTITY.looking_to(Vec3::new(0.4, -0.5, -0.4).normalize(), Vec3::Y),
            layer.clone(),
        ))
        .id();
    session.entities.push(fill);

    // ---- Soft AO contact shadow ----
    // A very soft, colored dark pool pooled directly under the footprint, on
    // top of the (real) cast shadow, to deepen where the model meets the
    // ground. Centered (no offset) so it grounds the base on every side.
    let sx = model_max.x - model_min.x;
    let sz = model_max.z - model_min.z;
    let contact_tex = images.add(soft_blob_image(256));
    // Quad scaled so the solid footprint fills CONTACT_INNER of it; the halo
    // then feathers out into the surrounding open ground.
    let contact_scale = 1.0 / CONTACT_INNER;
    let contact_mesh = meshes.add(Mesh::from(Plane3d::new(
        Vec3::Y,
        Vec2::new(sx * contact_scale * 0.5, sz * contact_scale * 0.5),
    )));
    let cs = shadow_color(bg, 0.6);
    let contact_mat = materials.add(StandardMaterial {
        base_color: Color::srgba(cs[0], cs[1], cs[2], CONTACT_OPACITY),
        base_color_texture: Some(contact_tex.clone()),
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        ..default()
    });
    let contact = commands
        .spawn((
            Mesh3d(contact_mesh.clone()),
            MeshMaterial3d(contact_mat.clone()),
            Transform::from_xyz(cx, base_y + 0.015, cz),
            layer.clone(),
        ))
        .id();
    session.meshes.push(contact_mesh);
    session.materials.push(contact_mat);
    session.textures.push(contact_tex);
    session.entities.push(contact);

    // ---- Camera ----
    let union_min = Vec3::new(model_min.x, base_y, model_min.z);
    let union_max = model_max;
    let focus = (union_min + union_max) * 0.5;
    let corners = aabb_corners(union_min, union_max);
    let look = Transform::from_translation(focus + iso_dir()).looking_at(focus, Vec3::Y);
    let aspect = SHOT_WIDTH as f32 / SHOT_HEIGHT as f32;
    let view_h = fit_ortho_height(&corners, look.rotation, aspect, FRAME_MARGIN);
    let dist = scene_size * 3.0 + 50.0;

    let image =
        Image::new_target_texture(SHOT_WIDTH, SHOT_HEIGHT, TextureFormat::Rgba8UnormSrgb, None);
    let handle = images.add(image);

    let camera = commands
        .spawn((
            Camera3d::default(),
            Camera {
                clear_color: ClearColorConfig::Custom(color_from_rgb(bg)),
                order: 20,
                ..default()
            },
            RenderTarget::Image(handle.clone().into()),
            Transform::from_translation(focus + iso_dir() * dist).looking_at(focus, Vec3::Y),
            Projection::Orthographic(OrthographicProjection {
                scaling_mode: ScalingMode::FixedVertical {
                    viewport_height: view_h,
                },
                near: -dist * 4.0,
                far: dist * 4.0,
                ..OrthographicProjection::default_3d()
            }),
            Tonemapping::None,
            ShadowFilteringMethod::Gaussian,
            layer,
        ))
        .id();
    session.image = Some(handle);
    session.entities.push(camera);
}

/// Smoothstep in [edge0, edge1].
fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Peak alpha of the soft AO contact shadow under the base.
const CONTACT_OPACITY: f32 = 0.4;

/// Fraction of the quad half-extent the (solid) footprint occupies; the
/// remainder is the outward feather of the AO halo.
const CONTACT_INNER: f32 = 0.62;

/// Signed distance to an axis-aligned rounded box (half-extents hx,hy, corner
/// radius r), centered at origin. Negative inside.
fn sdf_round_box(px: f32, py: f32, hx: f32, hy: f32, r: f32) -> f32 {
    let qx = px.abs() - (hx - r);
    let qy = py.abs() - (hy - r);
    (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt() + qx.max(qy).min(0.0) - r
}

/// AO contact-halo mask at normalized point (px,py) in [-1,1]: SOLID across the
/// footprint (out to `inner`), then feathers *outward* to zero at the quad edge
/// — so the darkest band hugs the base perimeter and fades into the open
/// ground, rather than peaking (hidden) under the model center.
fn contact_alpha(px: f32, py: f32, inner: f32) -> f32 {
    let corner = inner * 0.4;
    let d = sdf_round_box(px, py, inner, inner, corner);
    let feather = (1.0 - inner).max(1e-3);
    (1.0 - smoothstep(0.0, feather, d)).clamp(0.0, 1.0)
}

/// Colored shadow tint: the background pushed *more* saturated and darkened, so
/// the shadow reads as a real colored shadow (darker green on green), not grey.
fn shadow_color(bg: [u8; 3], darken: f32) -> [f32; 3] {
    let l = 0.2126 * bg[0] as f32 + 0.7152 * bg[1] as f32 + 0.0722 * bg[2] as f32;
    let mut out = [0.0f32; 3];
    for (i, o) in out.iter_mut().enumerate() {
        let saturated = l + (bg[i] as f32 - l) * 1.5;
        *o = (saturated * darken / 255.0).clamp(0.0, 1.0);
    }
    out
}

/// White RGBA texture whose alpha is the [`contact_alpha`] outward-feather halo.
fn soft_blob_image(size: u32) -> Image {
    let mut data = vec![0u8; (size * size * 4) as usize];
    let c = (size as f32 - 1.0) * 0.5;
    for y in 0..size {
        for x in 0..size {
            let dx = (x as f32 - c) / c;
            let dy = (y as f32 - c) / c;
            let a = contact_alpha(dx, dy, CONTACT_INNER);
            let i = ((y * size + x) * 4) as usize;
            data[i] = 255;
            data[i + 1] = 255;
            data[i + 2] = 255;
            data[i + 3] = (a * 255.0).round() as u8;
        }
    }
    Image::new(
        Extent3d {
            width: size,
            height: size,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    )
}

fn matte_material(base: Color) -> StandardMaterial {
    StandardMaterial {
        base_color: base,
        perceptual_roughness: 1.0,
        metallic: 0.0,
        reflectance: 0.1,
        ..default()
    }
}

fn color_from_rgb(rgb: [u8; 3]) -> Color {
    Color::srgb_u8(rgb[0], rgb[1], rgb[2])
}

fn on_shot_captured(
    trigger: On<ScreenshotCaptured>,
    mut commands: Commands,
    mut session: ResMut<ShotSession>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut toasts: ResMut<Toasts>,
) {
    if let Some(path) = session.path.clone() {
        match finish_shot(&trigger.image, &path) {
            Ok(()) => {
                let label = path
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("shot.png");
                toasts.success(format!("Exported {label}"));
            }
            Err(e) => toasts.error(format!("Shot export failed: {e}")),
        }
    }

    // Tear down the throwaway scene + its assets.
    for e in session.entities.drain(..) {
        commands.entity(e).despawn();
    }
    for h in session.meshes.drain(..) {
        meshes.remove(&h);
    }
    for h in session.materials.drain(..) {
        materials.remove(&h);
    }
    for h in session.textures.drain(..) {
        images.remove(&h);
    }
    if let Some(h) = session.image.take() {
        images.remove(&h);
    }
    session.reset();
}

/// Decode the captured image, run the CPU post pass, and write the PNG.
fn finish_shot(img: &Image, path: &std::path::Path) -> anyhow::Result<()> {
    let dyn_img = img
        .clone()
        .try_into_dynamic()
        .map_err(|e| anyhow::anyhow!("image not convertible: {e:?}"))?;
    let mut rgba = dyn_img.into_rgba8();
    apply_vignette(&mut rgba);
    apply_watermark(&mut rgba);
    rgba.save_with_format(path, image::ImageFormat::Png)?;
    Ok(())
}

/// Subtle vignette darkening toward the corners.
const VIGNETTE_STRENGTH: f32 = 0.16;
const VIGNETTE_INNER: f32 = 0.65;
const VIGNETTE_OUTER: f32 = 1.35;

/// Vignette multiplier at a point whose distance from center (normalized so the
/// half-width/height = 1) is `dist`. 1.0 in the center, dipping toward the
/// corners.
fn vignette_factor(dist: f32) -> f32 {
    1.0 - VIGNETTE_STRENGTH * smoothstep(VIGNETTE_INNER, VIGNETTE_OUTER, dist)
}

/// Multiply each pixel's RGB by [`vignette_factor`] of its distance from center.
fn apply_vignette(img: &mut image::RgbaImage) {
    let (w, h) = img.dimensions();
    if w == 0 || h == 0 {
        return;
    }
    let cx = (w as f32 - 1.0) * 0.5;
    let cy = (h as f32 - 1.0) * 0.5;
    let (hx, hy) = (w as f32 * 0.5, h as f32 * 0.5);
    for y in 0..h {
        for x in 0..w {
            let nx = (x as f32 - cx) / hx;
            let ny = (y as f32 - cy) / hy;
            let f = vignette_factor((nx * nx + ny * ny).sqrt());
            let p = img.get_pixel_mut(x, y);
            for c in p.0.iter_mut().take(3) {
                *c = (*c as f32 * f).round().clamp(0.0, 255.0) as u8;
            }
        }
    }
}

/// Composite the roxel wordmark into the bottom-right corner, tinted white or
/// black depending on the local background contrast.
fn apply_watermark(img: &mut image::RgbaImage) {
    let (w, h) = img.dimensions();
    let Ok(mark) = image::load_from_memory(WORDMARK_PNG) else {
        return;
    };
    let mark = mark.into_rgba8();
    let (mw0, mh0) = mark.dimensions();
    if mw0 == 0 || mh0 == 0 {
        return;
    }

    // Target watermark width = 16% of the image; keep aspect.
    let target_w = (w as f32 * 0.16).round().max(1.0) as u32;
    let target_h = ((target_w as f32) * (mh0 as f32) / (mw0 as f32))
        .round()
        .max(1.0) as u32;
    let mark = image::imageops::resize(
        &mark,
        target_w,
        target_h,
        image::imageops::FilterType::Lanczos3,
    );

    let margin = (w as f32 * 0.03).round() as u32;
    let ox = w.saturating_sub(target_w + margin);
    let oy = h.saturating_sub(target_h + margin);

    let luma = region_avg_luma(img, ox, oy, target_w, target_h);
    let tint = pick_watermark_tint(luma);
    composite_tinted(img, &mark, ox, oy, tint, WATERMARK_OPACITY);
}

const WATERMARK_OPACITY: f32 = 0.3;

// ---------------------------------------------------------------------------
// Pure helpers (unit-tested)
// ---------------------------------------------------------------------------

/// Relative luminance of an sRGB color in 0..1 (perceptual weights, no gamma
/// decode — good enough for a light/dark contrast decision).
pub fn relative_luma(rgb: [u8; 3]) -> f32 {
    (0.2126 * rgb[0] as f32 + 0.7152 * rgb[1] as f32 + 0.0722 * rgb[2] as f32) / 255.0
}

/// Black on light backgrounds, white on dark — maximizing watermark contrast.
pub fn pick_watermark_tint(bg_luma: f32) -> [u8; 3] {
    if bg_luma > 0.5 {
        [0, 0, 0]
    } else {
        [255, 255, 255]
    }
}

/// Mean luminance of a rectangular region of the image.
fn region_avg_luma(img: &image::RgbaImage, x: u32, y: u32, w: u32, h: u32) -> f32 {
    let (iw, ih) = img.dimensions();
    let mut sum = 0.0f32;
    let mut n = 0u32;
    for yy in y..(y + h).min(ih) {
        for xx in x..(x + w).min(iw) {
            let p = img.get_pixel(xx, yy).0;
            sum += relative_luma([p[0], p[1], p[2]]);
            n += 1;
        }
    }
    if n == 0 { 0.5 } else { sum / n as f32 }
}

/// Alpha-blend a tinted version of `mark` (using its alpha as coverage) onto
/// `dst` at (ox, oy), scaled by `opacity`.
fn composite_tinted(
    dst: &mut image::RgbaImage,
    mark: &image::RgbaImage,
    ox: u32,
    oy: u32,
    tint: [u8; 3],
    opacity: f32,
) {
    let (dw, dh) = dst.dimensions();
    let (mw, mh) = mark.dimensions();
    for my in 0..mh {
        let dy = oy + my;
        if dy >= dh {
            break;
        }
        for mx in 0..mw {
            let dx = ox + mx;
            if dx >= dw {
                break;
            }
            let a = (mark.get_pixel(mx, my).0[3] as f32 / 255.0) * opacity;
            if a <= 0.0 {
                continue;
            }
            let p = dst.get_pixel_mut(dx, dy);
            for (c, &t) in tint.iter().enumerate() {
                p.0[c] = (p.0[c] as f32 * (1.0 - a) + t as f32 * a).round() as u8;
            }
        }
    }
}

/// Mean sRGB color of all occupied voxels (rounded per channel).
fn average_color(cells: &[(IVec3, Color8)]) -> [u8; 3] {
    if cells.is_empty() {
        return [128, 128, 128];
    }
    let mut acc = [0u64; 3];
    for (_, c) in cells {
        acc[0] += c[0] as u64;
        acc[1] += c[1] as u64;
        acc[2] += c[2] as u64;
    }
    let n = cells.len() as u64;
    [(acc[0] / n) as u8, (acc[1] / n) as u8, (acc[2] / n) as u8]
}

/// Saturation of an sRGB color in 0..1 (HSV S: chroma over max channel).
fn color_saturation(c: [u8; 3]) -> f32 {
    let mx = c[0].max(c[1]).max(c[2]) as f32;
    let mn = c[0].min(c[1]).min(c[2]) as f32;
    if mx <= 0.0 { 0.0 } else { (mx - mn) / mx }
}

/// The model's "hero" color: the distinct voxel color maximizing frequency ×
/// saturation, so the most prominent *vivid* color wins (grass green over a
/// duller but common dirt). Falls back to the average for a fully grey model.
fn hero_color(cells: &[(IVec3, Color8)]) -> [u8; 3] {
    use std::collections::HashMap;
    let mut counts: HashMap<[u8; 3], u32> = HashMap::new();
    for (_, c) in cells {
        *counts.entry([c[0], c[1], c[2]]).or_insert(0) += 1;
    }
    // Weight saturation (squared) AND brightness (cubed) so a vivid, bright,
    // common color (grass/water) wins over a dark-but-common one (the dirt
    // base) and over a few tiny ultra-saturated specks (berries).
    let score = |c: [u8; 3], n: u32| {
        let s = color_saturation(c);
        let v = c[0].max(c[1]).max(c[2]) as f32 / 255.0;
        n as f32 * s * s * v * v * v
    };
    counts
        .iter()
        .max_by(|a, b| {
            score(*a.0, *a.1)
                .partial_cmp(&score(*b.0, *b.1))
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .filter(|(c, _)| color_saturation(**c) > 0.0)
        .map(|(c, _)| *c)
        .unwrap_or_else(|| average_color(cells))
}

/// Light pastel of the model's hero hue — a clear, colorful background that
/// follows the palette instead of a muddy average or warm-cream tan.
pub fn auto_background(avg: [u8; 3]) -> [u8; 3] {
    // 44% model color + 56% near-white, per channel, clamped to a light band.
    // Keeping more of the model's color (and a neutral white target, not a tan
    // base) gives the background real hue without going brown.
    let mix = |ch: u8| (ch as f32 * 0.44 + 250.0 * 0.56).clamp(188.0, 250.0) as u8;
    [mix(avg[0]), mix(avg[1]), mix(avg[2])]
}

/// The 8 corners of a world-space AABB.
fn aabb_corners(min: Vec3, max: Vec3) -> [Vec3; 8] {
    [
        Vec3::new(min.x, min.y, min.z),
        Vec3::new(max.x, min.y, min.z),
        Vec3::new(min.x, max.y, min.z),
        Vec3::new(max.x, max.y, min.z),
        Vec3::new(min.x, min.y, max.z),
        Vec3::new(max.x, min.y, max.z),
        Vec3::new(min.x, max.y, max.z),
        Vec3::new(max.x, max.y, max.z),
    ]
}

/// Orthographic viewport height that frames `corners` (with margin) for the
/// given camera rotation and output aspect ratio. The camera looks down its
/// local -Z, so projecting corners into camera space and taking the X/Y extent
/// gives the on-screen footprint.
pub fn fit_ortho_height(corners: &[Vec3], cam_rot: Quat, aspect: f32, margin: f32) -> f32 {
    let inv = cam_rot.inverse();
    let mut min = Vec2::splat(f32::MAX);
    let mut max = Vec2::splat(f32::MIN);
    for &c in corners {
        let v = inv * c;
        let xy = Vec2::new(v.x, v.y);
        min = min.min(xy);
        max = max.max(xy);
    }
    let ext = max - min;
    let needed = ext.y.max(ext.x / aspect.max(1e-3));
    (needed * (1.0 + margin)).max(1e-3)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_luma_endpoints() {
        assert!(relative_luma([0, 0, 0]) < 1e-6);
        assert!((relative_luma([255, 255, 255]) - 1.0).abs() < 1e-4);
        assert!(relative_luma([0, 255, 0]) > relative_luma([0, 0, 255]));
    }

    #[test]
    fn watermark_tint_follows_contrast() {
        assert_eq!(pick_watermark_tint(0.9), [0, 0, 0]); // light bg -> dark mark
        assert_eq!(pick_watermark_tint(0.1), [255, 255, 255]); // dark bg -> light mark
    }

    #[test]
    fn auto_background_is_light_and_carries_hue() {
        for avg in [[0, 0, 0], [128, 128, 128], [255, 255, 255], [12, 240, 60]] {
            let bg = auto_background(avg);
            for ch in bg {
                assert!((188..=250).contains(&ch), "bg channel out of range: {ch}");
            }
            assert!(relative_luma(bg) > 0.7, "background should be light");
        }
        // A greener model yields a background whose green leads red/blue.
        let bg = auto_background([40, 200, 40]);
        assert!(
            bg[1] > bg[0] && bg[1] > bg[2],
            "bg should carry hue: {bg:?}"
        );
    }

    #[test]
    fn contact_alpha_solid_to_footprint_then_feathers_out() {
        let inner = 0.62;
        // Solid across the footprint (center and inside the inner box).
        assert!((contact_alpha(0.0, 0.0, inner) - 1.0).abs() < 1e-6);
        assert!(contact_alpha(0.3, 0.3, inner) > 0.99);
        // Feathers in the outer ring, zero at the quad edge.
        let ring = contact_alpha(0.8, 0.0, inner);
        assert!(ring > 0.0 && ring < 1.0, "ring={ring}");
        assert_eq!(contact_alpha(1.0, 1.0, inner), 0.0);
    }

    #[test]
    fn shadow_color_is_darker_and_more_saturated() {
        let bg = [188, 228, 188];
        let s = shadow_color(bg, 0.6);
        for (i, &c) in s.iter().enumerate() {
            assert!(c < bg[i] as f32 / 255.0, "channel {i} should be darker");
        }
        let bg_gap = (bg[1] as f32 - bg[0] as f32) / 255.0;
        assert!(s[1] - s[0] > bg_gap * 0.6, "should keep/boost chroma");
    }

    #[test]
    fn vignette_darkens_corners_not_center() {
        assert!((vignette_factor(0.0) - 1.0).abs() < 1e-6);
        assert!((vignette_factor(0.5) - 1.0).abs() < 1e-6); // inside inner radius
        let corner = vignette_factor(2.0_f32.sqrt()); // image corner
        assert!(corner < 1.0 && corner > 0.7, "corner factor {corner}");
    }

    #[test]
    fn hero_color_prefers_vivid_over_common_dull() {
        // Lots of near-grey dirt + fewer vivid green: green should win.
        let mut cells = Vec::new();
        for _ in 0..100 {
            cells.push((IVec3::ZERO, [90, 85, 80, 255])); // dull, common
        }
        for _ in 0..30 {
            cells.push((IVec3::ONE, [40, 200, 40, 255])); // vivid, rarer
        }
        assert_eq!(hero_color(&cells), [40, 200, 40]);
    }

    #[test]
    fn hero_color_falls_back_to_average_when_all_grey() {
        let cells = vec![
            (IVec3::ZERO, [100, 100, 100, 255]),
            (IVec3::ONE, [200, 200, 200, 255]),
        ];
        assert_eq!(hero_color(&cells), [150, 150, 150]);
    }

    #[test]
    fn average_color_of_two_cells() {
        let cells = vec![
            (IVec3::ZERO, [100, 100, 100, 255]),
            (IVec3::ONE, [200, 200, 200, 255]),
        ];
        assert_eq!(average_color(&cells), [150, 150, 150]);
    }

    #[test]
    fn fit_ortho_height_grows_with_aabb() {
        let rot = Quat::IDENTITY;
        let small = fit_ortho_height(
            &aabb_corners(Vec3::ZERO, Vec3::splat(1.0)),
            rot,
            4.0 / 3.0,
            0.0,
        );
        let big = fit_ortho_height(
            &aabb_corners(Vec3::ZERO, Vec3::splat(4.0)),
            rot,
            4.0 / 3.0,
            0.0,
        );
        assert!(big > small);
    }

    #[test]
    fn fit_ortho_height_respects_aspect_for_wide_box() {
        // A box wider than it is tall must be framed by its width / aspect.
        let rot = Quat::IDENTITY;
        let h = fit_ortho_height(
            &aabb_corners(Vec3::ZERO, Vec3::new(40.0, 2.0, 2.0)),
            rot,
            4.0 / 3.0,
            0.0,
        );
        // width 40 / aspect (1.333) ≈ 30, well above the 2-unit height.
        assert!(h > 25.0, "h={h}");
    }

    #[test]
    fn fit_ortho_height_margin_inflates() {
        let rot = Quat::IDENTITY;
        let corners = aabb_corners(Vec3::ZERO, Vec3::splat(3.0));
        let tight = fit_ortho_height(&corners, rot, 1.0, 0.0);
        let loose = fit_ortho_height(&corners, rot, 1.0, 0.2);
        assert!((loose / tight - 1.2).abs() < 1e-4);
    }

    #[test]
    fn watermark_composite_blends_toward_tint() {
        // 4x1 white mark fully opaque over a black image, 50% opacity -> grey.
        let mut dst = image::RgbaImage::from_pixel(4, 1, image::Rgba([0, 0, 0, 255]));
        let mark = image::RgbaImage::from_pixel(4, 1, image::Rgba([123, 45, 67, 255]));
        composite_tinted(&mut dst, &mark, 0, 0, [255, 255, 255], 0.5);
        for x in 0..4 {
            let p = dst.get_pixel(x, 0).0;
            assert_eq!(p[0], 128);
            assert_eq!(p[1], 128);
            assert_eq!(p[2], 128);
        }
    }

    #[test]
    fn region_avg_luma_of_solid_image() {
        let img = image::RgbaImage::from_pixel(10, 10, image::Rgba([255, 255, 255, 255]));
        assert!((region_avg_luma(&img, 0, 0, 10, 10) - 1.0).abs() < 1e-3);
    }

    #[test]
    fn embedded_wordmark_decodes_with_alpha() {
        let mark = image::load_from_memory(WORDMARK_PNG)
            .expect("wordmark png decodes")
            .into_rgba8();
        let (w, h) = mark.dimensions();
        assert!(w > 0 && h > 0);
        // Some pixels transparent (corners) and some opaque (the glyphs).
        assert!(mark.pixels().any(|p| p.0[3] == 0));
        assert!(mark.pixels().any(|p| p.0[3] > 200));
    }
}
