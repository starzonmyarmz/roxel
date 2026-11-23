//! Scripted capture mode for screenshots: load a scene, frame it, write one
//! PNG of the editor window (`--capture`) or one Roxel Shot (`--export-shot`),
//! then quit. Hidden from users: it only runs when `ROXEL_CAPTURE` is set.
//!
//! ```sh
//! ROXEL_CAPTURE=1 Roxel scene.rox --capture out.png --yaw 45 --pitch 30 --theme dark
//! ROXEL_CAPTURE=1 Roxel scene.rox --export-shot out.png --yaw 135 --aspect 3:4 --style radial
//! ```
//!
//! Capture runs on default preferences (plus the flags) and never writes the
//! user's config files.

use std::f32::consts::FRAC_PI_2;
use std::path::PathBuf;

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured};
use bevy::window::PrimaryWindow;
use bevy_panorbit_camera::PanOrbitCamera;
use roxel::grid::VoxelGrid;

use crate::GridResource;
use crate::camera::ViewportRect;
use crate::onboarding::OnboardingAnchors;
use crate::shot::{GradientMode, ResPreset, ShotPanel, ShotParams, ShotRequest, ShotSession};
use crate::snapshot::PngWrites;
use crate::theme::{Preferences, ThemePref};
use crate::ui::{ToastKind, Toasts, UnsavedGuard};

pub const ENV_VAR: &str = "ROXEL_CAPTURE";

/// Share of the span between the inspector and the tool rail the model may
/// fill sideways.
const SIDE_FILL: f32 = 0.8;
/// Frames the framed window must hold steady before the screenshot.
const SETTLE_FRAMES: u32 = 30;
/// Give up if the scene hasn't loaded, meshed and been captured by then.
const FRAME_LIMIT: u32 = 3600;

