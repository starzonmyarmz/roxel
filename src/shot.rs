// Social-media "shot" export: a polished, framed PNG of the model — a
// gradient-or-solid background, real directional lighting for face shading, a
// real cast shadow tied to the scene, a subtle vignette, optional gradient
// dithering, and a contrast-tinted roxel wordmark watermark. A modal tweak
// panel (`ui::modals::draw_shot_panel`) drives a live, low-res preview; Export
// re-renders at full resolution.
//
// Unlike `snapshot.rs` (which captures the *live* unlit editor scene at the
// user's current camera with a transparent background), the shot builds a
// throwaway scene split across two render layers and captured by two cameras:
//
//   * SHOT_LAYER (2):   the lit model mesh (`build_lit_mesh`), captured by
//                       camera A over a transparent clear → a clean model
//                       RGBA *with alpha* (no background to color-key against).
//   * GROUND_LAYER (3): a white-albedo ground plane + a neutral AO contact
//                       halo, captured by camera B over a white clear → a
//                       per-pixel *shadow-factor map* (white = fully lit, darker
//                       = in shadow). The model is NOT on this layer, but the
//                       key light is on both layers, so the model's *real* cast
//                       shadow still lands on the ground.
//
// Both cameras share one transform + ortho projection, so the two captures are
// pixel-aligned. The CPU composite (`composite`) then paints the background
// (solid / vertical / radial gradient, optionally dithered) modulated by the
// shadow factor, alpha-composites the model on top, and applies the vignette +
// watermark. Because the ground is white, the *background color and gradient
// live entirely in the composite* — changing them needs no GPU re-render, which
// is what makes the live preview cheap (only saturation / lift / aspect re-render).
//
// Render race: newly spawned meshes/lights must be extracted before the
// screenshots read back, so the system warms up for a couple of frames between
// building the scene and requesting the captures.

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
use bevy_egui::{EguiTextureHandle, EguiUserTextures, egui};

use crate::GridResource;
use crate::mesh::build_lit_mesh;
use crate::ui::Toasts;
use roxel::grid::{Color8, VoxelGrid};

/// Render layer the lit model lives on (camera A → model silhouette + alpha).
pub const SHOT_LAYER: usize = 2;
/// Render layer the white ground + AO halo live on (camera B → shadow factor).
pub const GROUND_LAYER: usize = 3;

/// Long-edge resolution of the live preview render (the export uses the full
/// [`ResPreset`] dimensions). Small enough that re-rendering on a knob change is
/// instant, large enough to judge framing + shading.
const PREVIEW_LONG_EDGE: u32 = 800;

/// Fraction of empty margin around the framed model.
const FRAME_MARGIN: f32 = 0.18;

/// Isometric view direction (camera sits at focus + this).
fn iso_dir() -> Vec3 {
    Vec3::new(1.0, 0.85, 1.0).normalize()
}

/// Frames to wait between spawning the scene and requesting the screenshots,
/// so the new meshes/lights are extracted into the render world first.
const WARMUP_FRAMES: u32 = 2;

/// White wordmark silhouette (alpha = coverage); tinted at composite time.
const WORDMARK_PNG: &[u8] = include_bytes!("../assets/branding/roxel-wordmark.png");

/// Key light (casts the real shadow) and front fill, in lux. The ground is
/// white, so the composite normalizes the shadow-factor map by its own
/// brightest (fully-lit) pixel — these values only set how deep the *shadowed*
/// ground reads, and how the model is shaded.
const KEY_LUX: f32 = 2_600.0;
const FILL_LUX: f32 = 700.0;

/// Peak alpha of the soft AO contact shadow under the base, at zero lift.
const CONTACT_OPACITY: f32 = 0.4;
/// Fraction of the AO quad half-extent the (solid) footprint occupies; the
/// remainder is the outward feather of the halo.
const CONTACT_INNER: f32 = 0.62;

/// Default vignette strength (also `ShotParams::default().vignette`).
const VIGNETTE_STRENGTH: f32 = 0.16;
const VIGNETTE_INNER: f32 = 0.65;
const VIGNETTE_OUTER: f32 = 1.35;

const WATERMARK_OPACITY: f32 = 0.3;

// ---------------------------------------------------------------------------
// Tunable parameters (driven by the tweak panel)
// ---------------------------------------------------------------------------

/// Background style. `None` = a flat fill; the gradients sweep between a lighter
/// and the base tone for the dribbble "studio backdrop" look.
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum GradientMode {
    #[default]
    None,
    Vertical,
    Radial,
}

impl GradientMode {
    pub const ALL: [GradientMode; 3] = [
        GradientMode::None,
        GradientMode::Vertical,
        GradientMode::Radial,
    ];
    pub fn label(self) -> &'static str {
        match self {
            GradientMode::None => "Solid",
            GradientMode::Vertical => "Vertical",
            GradientMode::Radial => "Radial",
        }
    }
}

/// Output resolution / aspect presets. The preview always renders at
/// [`PREVIEW_LONG_EDGE`]; only the *aspect* affects the preview framing.
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum ResPreset {
    #[default]
    Landscape,
    Square,
    Wide,
    Portrait,
}

impl ResPreset {
    pub const ALL: [ResPreset; 4] = [
        ResPreset::Landscape,
        ResPreset::Square,
        ResPreset::Wide,
        ResPreset::Portrait,
    ];
    /// Full-resolution export dimensions.
    pub fn dims(self) -> (u32, u32) {
        match self {
            ResPreset::Landscape => (2000, 1500),
            ResPreset::Square => (2000, 2000),
            ResPreset::Wide => (1920, 1080),
            ResPreset::Portrait => (1500, 2000),
        }
    }
    pub fn aspect(self) -> f32 {
        let (w, h) = self.dims();
        w as f32 / h as f32
    }
    pub fn label(self) -> &'static str {
        match self {
            ResPreset::Landscape => "4:3",
            ResPreset::Square => "1:1",
            ResPreset::Wide => "16:9",
            ResPreset::Portrait => "3:4",
        }
    }
}

