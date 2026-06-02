// Transparent PNG capture of the 3D view in two modes:
//
//   ExportPng  — saves a PNG to disk at the current camera transform (existing
//                 behaviour; triggered from File → Export → Transparent PNG).
//
//   SavePreview — encodes the PNG into memory and publishes the bytes on
//                 [`CapturedPreview`] so the project-save pipeline can embed
//                 them in the `.rox` file as a Finder-friendly thumbnail. The
//                 camera frames every occupied voxel (isometric, orthographic)
//                 instead of cloning the user's current view.
//
// Both modes spawn a one-shot Camera3d that renders into a texture with
// `ClearColorConfig::Custom(Color::NONE)` and capture the result through
// Bevy's Screenshot pipeline. The gizmo overlay camera writes to the window,
// not the offline texture, so it's excluded automatically. Floor dots, origin
// axes, the vignette, and the selection outline are suppressed for the
// snapshot frame via [`SnapshotInProgress`]. Tonemapping is forced off so the
// alpha channel from the transparent-clear background round-trips into the PNG
// instead of being clobbered to 1.0 by the tonemap pass.

use std::path::{Path, PathBuf};

use bevy::camera::{ClearColorConfig, RenderTarget, ScalingMode};
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::ecs::system::SystemParam;
use bevy::image::Image;
use bevy::prelude::*;
use bevy::render::render_resource::TextureFormat;
use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured};
use bevy::window::PrimaryWindow;
use bevy_panorbit_camera::PanOrbitCamera;

use crate::GridResource;
use crate::camera::fit_view;
use crate::ui::Toasts;

pub const PREVIEW_SIZE: u32 = 512;

#[derive(Component)]
pub struct SnapshotCamera;

#[derive(Resource, Default)]
pub struct SnapshotRequest(pub Option<PathBuf>);

#[derive(Resource, Default)]
pub struct SnapshotSession {
    camera: Option<Entity>,
}

/// Set for the duration of a snapshot render so gizmo/overlay systems can
/// early-return and stay out of the captured image.
#[derive(Resource, Default)]
pub struct SnapshotInProgress(pub bool);

/// When true the next snapshot is a save-preview: the camera frames the model
/// and the captured PNG bytes are stored on [`CapturedPreview`] instead of
/// being written to disk.
#[derive(Resource, Default)]
pub struct SavePreviewCapture(pub bool);

/// Published by the save-preview observer. Consumed by
/// [`crate::ui::dialogs::process_save_preview_system`].
#[derive(Resource, Default)]
pub struct CapturedPreview(pub Option<Vec<u8>>);

#[derive(SystemParam)]
pub struct SnapshotParams<'w> {
    request: ResMut<'w, SnapshotRequest>,
    session: ResMut<'w, SnapshotSession>,
    in_progress: ResMut<'w, SnapshotInProgress>,
}