#[derive(Clone, Debug, PartialEq)]
pub enum Mode {
    Window,
    Shot,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Focus {
    /// Fraction of the model's bounding box on each axis, y up from the floor.
    Fraction(Vec3),
    Voxel(IVec3),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Detail {
    pub focus: Focus,
    pub zoom: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Framing {
    /// Share of the window height the model fills.
    pub fill: f32,
    /// Weight of the visual-mass centroid against the bounding-box center.
    pub mass: f32,
    /// Extra offset of the composition center, in window points.
    pub nudge: Vec2,
}

impl Default for Framing {
    fn default() -> Self {
        Self {
            fill: 0.72,
            mass: 0.15,
            nudge: Vec2::ZERO,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct CaptureArgs {
    pub scene: PathBuf,
    pub out: PathBuf,
    pub mode: Mode,
    /// Radians, `bevy_panorbit_camera` convention (the isometric preset is π/4).
    pub yaw: f32,
    pub pitch: f32,
    pub theme: ThemePref,
    /// Window size in points.
    pub size: UVec2,
    pub framing: Framing,
    pub detail: Option<Detail>,
    pub shot: ShotParams,
}

/// The capture requested by this process's flags, or `None` for a normal
/// launch. Bad flags print an error and exit.
pub fn from_env() -> Option<CaptureArgs> {
    std::env::var_os(ENV_VAR)?;
    let args: Vec<String> = std::env::args_os()
        .skip(1)
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
    if !asks_for_capture(&args) {
        return None;
    }
    match parse(args) {
        Ok(args) => Some(args),
        Err(e) => {
            eprintln!("roxel capture: {e}");
            std::process::exit(2);
        }
    }
}

/// A stray `ROXEL_CAPTURE` in the environment mustn't break a normal launch,
/// so capture only starts when an output flag is there too.
fn asks_for_capture(args: &[String]) -> bool {
    args.iter()
        .any(|a| a == "--capture" || a == "--export-shot")
}

pub fn parse(args: impl IntoIterator<Item = String>) -> Result<CaptureArgs, String> {
    let mut args = args.into_iter();
    let mut scene = None;
    let mut target: Option<(Mode, PathBuf)> = None;
    let mut yaw = crate::camera::preset_angles(crate::camera::CameraPreset::Iso).0;
    let mut pitch = crate::camera::preset_angles(crate::camera::CameraPreset::Iso).1;
    let mut theme = ThemePref::Light;
    let mut size = UVec2::new(1280, 832);
    let mut framing = Framing::default();
    let mut focus = None;
    let mut zoom = None;
    let mut shot = ShotParams::default();
    let mut window_flags = Vec::new();
    let mut shot_flags = Vec::new();

    while let Some(arg) = args.next() {
        if !arg.starts_with("--") {
            if scene.replace(PathBuf::from(&arg)).is_some() {
                return Err(format!("more than one scene given: {arg}"));
            }
            continue;
        }
        let flag = arg.as_str();
        if flag == "--flip" {
            shot.gradient_flip = true;
            shot_flags.push(arg);
            continue;
        }
        let value = args.next().ok_or_else(|| format!("{flag} needs a value"))?;
        let v = value.as_str();
        match flag {
            "--capture" | "--export-shot" => {
                let mode = if flag == "--capture" {
                    Mode::Window
                } else {
                    Mode::Shot
                };
                if target.replace((mode, PathBuf::from(v))).is_some() {
                    return Err("give one of --capture or --export-shot".into());
                }
            }
            "--yaw" => yaw = number(flag, v)?.to_radians(),
            "--theme" => {
                theme = match v {
                    "light" => ThemePref::Light,
                    "dark" => ThemePref::Dark,
                    _ => return Err(format!("--theme takes light or dark, not {v}")),
                };
                window_flags.push(arg);
            }
            "--pitch" | "--size" | "--fill" | "--mass" | "--nudge" | "--focus"
            | "--focus-voxel" | "--zoom" => {
                match flag {
                    "--pitch" => pitch = number(flag, v)?.to_radians(),
                    "--size" => {
                        let (w, h) = v
                            .split_once('x')
                            .ok_or_else(|| format!("--size takes WxH, not {v}"))?;
                        size = UVec2::new(whole(flag, w)?, whole(flag, h)?);
                    }
                    "--fill" => framing.fill = number(flag, v)?,
                    "--mass" => framing.mass = number(flag, v)?,
                    "--nudge" => {
                        let [x, y] = numbers(flag, v)?;
                        framing.nudge = Vec2::new(x, y);
                    }
                    "--focus" => focus = Some(Focus::Fraction(Vec3::from(numbers(flag, v)?))),
                    "--focus-voxel" => {
                        let [x, y, z] = numbers(flag, v)?;
                        focus = Some(Focus::Voxel(Vec3::new(x, y, z).round().as_ivec3()));
                    }
                    _ => zoom = Some(number(flag, v)?),
                }
                window_flags.push(arg);
            }
            "--aspect" | "--style" | "--strength" | "--grain" | "--vignette" | "--saturation"
            | "--lift" | "--bg" => {
                match flag {
                    "--aspect" => {
                        shot.resolution = ResPreset::ALL
                            .into_iter()
                            .find(|r| r.label() == v)
                            .ok_or_else(|| {
                                format!("--aspect takes 4:3, 1:1, 16:9 or 3:4, not {v}")
                            })?
                    }
                    "--style" => {
                        shot.gradient = GradientMode::ALL
                            .into_iter()
                            .find(|g| g.label().eq_ignore_ascii_case(v))
                            .ok_or_else(|| {
                                format!("--style takes solid, vertical or radial, not {v}")
                            })?
                    }
                    "--strength" => shot.gradient_strength = number(flag, v)?,
                    "--grain" => shot.dither = number(flag, v)?,
                    "--vignette" => shot.vignette = number(flag, v)?,
                    "--saturation" => shot.saturation = number(flag, v)?,
                    "--lift" => shot.lift = number(flag, v)?,
                    _ => {
                        shot.bg_override = Some(
                            roxel::color_space::parse_hex(v)
                                .ok_or_else(|| format!("--bg takes a hex color, not {v}"))?,
                        )
                    }
                }
                shot_flags.push(arg);
            }
            _ => return Err(format!("unknown flag {flag}")),
        }
    }

    let scene = scene.ok_or("no scene given")?;
    let (mode, out) = target.ok_or("give --capture <out.png> or --export-shot <out.png>")?;
    match mode {
        Mode::Window if !shot_flags.is_empty() => {
            return Err(format!("{} only applies to --export-shot", shot_flags[0]));
        }
        Mode::Shot if !window_flags.is_empty() => {
            return Err(format!("{} only applies to --capture", window_flags[0]));
        }
        _ => {}
    }
    let detail = match (focus, zoom) {
        (Some(focus), zoom) => Some(Detail {
            focus,
            zoom: zoom.unwrap_or(2.0),
        }),
        (None, Some(_)) => return Err("--zoom needs --focus or --focus-voxel".into()),
        (None, None) => None,
    };
    if detail.is_some_and(|d| d.zoom <= 0.0) {
        return Err("--zoom must be above 0".into());
    }
    if size.x == 0 || size.y == 0 {
        return Err("--size must be above 0x0".into());
    }
    let limit = FRAC_PI_2 - 0.05;
    shot.yaw = yaw;
    Ok(CaptureArgs {
        scene,
        out,
        mode,
        yaw,
        pitch: pitch.clamp(-limit, limit),
        theme,
        size,
        framing,
        detail,
        shot,
    })
}

fn number(flag: &str, v: &str) -> Result<f32, String> {
    v.parse::<f32>()
        .ok()
        .filter(|n| n.is_finite())
        .ok_or_else(|| format!("{flag} takes a number, not {v}"))
}

fn whole(flag: &str, v: &str) -> Result<u32, String> {
    v.parse()
        .map_err(|_| format!("{flag} takes whole numbers, not {v}"))
}

fn numbers<const N: usize>(flag: &str, v: &str) -> Result<[f32; N], String> {
    let parts: Vec<f32> = v
        .split(',')
        .map(|p| number(flag, p.trim()))
        .collect::<Result<_, _>>()?;
    parts
        .try_into()
        .map_err(|_| format!("{flag} takes {N} comma-separated numbers, not {v}"))
}

/// Preferences for a capture run: defaults, so the user's settings don't leak
/// into screenshots, plus the requested theme.
pub fn preferences(args: &CaptureArgs) -> Preferences {
    Preferences {
        theme: args.theme,
        show_origin_axes: false,
        onboarding_seen: true,
        auto_update_check: false,
        reduce_motion: true,
        ..Preferences::default()
    }
}

// ---------------------------------------------------------------------------
// Framing
// ---------------------------------------------------------------------------

/// Camera angle and lens, plus the window it renders into (points).
#[derive(Clone, Copy, Debug)]
pub struct Lens {
    pub yaw: f32,
    pub pitch: f32,
    /// Vertical field of view, radians.
    pub fov: f32,
    pub window: Vec2,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Orbit {
    pub focus: Vec3,
    pub radius: f32,
}

/// Where the model should sit in the window, in points.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Composition {
    pub target: Vec2,
    /// Largest silhouette size; the model fills it on its limiting side.
    pub max_size: Vec2,
    pub mass: f32,
}

impl Composition {
    /// Center between the inspector and the tool rail, eased halfway toward
    /// the canvas center since the rail floats over the canvas.
    pub fn new(window: Vec2, canvas: (f32, f32), rail_left: f32, framing: Framing) -> Self {
        let (left, right) = canvas;
        let span = (rail_left - left).max(1.0);
        let x = ((left + rail_left) * 0.5 + (left + right) * 0.5) * 0.5;
        Self {
            target: Vec2::new(x, window.y * 0.5) + framing.nudge,
            max_size: Vec2::new(span * SIDE_FILL, window.y * framing.fill),
            mass: framing.mass,
        }
    }
}

fn rotation(lens: &Lens) -> Quat {
    Quat::from_axis_angle(Vec3::Y, lens.yaw) * Quat::from_axis_angle(Vec3::X, -lens.pitch)
}

/// Window position (points, y down) of world point `p`, or `None` behind the
/// camera. Matches `bevy_panorbit_camera`'s transform and Bevy's perspective.
pub fn project(lens: &Lens, orbit: &Orbit, p: Vec3) -> Option<Vec2> {
    let rot = rotation(lens);
    let eye = orbit.focus + rot * Vec3::Z * orbit.radius;
    let v = rot.inverse() * (p - eye);
    if v.z > -1e-3 {
        return None;
    }
    let t = (lens.fov * 0.5).tan();
    let aspect = lens.window.x / lens.window.y;
    let ndc = Vec2::new(v.x / (-v.z * t * aspect), v.y / (-v.z * t));
    Some(Vec2::new(
        (ndc.x + 1.0) * 0.5 * lens.window.x,
        (1.0 - ndc.y) * 0.5 * lens.window.y,
    ))
}

/// Occupied cells with at least one empty neighbour: the only ones that can
/// reach the silhouette.
pub fn surface_cells(grid: &VoxelGrid) -> Vec<IVec3> {
    const NEIGHBOURS: [IVec3; 6] = [
        IVec3::X,
        IVec3::NEG_X,
        IVec3::Y,
        IVec3::NEG_Y,
        IVec3::Z,
        IVec3::NEG_Z,
    ];
    grid.iter_occupied()
        .map(|(p, _)| p)
        .filter(|p| NEIGHBOURS.iter().any(|n| grid.get(*p + *n).is_none()))
        .collect()
}

#[derive(Clone, Copy, Debug)]
pub struct Silhouette {
    pub min: Vec2,
    pub max: Vec2,
    /// Centroid of the area the model covers on screen.
    pub centroid: Vec2,
}

impl Silhouette {
    fn anchor(&self, mass: f32) -> Vec2 {
        ((self.min + self.max) * 0.5).lerp(self.centroid, mass)
    }
}

/// The model's on-screen bounding box (from every cell corner) and the
/// centroid of its coverage (cell centers bucketed into voxel-sized squares,
/// so depth doesn't weigh in).
pub fn silhouette(lens: &Lens, orbit: &Orbit, cells: &[IVec3]) -> Option<Silhouette> {
    let mut min = Vec2::splat(f32::INFINITY);
    let mut max = Vec2::splat(f32::NEG_INFINITY);
    let px_per_unit = lens.window.y / (2.0 * orbit.radius * (lens.fov * 0.5).tan());
    let bucket = px_per_unit.max(0.5);
    let mut covered = std::collections::HashSet::new();
    for c in cells {
        let base = c.as_vec3();
        for i in 0..8 {
            let corner = base + Vec3::new((i & 1) as f32, ((i >> 1) & 1) as f32, (i >> 2) as f32);
            let s = project(lens, orbit, corner)?;
            min = min.min(s);
            max = max.max(s);
        }
        let s = project(lens, orbit, base + Vec3::splat(0.5))?;
        covered.insert((s / bucket).floor().as_ivec2());
    }
    if covered.is_empty() {
        return None;
    }
    let sum: Vec2 = covered.iter().map(|b| b.as_vec2() + Vec2::splat(0.5)).sum();
    Some(Silhouette {
        min,
        max,
        centroid: sum / covered.len() as f32 * bucket,
    })
}

/// Move the focus in the view plane so the model shifts by `delta` points.
fn pan(lens: &Lens, orbit: Orbit, delta: Vec2) -> Orbit {
    let rot = rotation(lens);
    let per_point = 2.0 * orbit.radius * (lens.fov * 0.5).tan() / lens.window.y;
    Orbit {
        focus: orbit.focus - rot * Vec3::X * delta.x * per_point
            + rot * Vec3::Y * delta.y * per_point,
        radius: orbit.radius,
    }
}

/// Focus and radius that put the model on `comp`: its anchor (bounding box
/// center blended with the coverage centroid) on the target, filling
/// `max_size` on its limiting side.
pub fn frame(lens: &Lens, comp: &Composition, cells: &[IVec3]) -> Option<Orbit> {
    let (min, max) = bounds(cells)?;
    let extent = (max - min).max_element();
    let mut orbit = Orbit {
        focus: (min + max) * 0.5,
        radius: (extent * 1.6).max(4.0),
    };
    for _ in 0..32 {
        let s = silhouette(lens, &orbit, cells)?;
        let size = (s.max - s.min).max(Vec2::splat(1e-3));
        let scale = (comp.max_size.x / size.x).min(comp.max_size.y / size.y);
        let delta = comp.target - s.anchor(comp.mass);
        if (scale - 1.0).abs() < 1e-4 && delta.length() < 0.05 {
            break;
        }
        orbit = pan(lens, orbit, delta);
        orbit.radius /= scale;
    }
    Some(orbit)
}

/// Zoom `orbit` in by `zoom` and pan so `point` lands on `target`.
pub fn zoom_on(lens: &Lens, orbit: Orbit, target: Vec2, point: Vec3, zoom: f32) -> Option<Orbit> {
    let mut orbit = Orbit {
        focus: orbit.focus,
        radius: orbit.radius / zoom,
    };
    for _ in 0..16 {
        let delta = target - project(lens, &orbit, point)?;
        if delta.length() < 0.05 {
            break;
        }
        orbit = pan(lens, orbit, delta);
    }
    Some(orbit)
}

/// World-space corners of the cells' bounding box (each cell spans p..p+1).
fn bounds(cells: &[IVec3]) -> Option<(Vec3, Vec3)> {
    let first = cells.first()?.as_vec3();
    let (min, max) = cells.iter().fold((first, first), |(lo, hi), c| {
        (lo.min(c.as_vec3()), hi.max(c.as_vec3()))
    });
    Some((min, max + Vec3::ONE))
}

pub fn focus_point(focus: Focus, cells: &[IVec3]) -> Option<Vec3> {
    match focus {
        Focus::Voxel(v) => Some(v.as_vec3() + Vec3::splat(0.5)),
        Focus::Fraction(f) => bounds(cells).map(|(min, max)| min + (max - min) * f),
    }
}

// ---------------------------------------------------------------------------
// Systems
// ---------------------------------------------------------------------------

#[derive(Default)]
enum Phase {
    #[default]
    Loading,
    Settling {
        layout: (Vec2, f32, f32, f32),
        frames: u32,
    },
    Shooting,
    ShotPending,
    Done(Result<(), String>),
    Exiting,
}

#[derive(Resource)]
pub struct Capture {
    args: CaptureArgs,
    phase: Phase,
    frames: u32,
    cells: Vec<IVec3>,
    orbit: Option<Orbit>,
}

impl Capture {
    pub fn new(args: CaptureArgs) -> Self {
        Self {
            args,
            phase: Phase::Loading,
            frames: 0,
            cells: Vec::new(),
            orbit: None,
        }
    }
}

#[derive(SystemParam)]
pub struct CaptureView<'w, 's> {
    viewport: Res<'w, ViewportRect>,
    anchors: Res<'w, OnboardingAnchors>,
    windows: Query<'w, 's, &'static Window, With<PrimaryWindow>>,
    cameras: Query<'w, 's, (&'static mut PanOrbitCamera, &'static Projection)>,
}

#[derive(SystemParam)]
pub struct ShotParts<'w> {
    request: ResMut<'w, ShotRequest>,
    panel: ResMut<'w, ShotPanel>,
    session: Res<'w, ShotSession>,
    png_writes: Res<'w, PngWrites>,
}

#[allow(clippy::too_many_arguments)]
pub fn capture_system(
    mut commands: Commands,
    mut cap: ResMut<Capture>,
    grid: Res<GridResource>,
    mut toasts: ResMut<Toasts>,
    guard: Res<UnsavedGuard>,
    mut view: CaptureView,
    mut shot: ShotParts,
    mut exit: MessageWriter<AppExit>,
) {
    if matches!(cap.phase, Phase::Exiting) {
        return;
    }
    cap.frames += 1;
    // Toasts queued at startup (a corrupt palettes file) say nothing about the
    // scene, so only errors from the first frame on fail the run.
    if cap.frames > 1
        && let Some(t) = toasts.0.iter().find(|t| t.kind == ToastKind::Error)
    {
        cap.phase = Phase::Done(Err(t.message.clone()));
    }
    toasts.0.clear();
    if cap.frames > FRAME_LIMIT && !matches!(cap.phase, Phase::Done(_)) {
        cap.phase = Phase::Done(Err(
            "timed out waiting for the scene to load (is it empty?)".into(),
        ));
    }

    let loaded = guard.is_idle() && !grid.dirty && grid.total_count > 0;
    match std::mem::take(&mut cap.phase) {
        Phase::Loading if !loaded => cap.phase = Phase::Loading,
        Phase::Loading if cap.args.mode == Mode::Shot => {
            shot.panel.params = cap.args.shot.clone();
            shot.request.0 = Some(cap.args.out.clone());
            cap.phase = Phase::ShotPending;
        }
        Phase::Loading => {
            cap.cells = surface_cells(&grid);
            cap.phase = settle(&mut cap, &mut view, None);
        }
        Phase::Settling { layout, frames } => {
            cap.phase = settle(&mut cap, &mut view, Some((layout, frames)));
            if let Phase::Settling { frames: 0, .. } = cap.phase {
                commands
                    .spawn(Screenshot::primary_window())
                    .observe(on_window_captured);
                cap.phase = Phase::Shooting;
            }
        }
        Phase::ShotPending => {
            let finished =
                shot.request.0.is_none() && shot.session.is_idle() && shot.png_writes.is_empty();
            cap.phase = if finished {
                Phase::Done(Ok(()))
            } else {
                Phase::ShotPending
            };
        }
        Phase::Done(result) => {
            match result {
                Ok(()) => {
                    println!("{}", cap.args.out.display());
                    exit.write(AppExit::Success);
                }
                Err(e) => {
                    eprintln!("roxel capture: {e}");
                    exit.write(AppExit::error());
                }
            }
            cap.phase = Phase::Exiting;
        }
        other => cap.phase = other,
    }
}

/// Frame the camera for the current layout and hold it there. Reframes and
/// restarts the countdown whenever the layout moves.
fn settle(
    cap: &mut Capture,
    view: &mut CaptureView,
    prev: Option<((Vec2, f32, f32, f32), u32)>,
) -> Phase {
    let Some(layout) = layout(view) else {
        return Phase::Loading;
    };
    let frames = match prev {
        Some((old, n)) if old == layout => n.saturating_sub(1),
        _ => {
            cap.orbit = compute_orbit(cap, view, layout);
            SETTLE_FRAMES
        }
    };
    let Some(orbit) = cap.orbit else {
        return Phase::Done(Err("couldn't frame the model".into()));
    };
    for (mut cam, _) in &mut view.cameras {
        cam.focus = orbit.focus;
        cam.target_focus = orbit.focus;
        cam.radius = Some(orbit.radius);
        cam.target_radius = orbit.radius;
        cam.yaw = Some(cap.args.yaw);
        cam.target_yaw = cap.args.yaw;
        cam.pitch = Some(cap.args.pitch);
        cam.target_pitch = cap.args.pitch;
        cam.zoom_lower_limit = cam.zoom_lower_limit.min(orbit.radius);
        cam.zoom_upper_limit = cam.zoom_upper_limit.map(|u| u.max(orbit.radius));
        cam.force_update = true;
    }
    Phase::Settling { layout, frames }
}

/// Window size, then the canvas's left and right edges and the tool rail's
/// left edge, all in window points.
fn layout(view: &CaptureView) -> Option<(Vec2, f32, f32, f32)> {
    let window = view.windows.single().ok()?;
    let size = Vec2::new(window.width(), window.height());
    let screen = view.viewport.screen?;
    let canvas = view.viewport.avail?;
    let rail = view.anchors.tool_rail?;
    let k = size.x / (screen.max.x - screen.min.x);
    Some((size, canvas.min.x * k, canvas.max.x * k, rail.min.x * k))
}

fn compute_orbit(
    cap: &Capture,
    view: &CaptureView,
    (size, left, right, rail): (Vec2, f32, f32, f32),
) -> Option<Orbit> {
    let (_, projection) = view.cameras.iter().next()?;
    let Projection::Perspective(persp) = projection else {
        return None;
    };
    let lens = Lens {
        yaw: cap.args.yaw,
        pitch: cap.args.pitch,
        fov: persp.fov,
        window: size,
    };
    let comp = Composition::new(size, (left, right), rail, cap.args.framing);
    let orbit = frame(&lens, &comp, &cap.cells)?;
    match cap.args.detail {
        None => Some(orbit),
        Some(d) => {
            let point = focus_point(d.focus, &cap.cells)?;
            zoom_on(&lens, orbit, comp.target, point, d.zoom)
        }
    }
}

fn on_window_captured(
    trigger: On<ScreenshotCaptured>,
    mut commands: Commands,
    mut cap: ResMut<Capture>,
) {
    commands.entity(trigger.entity).despawn();
    let path = cap.args.out.clone();
    let result = trigger
        .image
        .clone()
        .try_into_dynamic()
        .map_err(|e| format!("couldn't read the capture: {e}"))
        .and_then(|img| {
            roxel::io::atomic::write_with(&path, |w| {
                img.to_rgba8().write_to(w, image::ImageFormat::Png)
            })
            .map_err(|e| format!("couldn't write {}: {e}", path.display()))
        });
    cap.phase = Phase::Done(result);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(s: &str) -> Result<CaptureArgs, String> {
        parse(s.split_whitespace().map(String::from))
    }

    fn tower() -> Vec<IVec3> {
        let mut cells = Vec::new();
        for y in 0..12 {
            for x in 0..3 {
                for z in 0..3 {
                    cells.push(IVec3::new(x, y, z));
                }
            }
        }
        for x in -4..7 {
            for z in -4..7 {
                cells.push(IVec3::new(x, 0, z));
            }
        }
        cells
    }

    fn lens() -> Lens {
        Lens {
            yaw: 0.6,
            pitch: 0.4,
            fov: std::f32::consts::FRAC_PI_4,
            window: Vec2::new(1280.0, 832.0),
        }
    }

    #[test]
    fn parses_a_window_capture() {
        let a = args("scene.rox --capture out.png --yaw 90 --pitch 30 --theme dark --size 800x600")
            .unwrap();
        assert_eq!(a.scene, PathBuf::from("scene.rox"));
        assert_eq!(a.out, PathBuf::from("out.png"));
        assert_eq!(a.mode, Mode::Window);
        assert!((a.yaw - FRAC_PI_2).abs() < 1e-6);
        assert!((a.pitch - 30f32.to_radians()).abs() < 1e-6);
        assert_eq!(a.theme, ThemePref::Dark);
        assert_eq!(a.size, UVec2::new(800, 600));
        assert_eq!(a.detail, None);
    }

    #[test]
    fn defaults_to_the_isometric_preset_in_light() {
        let a = args("scene.rox --capture out.png").unwrap();
        let (yaw, pitch) = crate::camera::preset_angles(crate::camera::CameraPreset::Iso);
        assert_eq!((a.yaw, a.pitch), (yaw, pitch));
        assert_eq!(a.theme, ThemePref::Light);
        assert_eq!(a.size, UVec2::new(1280, 832));
        assert_eq!(a.framing, Framing::default());
    }

    #[test]
    fn parses_a_detail_shot() {
        let a = args("s.rox --capture o.png --focus 0.5,0.87,0.5 --zoom 2.5").unwrap();
        assert_eq!(
            a.detail,
            Some(Detail {
                focus: Focus::Fraction(Vec3::new(0.5, 0.87, 0.5)),
                zoom: 2.5
            })
        );
        let v = args("s.rox --capture o.png --focus-voxel 3,40,-2").unwrap();
        assert_eq!(
            v.detail,
            Some(Detail {
                focus: Focus::Voxel(IVec3::new(3, 40, -2)),
                zoom: 2.0
            })
        );
    }

    #[test]
    fn parses_shot_knobs() {
        let a = args(
            "s.rox --export-shot o.png --yaw 135 --aspect 3:4 --style radial --strength 0.6 \
             --grain 4 --vignette 0.3 --saturation 1.1 --lift 2 --bg #aabbcc --flip",
        )
        .unwrap();
        assert_eq!(a.mode, Mode::Shot);
        assert_eq!(a.shot.resolution, ResPreset::Portrait);
        assert_eq!(a.shot.gradient, GradientMode::Radial);
        assert_eq!(a.shot.gradient_strength, 0.6);
        assert_eq!(a.shot.dither, 4.0);
        assert_eq!(a.shot.vignette, 0.3);
        assert_eq!(a.shot.saturation, 1.1);
        assert_eq!(a.shot.lift, 2.0);
        assert_eq!(a.shot.bg_override, Some([0xaa, 0xbb, 0xcc]));
        assert!(a.shot.gradient_flip);
        assert!((a.shot.yaw - 135f32.to_radians()).abs() < 1e-6);
    }

    #[test]
    fn capture_needs_an_output_flag() {
        let s = |v: &[&str]| v.iter().map(|a| a.to_string()).collect::<Vec<_>>();
        assert!(!asks_for_capture(&s(&["scene.rox"])));
        assert!(!asks_for_capture(&s(&[])));
        assert!(asks_for_capture(&s(&["scene.rox", "--capture", "o.png"])));
        assert!(asks_for_capture(&s(&["--export-shot", "o.png"])));
    }

    #[test]
    fn rejects_bad_flags() {
        for bad in [
            "s.rox",
            "--capture o.png",
            "s.rox --capture o.png --export-shot p.png",
            "s.rox --capture o.png --style radial",
            "s.rox --export-shot o.png --pitch 10",
            "s.rox --capture o.png --zoom 2",
            "s.rox --capture o.png --yaw",
            "s.rox --capture o.png --yaw left",
            "s.rox --capture o.png --size 1280",
            "s.rox --capture o.png --theme blue",
            "s.rox --export-shot o.png --theme dark",
            "s.rox --capture o.png --focus 1,2",
            "s.rox --capture o.png --wat 1",
            "a.rox b.rox --capture o.png",
        ] {
            assert!(args(bad).is_err(), "accepted {bad}");
        }
    }

    #[test]
    fn capture_preferences_ignore_the_users_and_hide_chrome() {
        let a = args("s.rox --capture o.png --theme dark").unwrap();
        let p = preferences(&a);
        assert_eq!(p.theme, ThemePref::Dark);
        assert!(!p.show_origin_axes && p.onboarding_seen && !p.auto_update_check);
        assert!(p.reduce_motion);
        assert_eq!(p.ui_scale, Preferences::default().ui_scale);
    }

    #[test]
    fn projects_the_focus_to_the_window_center() {
        let orbit = Orbit {
            focus: Vec3::new(3.0, 2.0, -1.0),
            radius: 20.0,
        };
        let s = project(&lens(), &orbit, orbit.focus).unwrap();
        assert!((s - Vec2::new(640.0, 416.0)).length() < 1e-3, "{s:?}");
    }

    #[test]
    fn projection_matches_panorbit_camera_position() {
        // Isometric preset: the camera sits on the (1,1,1) diagonal, so the
        // point just past it toward the camera is behind the lens.
        let l = Lens {
            yaw: std::f32::consts::FRAC_PI_4,
            pitch: (1.0_f32 / 3.0_f32.sqrt()).asin(),
            ..lens()
        };
        let orbit = Orbit {
            focus: Vec3::ZERO,
            radius: 10.0,
        };
        assert!(project(&l, &orbit, Vec3::ONE.normalize() * 11.0).is_none());
        let up = project(&l, &orbit, Vec3::Y).unwrap();
        assert!((up.x - 640.0).abs() < 1e-3 && up.y < 416.0, "{up:?}");
    }

    #[test]
    fn frame_fills_and_centers_the_model() {
        let cells = tower();
        let l = lens();
        let comp = Composition::new(l.window, (245.0, 1280.0), 1170.0, Framing::default());
        let orbit = frame(&l, &comp, &cells).unwrap();
        let s = silhouette(&l, &orbit, &cells).unwrap();
        let size = s.max - s.min;
        let fills_h = (size.y - comp.max_size.y).abs() < 0.5;
        let fills_w = (size.x - comp.max_size.x).abs() < 0.5;
        assert!(fills_h || fills_w, "size {size:?} vs {:?}", comp.max_size);
        assert!(size.x <= comp.max_size.x + 0.5 && size.y <= comp.max_size.y + 0.5);
        assert!((s.anchor(comp.mass) - comp.target).length() < 0.5);
    }

    #[test]
    fn composition_targets_between_inspector_and_rail() {
        let c = Composition::new(
            Vec2::new(1280.0, 832.0),
            (245.0, 1280.0),
            1170.0,
            Framing::default(),
        );
        assert!(
            (c.target - Vec2::new(735.0, 416.0)).length() < 1e-3,
            "{:?}",
            c.target
        );
        assert!((c.max_size.y - 832.0 * 0.72).abs() < 1e-3);
        let nudged = Composition::new(
            Vec2::new(1280.0, 832.0),
            (245.0, 1280.0),
            1170.0,
            Framing {
                nudge: Vec2::new(10.0, -5.0),
                ..Framing::default()
            },
        );
        assert_eq!(nudged.target, c.target + Vec2::new(10.0, -5.0));
    }

    #[test]
    fn mass_pulls_the_anchor_toward_the_heavy_part() {
        let cells = tower();
        let l = lens();
        let orbit = frame(
            &l,
            &Composition::new(l.window, (245.0, 1280.0), 1170.0, Framing::default()),
            &cells,
        )
        .unwrap();
        let s = silhouette(&l, &orbit, &cells).unwrap();
        // The wide base sits low on screen, so mass pulls the anchor down.
        assert!(s.centroid.y > (s.min.y + s.max.y) * 0.5);
    }

    #[test]
    fn zoom_on_puts_the_focal_point_on_target() {
        let cells = tower();
        let l = lens();
        let comp = Composition::new(l.window, (245.0, 1280.0), 1170.0, Framing::default());
        let orbit = frame(&l, &comp, &cells).unwrap();
        let point = focus_point(Focus::Fraction(Vec3::new(0.5, 1.0, 0.5)), &cells).unwrap();
        let zoomed = zoom_on(&l, orbit, comp.target, point, 2.0).unwrap();
        assert!((zoomed.radius - orbit.radius / 2.0).abs() < 1e-4);
        let s = project(&l, &zoomed, point).unwrap();
        assert!((s - comp.target).length() < 0.1, "{s:?}");
    }

    #[test]
    fn focus_point_reads_fractions_of_the_bounds_and_voxel_centers() {
        let cells = vec![IVec3::new(0, 0, 0), IVec3::new(9, 4, 1)];
        assert_eq!(
            focus_point(Focus::Fraction(Vec3::new(0.5, 1.0, 0.0)), &cells),
            Some(Vec3::new(5.0, 5.0, 0.0))
        );
        assert_eq!(
            focus_point(Focus::Voxel(IVec3::new(2, 3, 4)), &cells),
            Some(Vec3::new(2.5, 3.5, 4.5))
        );
    }

    #[test]
    fn surface_cells_skip_buried_voxels() {
        let mut grid = VoxelGrid::default();
        for x in 0..3 {
            for y in 0..3 {
                for z in 0..3 {
                    grid.set(IVec3::new(x, y, z), Some([1, 2, 3, 255]));
                }
            }
        }
        let surface = surface_cells(&grid);
        assert_eq!(surface.len(), 26);
        assert!(!surface.contains(&IVec3::ONE));
    }
}