/// Low-res preview dimensions for a preset's aspect: long edge pinned to
/// [`PREVIEW_LONG_EDGE`].
fn preview_dims(res: ResPreset) -> (u32, u32) {
    let aspect = res.aspect();
    if aspect >= 1.0 {
        (
            PREVIEW_LONG_EDGE,
            ((PREVIEW_LONG_EDGE as f32) / aspect).round().max(1.0) as u32,
        )
    } else {
        (
            ((PREVIEW_LONG_EDGE as f32) * aspect).round().max(1.0) as u32,
            PREVIEW_LONG_EDGE,
        )
    }
}

/// All art-direction knobs. `Default` reproduces the phase-1 look (solid
/// hero-color background, full saturation, no lift, default vignette).
#[derive(Clone, PartialEq)]
pub struct ShotParams {
    pub gradient: GradientMode,
    /// Gradient spread (0 = flat, 1 = full lighten/darken sweep).
    pub gradient_strength: f32,
    /// Flip the gradient's light↔dark direction (top/bottom, center/edge).
    pub gradient_flip: bool,
    /// Override the auto hero-color background (`None` = auto).
    pub bg_override: Option<[u8; 3]>,
    /// Gradient dither amplitude in 8-bit units (0 = off) — kills banding.
    pub dither: f32,
    /// Vignette strength (0 = off).
    pub vignette: f32,
    /// Albedo saturation of the lit model (1.0 = true palette color).
    pub saturation: f32,
    /// World-space height the model floats above the ground.
    pub lift: f32,
    pub resolution: ResPreset,
}

impl Default for ShotParams {
    fn default() -> Self {
        Self {
            gradient: GradientMode::None,
            gradient_strength: 1.0,
            gradient_flip: false,
            bg_override: None,
            dither: 0.0,
            vignette: VIGNETTE_STRENGTH,
            saturation: 1.0,
            lift: 0.0,
            resolution: ResPreset::Landscape,
        }
    }
}

impl ShotParams {
    /// Knobs that require a GPU re-render (everything else recomposites from the
    /// cached captures on the CPU). Aspect — not raw resolution — drives the
    /// preview framing, but each preset has a distinct aspect anyway.
    pub fn scene_differs(&self, other: &Self) -> bool {
        self.saturation != other.saturation
            || self.lift != other.lift
            || self.resolution.aspect() != other.resolution.aspect()
    }
}

// ---------------------------------------------------------------------------
// Resources + state machine
// ---------------------------------------------------------------------------

#[derive(Resource, Default)]
pub struct ShotRequest(pub Option<PathBuf>);

/// Persistent tweak-panel state. Holds the live params, the cached low-res
/// captures (so post-only knobs recomposite instantly), and the egui-facing
/// preview texture.
#[derive(Resource, Default)]
pub struct ShotPanel {
    pub open: bool,
    pub params: ShotParams,
    needs_render: bool,
    needs_recomposite: bool,
    cached_a: Option<image::RgbaImage>,
    cached_b: Option<image::RgbaImage>,
    bg_base: [u8; 3],
    hero: [u8; 3],
    /// A scene knob moved but its slider is still being dragged — the GPU
    /// re-render is deferred (see `defer_scene_render` / `scene_due`).
    scene_pending: bool,
    /// Time (egui `input.time`, seconds) of the last deferred scene change.
    scene_since: f64,
    pending_upload: Option<image::RgbaImage>,
    preview_handle: Option<Handle<Image>>,
    pub preview_tex: Option<egui::TextureId>,
    pub preview_dims: (u32, u32),
}

impl ShotPanel {
    /// Open the panel and request the first preview render.
    pub fn open_panel(&mut self) {
        self.open = true;
        self.needs_render = true;
    }
    /// The auto-derived hero-color background of the last render — seed for the
    /// "Custom" background color picker so it starts from the computed pastel.
    pub fn auto_bg(&self) -> [u8; 3] {
        self.bg_base
    }
    /// Record that a knob changed: `scene` true → GPU re-render, else a cheap
    /// CPU recomposite from the cached captures.
    pub fn note_change(&mut self, scene: bool) {
        if scene {
            self.needs_render = true;
        } else {
            self.needs_recomposite = true;
        }
    }
    /// A scene knob changed while its slider is still being dragged: defer the
    /// (expensive) GPU re-render and stamp the change time, so it fires after a
    /// short pause mid-drag (or on release) rather than every frame.
    pub fn defer_scene_render(&mut self, now: f64) {
        self.scene_pending = true;
        self.scene_since = now;
    }
    /// Whether a deferred scene render has been idle for at least `debounce`
    /// seconds (the drag paused) and should now fire.
    pub fn scene_due(&self, now: f64, debounce: f64) -> bool {
        self.scene_pending && now - self.scene_since >= debounce
    }
    /// Whether a scene render is deferred and waiting (used to schedule a
    /// repaint so the debounce can fire during a held, motionless drag).
    pub fn has_pending_scene(&self) -> bool {
        self.scene_pending
    }
    /// Fire any deferred scene re-render now. Returns whether one was queued.
    pub fn flush_scene_render(&mut self) -> bool {
        if self.scene_pending {
            self.scene_pending = false;
            self.needs_render = true;
            true
        } else {
            false
        }
    }
}

#[derive(Clone)]
enum ShotTarget {
    Preview,
    Export(PathBuf),
}

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
    target: Option<ShotTarget>,
    params: ShotParams,
    bg_base: [u8; 3],
    hero: [u8; 3],
    image_a: Option<Handle<Image>>,
    image_b: Option<Handle<Image>>,
    screenshot_a: Option<Entity>,
    screenshot_b: Option<Entity>,
    captured_a: Option<image::RgbaImage>,
    captured_b: Option<image::RgbaImage>,
    entities: Vec<Entity>,
    meshes: Vec<Handle<Mesh>>,
    materials: Vec<Handle<StandardMaterial>>,
    textures: Vec<Handle<Image>>,
}