#[allow(clippy::too_many_arguments)]
pub fn start_snapshot_system(
    mut commands: Commands,
    mut snap: SnapshotParams,
    save_preview: Res<SavePreviewCapture>,
    mut capt_preview: ResMut<CapturedPreview>,
    mut toasts: ResMut<Toasts>,
    windows: Query<&Window, With<PrimaryWindow>>,
    main_cam: Query<(&GlobalTransform, &Projection), With<PanOrbitCamera>>,
    grid: Res<GridResource>,
    mut images: ResMut<Assets<Image>>,
) {
    let Some(path) = snap.request.0.take() else {
        return;
    };
    if snap.session.camera.is_some() {
        toasts.error("Snapshot already in progress");
        return;
    }

    let is_preview = save_preview.0;

    // Determine resolution and camera transform.
    let (width, height, cam_xform, cam_projection) = if is_preview {
        // Model-framing isometric preview — don't use the main camera at all.
        let size = PREVIEW_SIZE;
        match fit_view(&grid) {
            Some((centroid, radius)) => {
                let offset = Vec3::new(1.0, 1.0, 1.0).normalize() * (radius / 3f32.sqrt()).max(4.0);
                let xform =
                    Transform::from_translation(centroid + offset).looking_at(centroid, Vec3::Y);
                let view_size = (radius * 1.5).max(8.0);
                let projection = Projection::Orthographic(OrthographicProjection {
                    scaling_mode: ScalingMode::FixedVertical {
                        viewport_height: view_size,
                    },
                    near: -1000.0,
                    far: 1000.0,
                    ..OrthographicProjection::default_3d()
                });
                (size, size, xform, projection)
            }
            None => {
                // Empty grid — save without preview. Signal the save system to
                // proceed without waiting.
                capt_preview.0 = Some(Vec::new());
                return;
            }
        }
    } else {
        // Export PNG — use the current camera transform and window size.
        let Ok(window) = windows.single() else { return };
        let Ok((xform, projection)) = main_cam.single() else {
            return;
        };
        (
            window.physical_width().max(1),
            window.physical_height().max(1),
            Transform::from(*xform),
            projection.clone(),
        )
    };

    let image = Image::new_target_texture(width, height, TextureFormat::Rgba8UnormSrgb, None);
    let handle = images.add(image);

    let cam = commands.spawn((
        Camera3d::default(),
        Camera {
            clear_color: ClearColorConfig::Custom(Color::NONE),
            order: 10,
            ..default()
        },
        RenderTarget::Image(handle.clone().into()),
        cam_xform,
        cam_projection,
        Tonemapping::None,
        SnapshotCamera,
    ));
    snap.session.camera = Some(cam.id());
    snap.in_progress.0 = true;

    if is_preview {
        // Preview mode: store bytes on CapturedPreview, don't write to disk.
        commands.spawn(Screenshot::image(handle.clone())).observe(
            move |trigger: On<ScreenshotCaptured>,
                  mut commands: Commands,
                  mut session: ResMut<SnapshotSession>,
                  mut in_progress: ResMut<SnapshotInProgress>,
                  mut captured: ResMut<CapturedPreview>| {
                let bytes = image_to_png_bytes(&trigger.image).unwrap_or_default();
                captured.0 = Some(bytes);
                if let Some(cam) = session.camera.take() {
                    commands.entity(cam).despawn();
                }
                in_progress.0 = false;
            },
        );
    } else {
        // Export mode: save PNG to disk (existing behaviour).
        let path_for_observer = path.clone();
        commands.spawn(Screenshot::image(handle.clone())).observe(
            move |trigger: On<ScreenshotCaptured>,
                  mut commands: Commands,
                  mut session: ResMut<SnapshotSession>,
                  mut in_progress: ResMut<SnapshotInProgress>,
                  mut toasts: ResMut<Toasts>| {
                match save_rgba_png(&path_for_observer, &trigger.image) {
                    Ok(()) => {
                        let label = path_for_observer
                            .file_name()
                            .and_then(|s| s.to_str())
                            .unwrap_or("image.png");
                        toasts.success(format!("Saved {label}"));
                    }
                    Err(e) => toasts.error(format!("Snapshot save failed: {e}")),
                }
                if let Some(cam) = session.camera.take() {
                    commands.entity(cam).despawn();
                }
                in_progress.0 = false;
            },
        );
    }
}

fn image_to_png_bytes(img: &Image) -> anyhow::Result<Vec<u8>> {
    let dyn_img = img
        .clone()
        .try_into_dynamic()
        .map_err(|e| anyhow::anyhow!("image format not convertible: {e:?}"))?;
    let rgba = dyn_img.into_rgba8();
    let mut buf = std::io::Cursor::new(Vec::new());
    rgba.write_to(&mut buf, image::ImageFormat::Png)?;
    Ok(buf.into_inner())
}

fn save_rgba_png(path: &Path, img: &Image) -> anyhow::Result<()> {
    let dyn_img = img
        .clone()
        .try_into_dynamic()
        .map_err(|e| anyhow::anyhow!("image format not convertible: {e:?}"))?;
    let rgba = dyn_img.into_rgba8();
    rgba.save_with_format(path, image::ImageFormat::Png)?;
    Ok(())
}
