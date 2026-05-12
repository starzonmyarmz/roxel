// Transparent PNG export of the current 3D view.
//
// Spawns a one-shot Camera3d that copies the primary camera's transform and
// projection, renders into an Image at physical (HiDPI) window resolution
// with `ClearColorConfig::Custom(Color::NONE)`, then captures that image
// through Bevy's Screenshot pipeline. The ground plane is hidden for the
// duration of the capture; the gizmo overlay camera writes to the window,
// not the snapshot image, so it's excluded automatically.

use std::path::{Path, PathBuf};

use bevy::camera::{ClearColorConfig, RenderTarget};
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::image::Image;
use bevy::prelude::*;
use bevy::render::render_resource::TextureFormat;
use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured};
use bevy::window::PrimaryWindow;
use bevy_panorbit_camera::PanOrbitCamera;

#[derive(Component)]
pub struct GroundPlane;

#[derive(Component)]
pub struct WallPlane;

#[derive(Component)]
pub struct SnapshotCamera;

#[derive(Resource, Default)]
pub struct SnapshotRequest(pub Option<PathBuf>);

#[derive(Resource, Default)]
pub struct SnapshotSession {
    camera: Option<Entity>,
    hidden: Vec<Entity>,
}

pub fn start_snapshot_system(
    mut commands: Commands,
    mut request: ResMut<SnapshotRequest>,
    mut session: ResMut<SnapshotSession>,
    windows: Query<&Window, With<PrimaryWindow>>,
    main_cam: Query<
        (&GlobalTransform, &Projection, Option<&Tonemapping>),
        With<PanOrbitCamera>,
    >,
    mut images: ResMut<Assets<Image>>,
    mut ground: Query<(Entity, &mut Visibility), With<GroundPlane>>,
    mut walls: Query<(Entity, &mut Visibility), (With<WallPlane>, Without<GroundPlane>)>,
) {
    let Some(path) = request.0.take() else { return };
    if session.camera.is_some() {
        eprintln!("Snapshot already in progress; ignoring request");
        return;
    }

    let Ok(window) = windows.single() else { return };
    let Ok((xform, projection, tonemapping)) = main_cam.single() else { return };

    let width = window.physical_width().max(1);
    let height = window.physical_height().max(1);

    let image = Image::new_target_texture(width, height, TextureFormat::Rgba8UnormSrgb, None);
    let handle = images.add(image);

    let mut cam = commands.spawn((
        Camera3d::default(),
        Camera {
            clear_color: ClearColorConfig::Custom(Color::NONE),
            order: 10,
            ..default()
        },
        RenderTarget::Image(handle.clone().into()),
        Transform::from(*xform),
        projection.clone(),
        SnapshotCamera,
    ));
    if let Some(tm) = tonemapping {
        cam.insert(*tm);
    }
    session.camera = Some(cam.id());

    session.hidden.clear();
    for (e, mut vis) in ground.iter_mut() {
        *vis = Visibility::Hidden;
        session.hidden.push(e);
    }
    for (e, mut vis) in walls.iter_mut() {
        if *vis != Visibility::Hidden {
            *vis = Visibility::Hidden;
            session.hidden.push(e);
        }
    }

    let path_for_observer = path.clone();
    commands.spawn(Screenshot::image(handle.clone())).observe(
        move |trigger: On<ScreenshotCaptured>,
              mut commands: Commands,
              mut session: ResMut<SnapshotSession>,
              mut vis: Query<&mut Visibility>| {
            if let Err(e) = save_rgba_png(&path_for_observer, &trigger.image) {
                eprintln!("Snapshot save failed: {e:?}");
            } else {
                info!("Snapshot saved to {}", path_for_observer.display());
            }
            if let Some(cam) = session.camera.take() {
                commands.entity(cam).despawn();
            }
            for e in session.hidden.drain(..) {
                if let Ok(mut v) = vis.get_mut(e) {
                    *v = Visibility::Inherited;
                }
            }
        },
    );
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