impl ShotSession {
    fn reset(&mut self) {
        self.phase = ShotPhase::Idle;
        self.target = None;
        self.bg_base = [0; 3];
        self.hero = [0; 3];
        self.image_a = None;
        self.image_b = None;
        self.screenshot_a = None;
        self.screenshot_b = None;
        self.captured_a = None;
        self.captured_b = None;
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
    mut panel: ResMut<ShotPanel>,
    grid: Res<GridResource>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut user_textures: ResMut<EguiUserTextures>,
    mut toasts: ResMut<Toasts>,
) {
    // Push any freshly composited preview into the egui-facing texture.
    upload_preview(&mut panel, &mut images, &mut user_textures);

    if !matches!(session.phase, ShotPhase::Idle) {
        if let ShotPhase::Warmup(_) = session.phase {
            advance_warmup(&mut commands, &mut session);
        }
        return;
    }

    // Idle: prioritize an explicit export request, then a panel render, then a
    // cheap recomposite of the cached captures.
    if let Some(path) = request.0.take() {
        let params = panel.params.clone();
        if begin_render(
            &mut commands,
            &mut session,
            &mut meshes,
            &mut materials,
            &mut images,
            &grid,
            ShotTarget::Export(path),
            params,
        )
        .is_err()
        {
            toasts.error("Nothing to export — the scene is empty");
        }
        return;
    }

    if panel.open && panel.needs_render {
        panel.needs_render = false;
        let params = panel.params.clone();
        let _ = begin_render(
            &mut commands,
            &mut session,
            &mut meshes,
            &mut materials,
            &mut images,
            &grid,
            ShotTarget::Preview,
            params,
        );
        return;
    }

    if panel.open && panel.needs_recomposite {
        panel.needs_recomposite = false;
        if panel.cached_a.is_some() && panel.cached_b.is_some() {
            let out = {
                let a = panel.cached_a.as_ref().unwrap();
                let b = panel.cached_b.as_ref().unwrap();
                composite(a, b, &panel.params, panel.bg_base, panel.hero)
            };
            panel.pending_upload = Some(out);
        }
    }
}

/// Upload a freshly composited preview `RgbaImage` into the egui texture,
/// reusing the same `Handle<Image>` (and thus `TextureId`) across updates.
fn upload_preview(
    panel: &mut ShotPanel,
    images: &mut Assets<Image>,
    user_textures: &mut EguiUserTextures,
) {
    let Some(rgba) = panel.pending_upload.take() else {
        return;
    };
    let (w, h) = rgba.dimensions();
    let img = Image::new(
        Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        rgba.into_raw(),
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    );
    match panel.preview_handle.clone() {
        Some(handle) => {
            let _ = images.insert(handle.id(), img);
        }
        None => {
            let handle = images.add(img);
            let tex = user_textures.add_image(EguiTextureHandle::Strong(handle.clone()));
            panel.preview_handle = Some(handle);
            panel.preview_tex = Some(tex);
        }
    }
    panel.preview_dims = (w, h);
}

/// Build the throwaway scene for `target` and enter warmup. `Err` when the
/// scene is empty (no bounding box).
#[allow(clippy::too_many_arguments)]
fn begin_render(
    commands: &mut Commands,
    session: &mut ShotSession,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
    grid: &VoxelGrid,
    target: ShotTarget,
    params: ShotParams,
) -> Result<(), ()> {
    let Some((min, max)) = grid.bounding_box() else {
        return Err(());
    };
    let res = match target {
        ShotTarget::Export(_) => params.resolution.dims(),
        ShotTarget::Preview => preview_dims(params.resolution),
    };
    let cells: Vec<(IVec3, Color8)> = grid.iter_occupied().collect();
    let hero = hero_color(&cells);
    let bg_base = auto_background(hero);

    build_scene(
        commands, session, meshes, materials, images, grid, min, max, &params, res,
    );

    session.target = Some(target);
    session.params = params;
    session.bg_base = bg_base;
    session.hero = hero;
    session.phase = ShotPhase::Warmup(WARMUP_FRAMES);
    Ok(())
}

fn advance_warmup(commands: &mut Commands, session: &mut ShotSession) {
    let ShotPhase::Warmup(n) = session.phase else {
        return;
    };
    if n > 0 {
        session.phase = ShotPhase::Warmup(n - 1);
        return;
    }
    let (Some(a), Some(b)) = (session.image_a.clone(), session.image_b.clone()) else {
        session.reset();
        return;
    };
    let ea = commands
        .spawn(Screenshot::image(a))
        .observe(on_shot_captured)
        .id();
    let eb = commands
        .spawn(Screenshot::image(b))
        .observe(on_shot_captured)
        .id();
    session.screenshot_a = Some(ea);
    session.screenshot_b = Some(eb);
    session.phase = ShotPhase::Capturing;
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
    params: &ShotParams,
    res: (u32, u32),
) {
    let model_layer = RenderLayers::layer(SHOT_LAYER);
    let ground_layer = RenderLayers::layer(GROUND_LAYER);
    // The lights touch both layers: they shade the model (layer 2) for camera A
    // and light the ground (layer 3) for camera B. Shadow casting filters by
    // `light ∩ caster`, so the model (layer 2) still casts onto the ground.
    let light_layers = RenderLayers::from_layers(&[SHOT_LAYER, GROUND_LAYER]);

    // World-space AABB of the model: voxel p occupies [p, p+1].
    let model_min = min.as_vec3();
    let model_max = (max + IVec3::ONE).as_vec3();
    let base_y = model_min.y;
    let scene_size = (model_max - model_min).length().max(8.0);

    // ---- Model (lit albedo mesh), floated up by `lift` ----
    let model_mesh = meshes.add(build_lit_mesh(grid, params.saturation));
    let model_mat = materials.add(matte_material(Color::WHITE));
    let model_entity = commands
        .spawn((
            Mesh3d(model_mesh.clone()),
            MeshMaterial3d(model_mat.clone()),
            Transform::from_translation(Vec3::new(0.0, params.lift, 0.0)),
            model_layer.clone(),
        ))
        .id();
    session.meshes.push(model_mesh);
    session.materials.push(model_mat);
    session.entities.push(model_entity);

    let cx = (model_min.x + model_max.x) * 0.5;
    let cz = (model_min.z + model_max.z) * 0.5;

    // ---- Ground plane (WHITE albedo, receives the real cast shadow) ----
    // Captured by camera B over a white clear: a fully-lit ground pixel reads
    // ~white, a shadowed one darkens — i.e. a per-pixel shadow-factor map.
    let ground_half = scene_size * 12.0;
    let ground_mesh = meshes.add(Mesh::from(Plane3d::new(Vec3::Y, Vec2::splat(ground_half))));
    let ground_mat = materials.add(matte_material(Color::WHITE));
    let ground_entity = commands
        .spawn((
            Mesh3d(ground_mesh.clone()),
            MeshMaterial3d(ground_mat.clone()),
            Transform::from_xyz(cx, base_y, cz),
            ground_layer.clone(),
        ))
        .id();
    session.meshes.push(ground_mesh);
    session.materials.push(ground_mat);
    session.entities.push(ground_entity);

    // ---- Lighting + real cast shadow ----
    let key_dir = Vec3::new(-0.5, -1.0, 0.32).normalize();
    let cascade = CascadeShadowConfigBuilder {
        num_cascades: 1,
        maximum_distance: scene_size * 4.0 + params.lift + 50.0,
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
            light_layers.clone(),
        ))
        .id();
    session.entities.push(key);
    let fill = commands
        .spawn((
            DirectionalLight {
                illuminance: FILL_LUX,
                shadows_enabled: false,
                ..default()
            },
            Transform::IDENTITY.looking_to(Vec3::new(0.4, -0.5, -0.4).normalize(), Vec3::Y),
            light_layers,
        ))
        .id();
    session.entities.push(fill);

    // ---- Soft AO contact halo (neutral, on the ground layer) ----
    // A black blended pool that just *darkens* the shadow-factor map under the
    // footprint; the composite re-colors it via `gradient * factor`. Fades and
    // spreads as the model lifts off the ground.
    let sx = model_max.x - model_min.x;
    let sz = model_max.z - model_min.z;
    let lift_f = (params.lift / scene_size).clamp(0.0, 1.0);
    let contact_tex = images.add(soft_blob_image(256));
    let contact_scale = (1.0 + lift_f * 1.5) / CONTACT_INNER;
    let contact_mesh = meshes.add(Mesh::from(Plane3d::new(
        Vec3::Y,
        Vec2::new(sx * contact_scale * 0.5, sz * contact_scale * 0.5),
    )));
    let contact_op = CONTACT_OPACITY * (1.0 - 0.6 * lift_f);
    let contact_mat = materials.add(StandardMaterial {
        base_color: Color::srgba(0.0, 0.0, 0.0, contact_op),
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
            ground_layer.clone(),
        ))
        .id();
    session.meshes.push(contact_mesh);
    session.materials.push(contact_mat);
    session.textures.push(contact_tex);
    session.entities.push(contact);

    // ---- Cameras (shared transform + projection; aligned captures) ----
    let lifted_max = Vec3::new(model_max.x, model_max.y + params.lift, model_max.z);
    let union_min = Vec3::new(model_min.x, base_y, model_min.z);
    let union_max = lifted_max;
    let focus = (union_min + union_max) * 0.5;
    let corners = aabb_corners(union_min, union_max);
    let aspect = res.0 as f32 / res.1 as f32;
    let look = Transform::from_translation(focus + iso_dir()).looking_at(focus, Vec3::Y);
    let view_h = fit_ortho_height(&corners, look.rotation, aspect, FRAME_MARGIN);
    let dist = scene_size * 3.0 + params.lift + 50.0;
    let cam_transform =
        Transform::from_translation(focus + iso_dir() * dist).looking_at(focus, Vec3::Y);

    let image_a = images.add(Image::new_target_texture(
        res.0,
        res.1,
        TextureFormat::Rgba8UnormSrgb,
        None,
    ));
    let image_b = images.add(Image::new_target_texture(
        res.0,
        res.1,
        TextureFormat::Rgba8UnormSrgb,
        None,
    ));

    let cam_a = spawn_shot_camera(
        commands,
        model_layer,
        ClearColorConfig::Custom(Color::NONE),
        image_a.clone(),
        cam_transform,
        view_h,
        dist,
        20,
    );
    let cam_b = spawn_shot_camera(
        commands,
        ground_layer,
        ClearColorConfig::Custom(Color::WHITE),
        image_b.clone(),
        cam_transform,
        view_h,
        dist,
        21,
    );
    session.entities.push(cam_a);
    session.entities.push(cam_b);
    session.image_a = Some(image_a);
    session.image_b = Some(image_b);
}

#[allow(clippy::too_many_arguments)]
fn spawn_shot_camera(
    commands: &mut Commands,
    layer: RenderLayers,
    clear: ClearColorConfig,
    target: Handle<Image>,
    transform: Transform,
    view_h: f32,
    dist: f32,
    order: isize,
) -> Entity {
    commands
        .spawn((
            Camera3d::default(),
            Camera {
                clear_color: clear,
                order,
                ..default()
            },
            RenderTarget::Image(target.into()),
            transform,
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
        .id()
}

#[allow(clippy::too_many_arguments)]
fn on_shot_captured(
    trigger: On<ScreenshotCaptured>,
    mut commands: Commands,
    mut session: ResMut<ShotSession>,
    mut panel: ResMut<ShotPanel>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut toasts: ResMut<Toasts>,
) {
    let captured = trigger.entity;
    commands.entity(captured).despawn();

    let decoded = decode_capture(&trigger.image);
    if Some(captured) == session.screenshot_a {
        session.captured_a = decoded;
    } else if Some(captured) == session.screenshot_b {
        session.captured_b = decoded;
    }

    if session.captured_a.is_none() || session.captured_b.is_none() {
        return;
    }

    // Both captures in: composite and route by target.
    let (Some(a), Some(b)) = (session.captured_a.take(), session.captured_b.take()) else {
        return;
    };
    let params = session.params.clone();
    let bg = session.bg_base;
    let hero = session.hero;
    let out = composite(&a, &b, &params, bg, hero);

    match session.target.take() {
        Some(ShotTarget::Export(path)) => {
            match out.save_with_format(&path, image::ImageFormat::Png) {
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
        Some(ShotTarget::Preview) => {
            panel.cached_a = Some(a);
            panel.cached_b = Some(b);
            panel.bg_base = bg;
            panel.hero = hero;
            panel.pending_upload = Some(out);
        }
        None => {}
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
    if let Some(h) = session.image_a.take() {
        images.remove(&h);
    }
    if let Some(h) = session.image_b.take() {
        images.remove(&h);
    }
    session.reset();
}

fn decode_capture(img: &Image) -> Option<image::RgbaImage> {
    img.clone().try_into_dynamic().ok().map(|d| d.into_rgba8())
}

// ---------------------------------------------------------------------------
// CPU composite pipeline (preview + export share this)
// ---------------------------------------------------------------------------

/// Composite the model (`img_a`, RGBA with alpha) over a background painted from
/// the shadow-factor map (`img_b`) and `params`, then vignette + watermark.
fn composite(
    img_a: &image::RgbaImage,
    img_b: &image::RgbaImage,
    params: &ShotParams,
    bg_base: [u8; 3],
    hero: [u8; 3],
) -> image::RgbaImage {
    let (w, h) = img_a.dimensions();
    let bg = params.bg_override.unwrap_or(bg_base);
    // Cast shadow + vignette darken via a *multiply* by a slightly-darkened hero
    // color, so shadowed ground and frame corners pick up the model's hue
    // instead of going neutral grey/black.
    let tint = multiply_tint(hero);
    let tf = [
        tint[0] as f32 / 255.0,
        tint[1] as f32 / 255.0,
        tint[2] as f32 / 255.0,
    ];

    // Normalize the shadow factor by the brightest (fully-lit) ground pixel, so
    // lit areas land at 1.0 regardless of the exact lighting calibration.
    let mut lit_ref = 0.0f32;
    for p in img_b.pixels() {
        let l = shadow_factor([p.0[0], p.0[1], p.0[2]]);
        if l > lit_ref {
            lit_ref = l;
        }
    }
    let lit_ref = lit_ref.max(1e-3);

    let mut out = image::RgbaImage::new(w, h);
    for y in 0..h {
        for x in 0..w {
            let bp = img_b
                .get_pixel(x.min(img_b.width() - 1), y.min(img_b.height() - 1))
                .0;
            let k = (shadow_factor([bp[0], bp[1], bp[2]]) / lit_ref).clamp(0.0, 1.0);
            let g = gradient_color(x, y, w, h, params, bg);
            let mut bg_px = [0u8; 3];
            for c in 0..3 {
                // Lit (k=1) → no darken; full shadow (k=0) → multiply by the tint.
                let mul = tf[c] + (1.0 - tf[c]) * k;
                bg_px[c] = (g[c] as f32 * mul).round().clamp(0.0, 255.0) as u8;
            }
            let ap = img_a.get_pixel(x, y).0;
            let a = ap[3] as f32 / 255.0;
            let mut px = [0u8; 3];
            for c in 0..3 {
                px[c] = (ap[c] as f32 * a + bg_px[c] as f32 * (1.0 - a))
                    .round()
                    .clamp(0.0, 255.0) as u8;
            }
            out.put_pixel(x, y, image::Rgba([px[0], px[1], px[2], 255]));
        }
    }

    // Grain is a full-frame post pass (over model + background + shadows) so the
    // whole shot reads with one cohesive film grain, not just the flat bg.
    apply_grain(&mut out, params.dither);
    apply_vignette(&mut out, params.vignette, tint);
    apply_watermark(&mut out);
    out
}

/// Add per-pixel white-noise grain ([`dither_offset`]) to the whole image.
fn apply_grain(img: &mut image::RgbaImage, amount: f32) {
    if amount <= 0.0 {
        return;
    }
    let (w, h) = img.dimensions();
    for y in 0..h {
        for x in 0..w {
            let off = dither_offset(x, y, amount);
            let p = img.get_pixel_mut(x, y);
            for c in p.0.iter_mut().take(3) {
                *c = (*c as f32 + off).round().clamp(0.0, 255.0) as u8;
            }
        }
    }
}

/// Background color at pixel (x,y). Solid = `bg`; gradients sweep between a
/// lightened and a darkened tone of `bg`, scaled by `gradient_strength` and
/// optionally flipped. `gradient_flip == false` reads light→dark (top→bottom,
/// center→edge).
fn gradient_color(x: u32, y: u32, w: u32, h: u32, params: &ShotParams, bg: [u8; 3]) -> [u8; 3] {
    let s = params.gradient_strength.clamp(0.0, 1.0);
    match params.gradient {
        GradientMode::None => bg,
        GradientMode::Vertical => {
            let hi = lighten(bg, 0.22 * s);
            let lo = darken(bg, 0.14 * s);
            let (a, b) = if params.gradient_flip {
                (lo, hi)
            } else {
                (hi, lo)
            };
            let t = if h > 1 {
                y as f32 / (h - 1) as f32
            } else {
                0.0
            };
            mix_rgb(a, b, t)
        }
        GradientMode::Radial => {
            let inner = lighten(bg, 0.20 * s);
            let outer = darken(bg, 0.12 * s);
            let (a, b) = if params.gradient_flip {
                (outer, inner)
            } else {
                (inner, outer)
            };
            let cx = (w.max(1) as f32 - 1.0) * 0.5;
            let cy = (h.max(1) as f32 - 1.0) * 0.5;
            let max_d = (cx * cx + cy * cy).sqrt().max(1e-3);
            let dx = x as f32 - cx;
            let dy = y as f32 - cy;
            let r = ((dx * dx + dy * dy).sqrt() / max_d).clamp(0.0, 1.0);
            mix_rgb(a, b, r)
        }
    }
}

/// A slightly-darkened hero color used as the "multiply" tint for the cast
/// shadow and vignette. Keeps the hue (so shadows read warm/cool with the
/// model) while pulling value down.
fn multiply_tint(hero: [u8; 3]) -> [u8; 3] {
    darken(hero, 0.5)
}

/// Per-pixel white-noise grain offset in 8-bit units, centered on zero and
/// bounded to ±`amount`/2. Deterministic per (x, y) — a hash, not an ordered
/// tile — so it reads as fine film grain over the background rather than a
/// repeating cross-hatch. Zero when `amount <= 0`.
fn dither_offset(x: u32, y: u32, amount: f32) -> f32 {
    if amount <= 0.0 {
        return 0.0;
    }
    // Integer hash → uniform [0, 1).
    let mut n = x
        .wrapping_mul(0x1657_4d2b)
        .wrapping_add(y.wrapping_mul(0x68e3_1da4))
        .wrapping_add(0x9e37_79b9);
    n ^= n >> 15;
    n = n.wrapping_mul(0x85eb_ca6b);
    n ^= n >> 13;
    n = n.wrapping_mul(0xc2b2_ae35);
    n ^= n >> 16;
    let u = (n & 0x00ff_ffff) as f32 / 0x0100_0000_u32 as f32;
    (u - 0.5) * amount
}

/// Per-pixel shadow factor of a ground capture pixel (0 = black/shadow, 1 =
/// white/lit), before normalization. Equals its relative luminance.
fn shadow_factor(rgb: [u8; 3]) -> f32 {
    relative_luma(rgb)
}

fn mix_rgb(a: [u8; 3], b: [u8; 3], t: f32) -> [u8; 3] {
    let t = t.clamp(0.0, 1.0);
    let mut out = [0u8; 3];
    for c in 0..3 {
        out[c] = (a[c] as f32 + (b[c] as f32 - a[c] as f32) * t)
            .round()
            .clamp(0.0, 255.0) as u8;
    }
    out
}

fn lighten(c: [u8; 3], t: f32) -> [u8; 3] {
    mix_rgb(c, [255, 255, 255], t)
}

fn darken(c: [u8; 3], t: f32) -> [u8; 3] {
    mix_rgb(c, [0, 0, 0], t)
}

/// Smoothstep in [edge0, edge1].
fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Signed distance to an axis-aligned rounded box (half-extents hx,hy, corner
/// radius r), centered at origin. Negative inside.
fn sdf_round_box(px: f32, py: f32, hx: f32, hy: f32, r: f32) -> f32 {
    let qx = px.abs() - (hx - r);
    let qy = py.abs() - (hy - r);
    (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt() + qx.max(qy).min(0.0) - r
}

/// AO contact-halo mask at normalized point (px,py) in [-1,1]: SOLID across the
/// footprint (out to `inner`), then feathers *outward* to zero at the quad edge.
fn contact_alpha(px: f32, py: f32, inner: f32) -> f32 {
    let corner = inner * 0.4;
    let d = sdf_round_box(px, py, inner, inner, corner);
    let feather = (1.0 - inner).max(1e-3);
    (1.0 - smoothstep(0.0, feather, d)).clamp(0.0, 1.0)
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

/// Vignette multiplier at normalized distance `dist` from center (half-extent =
/// 1). 1.0 at center, dipping toward the corners by `strength`.
fn vignette_factor(dist: f32, strength: f32) -> f32 {
    1.0 - strength * smoothstep(VIGNETTE_INNER, VIGNETTE_OUTER, dist)
}

/// Multiply each pixel's RGB toward `tint` by [`vignette_factor`] of its
/// distance from center: center untouched, corners pull toward the darkened
/// hero `tint` (a multiply, so the corners darken *and* pick up the hue).
fn apply_vignette(img: &mut image::RgbaImage, strength: f32, tint: [u8; 3]) {
    let (w, h) = img.dimensions();
    if w == 0 || h == 0 || strength <= 0.0 {
        return;
    }
    let tf = [
        tint[0] as f32 / 255.0,
        tint[1] as f32 / 255.0,
        tint[2] as f32 / 255.0,
    ];
    let cx = (w as f32 - 1.0) * 0.5;
    let cy = (h as f32 - 1.0) * 0.5;
    let (hx, hy) = (w as f32 * 0.5, h as f32 * 0.5);
    for y in 0..h {
        for x in 0..w {
            let nx = (x as f32 - cx) / hx;
            let ny = (y as f32 - cy) / hy;
            // f: 1 at center → <1 toward corners.
            let f = vignette_factor((nx * nx + ny * ny).sqrt(), strength);
            let p = img.get_pixel_mut(x, y);
            for (c, px) in p.0.iter_mut().take(3).enumerate() {
                let mul = tf[c] + (1.0 - tf[c]) * f;
                *px = (*px as f32 * mul).round().clamp(0.0, 255.0) as u8;
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
/// saturation² × value³, so the most prominent *vivid, bright* color wins.
/// Falls back to the average for a fully grey model.
fn hero_color(cells: &[(IVec3, Color8)]) -> [u8; 3] {
    use std::collections::HashMap;
    let mut counts: HashMap<[u8; 3], u32> = HashMap::new();
    for (_, c) in cells {
        *counts.entry([c[0], c[1], c[2]]).or_insert(0) += 1;
    }
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

/// Light pastel of the model's hero hue — the auto background base.
pub fn auto_background(avg: [u8; 3]) -> [u8; 3] {
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
/// given camera rotation and output aspect ratio.
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
    fn shadow_factor_maps_lit_to_one_shadow_to_zero() {
        assert!((shadow_factor([255, 255, 255]) - 1.0).abs() < 1e-4);
        assert!((shadow_factor([128, 128, 128]) - 0.502).abs() < 1e-2);
        assert!(shadow_factor([0, 0, 0]) < 1e-6);
    }

    #[test]
    fn watermark_tint_follows_contrast() {
        assert_eq!(pick_watermark_tint(0.9), [0, 0, 0]);
        assert_eq!(pick_watermark_tint(0.1), [255, 255, 255]);
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
        let bg = auto_background([40, 200, 40]);
        assert!(
            bg[1] > bg[0] && bg[1] > bg[2],
            "bg should carry hue: {bg:?}"
        );
    }

    #[test]
    fn contact_alpha_solid_to_footprint_then_feathers_out() {
        let inner = 0.62;
        assert!((contact_alpha(0.0, 0.0, inner) - 1.0).abs() < 1e-6);
        assert!(contact_alpha(0.3, 0.3, inner) > 0.99);
        let ring = contact_alpha(0.8, 0.0, inner);
        assert!(ring > 0.0 && ring < 1.0, "ring={ring}");
        assert_eq!(contact_alpha(1.0, 1.0, inner), 0.0);
    }

    #[test]
    fn vignette_darkens_corners_not_center() {
        assert!((vignette_factor(0.0, VIGNETTE_STRENGTH) - 1.0).abs() < 1e-6);
        assert!((vignette_factor(0.5, VIGNETTE_STRENGTH) - 1.0).abs() < 1e-6);
        let corner = vignette_factor(2.0_f32.sqrt(), VIGNETTE_STRENGTH);
        assert!(corner < 1.0 && corner > 0.7, "corner factor {corner}");
        // Strength 0 disables it entirely.
        assert_eq!(vignette_factor(2.0_f32.sqrt(), 0.0), 1.0);
    }

    #[test]
    fn gradient_color_modes() {
        let bg = [100, 150, 120];
        let solid = ShotParams {
            gradient: GradientMode::None,
            ..Default::default()
        };
        // Solid ignores position.
        assert_eq!(gradient_color(0, 0, 100, 100, &solid, bg), bg);
        assert_eq!(gradient_color(50, 99, 100, 100, &solid, bg), bg);

        let vert = ShotParams {
            gradient: GradientMode::Vertical,
            ..Default::default()
        };
        let top = gradient_color(0, 0, 100, 100, &vert, bg);
        let bottom = gradient_color(0, 99, 100, 100, &vert, bg);
        // Default (unflipped) sweeps light top → dark bottom.
        assert!(relative_luma(top) > relative_luma(bg));
        assert!(relative_luma(bottom) < relative_luma(bg));

        let rad = ShotParams {
            gradient: GradientMode::Radial,
            ..Default::default()
        };
        let center = gradient_color(50, 50, 100, 100, &rad, bg);
        let corner = gradient_color(0, 0, 100, 100, &rad, bg);
        assert!(
            relative_luma(center) > relative_luma(corner),
            "center brighter"
        );
    }

    #[test]
    fn gradient_flip_reverses_direction() {
        let bg = [100, 150, 120];
        let base = ShotParams {
            gradient: GradientMode::Vertical,
            ..Default::default()
        };
        let flipped = ShotParams {
            gradient_flip: true,
            ..base.clone()
        };
        // Unflipped: light top, dark bottom. Flipped: the reverse.
        assert!(relative_luma(gradient_color(0, 0, 100, 100, &base, bg)) > relative_luma(bg));
        assert!(relative_luma(gradient_color(0, 0, 100, 100, &flipped, bg)) < relative_luma(bg));
    }

    #[test]
    fn gradient_strength_scales_spread() {
        let bg = [100, 150, 120];
        let strong = ShotParams {
            gradient: GradientMode::Vertical,
            gradient_strength: 1.0,
            ..Default::default()
        };
        let weak = ShotParams {
            gradient_strength: 0.25,
            ..strong.clone()
        };
        let flat = ShotParams {
            gradient_strength: 0.0,
            ..strong.clone()
        };
        let spread = |p: &ShotParams| {
            relative_luma(gradient_color(0, 0, 100, 100, p, bg))
                - relative_luma(gradient_color(0, 99, 100, 100, p, bg))
        };
        assert!(spread(&strong) > spread(&weak));
        assert!(spread(&weak) > 0.0);
        // Zero strength → flat (top == bottom == bg).
        assert_eq!(gradient_color(0, 0, 100, 100, &flat, bg), bg);
        assert_eq!(gradient_color(0, 99, 100, 100, &flat, bg), bg);
    }

    #[test]
    fn multiply_tint_darkens_and_keeps_hue() {
        let hero = [40, 200, 40];
        let t = multiply_tint(hero);
        // Darker than the hero on every channel that had value.
        assert!(t[1] < hero[1]);
        // Green still dominant → hue preserved.
        assert!(t[1] > t[0] && t[1] > t[2]);
    }

    #[test]
    fn gradient_uses_override_only_via_bg_arg() {
        // gradient_color takes the resolved bg directly; passing two different
        // bases yields two different solids.
        let p = ShotParams::default();
        assert_ne!(
            gradient_color(0, 0, 8, 8, &p, [10, 20, 30]),
            gradient_color(0, 0, 8, 8, &p, [200, 100, 50])
        );
    }

    #[test]
    fn dither_offset_noise_zero_mean_and_bounded() {
        let amount = 16.0;
        let mut sum = 0.0f32;
        let mut max_abs = 0.0f32;
        for y in 0..32 {
            for x in 0..32 {
                let o = dither_offset(x, y, amount);
                sum += o;
                max_abs = max_abs.max(o.abs());
            }
        }
        let mean = sum / (32.0 * 32.0);
        // White noise → near-zero mean over a decent sample, bounded ±amount/2.
        assert!(mean.abs() < amount * 0.05, "grain bias too high: {mean}");
        assert!(max_abs <= amount * 0.5, "bounded by ±amount/2");
        // Disabled at amount 0.
        assert_eq!(dither_offset(3, 5, 0.0), 0.0);
        // Deterministic per (x,y).
        assert_eq!(dither_offset(3, 5, amount), dither_offset(3, 5, amount));
        // Adjacent pixels differ → it's noise, not a flat offset.
        assert_ne!(dither_offset(3, 5, amount), dither_offset(4, 5, amount));
    }

    #[test]
    fn apply_grain_perturbs_whole_image_only_when_enabled() {
        let mid = image::Rgba([128, 128, 128, 255]);
        // Disabled (amount 0) → untouched.
        let mut off = image::RgbaImage::from_pixel(16, 16, mid);
        apply_grain(&mut off, 0.0);
        assert!(off.pixels().all(|p| p.0 == [128, 128, 128, 255]));
        // Enabled → some pixels move, and not uniformly (it's noise).
        let mut on = image::RgbaImage::from_pixel(16, 16, mid);
        apply_grain(&mut on, 20.0);
        assert!(on.pixels().any(|p| p.0[0] != 128), "grain changed nothing");
        let first = on.get_pixel(0, 0).0[0];
        assert!(
            on.pixels().any(|p| p.0[0] != first),
            "grain must vary per pixel"
        );
        // Alpha is never touched.
        assert!(on.pixels().all(|p| p.0[3] == 255));
    }

    #[test]
    fn composite_passes_model_and_paints_lit_background() {
        // 8x8 so the bottom-right watermark stays clear of the top-row pixels we
        // sample. Top-left is opaque model red; the rest is transparent (bg).
        let (w, h) = (8, 8);
        let mut a = image::RgbaImage::from_pixel(w, h, image::Rgba([0, 0, 0, 0]));
        a.put_pixel(0, 0, image::Rgba([255, 0, 0, 255]));
        // Ground capture: fully lit (white) everywhere.
        let b = image::RgbaImage::from_pixel(w, h, image::Rgba([255, 255, 255, 255]));
        let params = ShotParams {
            gradient: GradientMode::None,
            vignette: 0.0,
            ..Default::default()
        };
        let out = composite(&a, &b, &params, [40, 120, 200], [40, 120, 200]);
        // Model pixel passes through (top-left, clear of the watermark).
        let mp = out.get_pixel(0, 0).0;
        assert_eq!(mp[0], 255);
        assert!(mp[1] < 40 && mp[2] < 40);
        // Background pixel (top-right, clear of the watermark) = lit (k=1) bg color.
        let bgp = out.get_pixel(w - 1, 0).0;
        assert_eq!([bgp[0], bgp[1], bgp[2]], [40, 120, 200]);
        assert_eq!(bgp[3], 255);
    }

    #[test]
    fn composite_shadow_darkens_background() {
        // One background pixel lit, one in shadow → shadowed reads darker.
        let a = image::RgbaImage::from_pixel(2, 1, image::Rgba([0, 0, 0, 0]));
        let mut b = image::RgbaImage::new(2, 1);
        b.put_pixel(0, 0, image::Rgba([255, 255, 255, 255])); // lit
        b.put_pixel(1, 0, image::Rgba([100, 100, 100, 255])); // shadow
        let params = ShotParams {
            gradient: GradientMode::None,
            vignette: 0.0,
            dither: 0.0,
            ..Default::default()
        };
        let out = composite(&a, &b, &params, [200, 200, 200], [200, 200, 200]);
        let lit = out.get_pixel(0, 0).0;
        let shadow = out.get_pixel(1, 0).0;
        assert!(
            relative_luma([shadow[0], shadow[1], shadow[2]])
                < relative_luma([lit[0], lit[1], lit[2]])
        );
    }

    #[test]
    fn default_params_reproduce_phase_one() {
        let p = ShotParams::default();
        assert_eq!(p.gradient, GradientMode::None);
        assert_eq!(p.bg_override, None);
        assert_eq!(p.dither, 0.0);
        assert_eq!(p.saturation, 1.0);
        assert_eq!(p.lift, 0.0);
        assert_eq!(p.vignette, VIGNETTE_STRENGTH);
        assert_eq!(p.resolution, ResPreset::Landscape);
        assert_eq!(p.gradient_strength, 1.0);
        assert!(!p.gradient_flip);
    }

    #[test]
    fn deferred_scene_render_debounces_then_fires() {
        let mut p = ShotPanel::default();
        // Dragging a scene slider at t=1.0 defers the render.
        p.defer_scene_render(1.0);
        assert!(!p.needs_render, "no render while still dragging");
        // Still within the debounce window → not yet due.
        assert!(!p.scene_due(1.05, 0.1));
        // Paused past the window → due.
        assert!(p.scene_due(1.2, 0.1));
        // Flushing queues the render and clears the pending flag.
        assert!(p.flush_scene_render());
        assert!(p.needs_render, "render queued after debounce");
        assert!(!p.scene_due(2.0, 0.1), "no longer pending");
        // Nothing pending → flush is a no-op.
        let mut q = ShotPanel::default();
        assert!(!q.flush_scene_render());
        assert!(!q.needs_render);
    }

    #[test]
    fn scene_differs_only_for_render_knobs() {
        let base = ShotParams::default();
        // Post-only knobs: no re-render.
        let mut post = base.clone();
        post.gradient = GradientMode::Vertical;
        post.dither = 8.0;
        post.vignette = 0.3;
        post.bg_override = Some([1, 2, 3]);
        assert!(!base.scene_differs(&post));
        // Scene knobs: re-render.
        let mut sat = base.clone();
        sat.saturation = 0.5;
        assert!(base.scene_differs(&sat));
        let mut lift = base.clone();
        lift.lift = 3.0;
        assert!(base.scene_differs(&lift));
        let mut res = base.clone();
        res.resolution = ResPreset::Square;
        assert!(base.scene_differs(&res));
    }

    #[test]
    fn res_preset_dims_and_aspect() {
        assert_eq!(ResPreset::Landscape.dims(), (2000, 1500));
        assert!((ResPreset::Wide.aspect() - 16.0 / 9.0).abs() < 1e-3);
        assert!(ResPreset::Portrait.aspect() < 1.0);
    }

    #[test]
    fn preview_dims_pin_long_edge() {
        let (w, h) = preview_dims(ResPreset::Landscape);
        assert_eq!(w.max(h), PREVIEW_LONG_EDGE);
        let (pw, ph) = preview_dims(ResPreset::Portrait);
        assert_eq!(pw.max(ph), PREVIEW_LONG_EDGE);
        assert!(ph > pw, "portrait preview is taller than wide");
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
    fn hero_color_prefers_vivid_over_common_dull() {
        let mut cells = Vec::new();
        for _ in 0..100 {
            cells.push((IVec3::ZERO, [90, 85, 80, 255]));
        }
        for _ in 0..30 {
            cells.push((IVec3::ONE, [40, 200, 40, 255]));
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
        let rot = Quat::IDENTITY;
        let h = fit_ortho_height(
            &aabb_corners(Vec3::ZERO, Vec3::new(40.0, 2.0, 2.0)),
            rot,
            4.0 / 3.0,
            0.0,
        );
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
    fn open_panel_requests_first_render() {
        let mut p = ShotPanel::default();
        assert!(!p.open && !p.needs_render);
        p.open_panel();
        assert!(p.open, "panel opens");
        assert!(p.needs_render, "first preview render queued");
    }

    #[test]
    fn note_change_routes_scene_vs_post() {
        // Post-only knob → cheap CPU recomposite, no GPU re-render.
        let mut post = ShotPanel::default();
        post.note_change(false);
        assert!(post.needs_recomposite && !post.needs_render);
        // Scene knob → GPU re-render.
        let mut scene = ShotPanel::default();
        scene.note_change(true);
        assert!(scene.needs_render && !scene.needs_recomposite);
    }

    #[test]
    fn auto_bg_returns_cached_base() {
        let p = ShotPanel {
            bg_base: [10, 20, 30],
            ..Default::default()
        };
        assert_eq!(p.auto_bg(), [10, 20, 30]);
    }

    #[test]
    fn embedded_wordmark_decodes_with_alpha() {
        let mark = image::load_from_memory(WORDMARK_PNG)
            .expect("wordmark png decodes")
            .into_rgba8();
        let (w, h) = mark.dimensions();
        assert!(w > 0 && h > 0);
        assert!(mark.pixels().any(|p| p.0[3] == 0));
        assert!(mark.pixels().any(|p| p.0[3] > 200));
    }
}
