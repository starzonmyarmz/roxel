use crate::grid::VoxelGrid;
use crate::history::History;
use crate::io;
use crate::snapshot::SnapshotRequest;
use crate::ui::palette::{Palette, PaletteChoice, Palettes};
use bevy::prelude::*;
use bevy::tasks::{AsyncComputeTaskPool, Task, block_on, futures_lite::future};
use bevy::window::PrimaryWindow;
use bevy_panorbit_camera::PanOrbitCamera;
use std::path::PathBuf;

pub enum DialogResult {
    OpenProject(PathBuf),
    SaveProject(PathBuf),
    ExportVox(PathBuf),
    ExportObj(PathBuf),
    ExportFbx(PathBuf),
    ExportPng(PathBuf),
    ExportSvg(PathBuf),
    ImportAse(PathBuf),
    ExportAse(PathBuf, String, Vec<[u8; 4]>),
}

#[derive(Resource, Default)]
pub struct PendingDialog(pub Option<Task<Option<DialogResult>>>);

impl PendingDialog {
    pub fn is_active(&self) -> bool {
        self.0.is_some()
    }
    pub fn spawn<F>(&mut self, fut: F)
    where
        F: std::future::Future<Output = Option<DialogResult>> + Send + 'static,
    {
        self.0 = Some(AsyncComputeTaskPool::get().spawn(fut));
    }
}

pub fn poll_dialogs_system(
    mut pending: ResMut<PendingDialog>,
    mut grid: ResMut<VoxelGrid>,
    mut history: ResMut<History>,
    mut palettes: ResMut<Palettes>,
    mut palette_choice: ResMut<PaletteChoice>,
    mut snapshot: ResMut<SnapshotRequest>,
    camera: Query<(&GlobalTransform, &Projection), With<PanOrbitCamera>>,
    windows: Query<&Window, With<PrimaryWindow>>,
) {
    let Some(task) = pending.0.as_mut() else {
        return;
    };
    let Some(result) = block_on(future::poll_once(task)) else {
        return;
    };
    pending.0 = None;
    match result {
        Some(DialogResult::OpenProject(path)) => {
            if let Err(e) = io::project::load(&path, &mut grid) {
                eprintln!("Open failed: {e:?}");
            } else {
                history.undo.clear();
                history.redo.clear();
            }
        }
        Some(DialogResult::SaveProject(path)) => {
            if let Err(e) = io::project::save(&path, &grid) {
                eprintln!("Save failed: {e:?}");
            }
        }
        Some(DialogResult::ExportVox(path)) => {
            if let Err(e) = io::vox::export(&path, &grid) {
                eprintln!("Export .vox failed: {e:?}");
            }
        }
        Some(DialogResult::ExportObj(path)) => {
            if let Err(e) = io::obj::export(&path, &grid) {
                eprintln!("Export .obj failed: {e:?}");
            }
        }
        Some(DialogResult::ExportFbx(path)) => {
            if let Err(e) = io::fbx::export(&path, &grid) {
                eprintln!("Export .fbx failed: {e:?}");
            }
        }
        Some(DialogResult::ExportPng(path)) => {
            snapshot.0 = Some(path);
        }
        Some(DialogResult::ExportSvg(path)) => match (camera.single(), windows.single()) {
            (Ok((xform, projection)), Ok(window)) => {
                let viewport = Vec2::new(window.width(), window.height());
                if let Err(e) = io::svg::export(&path, &grid, xform, projection, viewport) {
                    eprintln!("Export .svg failed: {e:?}");
                }
            }
            (Err(e), _) => eprintln!("Export .svg failed: no camera found: {e:?}"),
            (_, Err(e)) => eprintln!("Export .svg failed: no window: {e:?}"),
        },
        Some(DialogResult::ImportAse(path)) => match io::ase::import(&path) {
            Ok((name, colors)) => {
                if colors.is_empty() {
                    eprintln!("Import .ase: no usable colors found");
                } else {
                    palettes.0.push(Palette {
                        name,
                        colors,
                        builtin: false,
                    });
                    palette_choice.0 = palettes.0.len() - 1;
                    io::palettes::save(&palettes.0);
                }
            }
            Err(e) => eprintln!("Import .ase failed: {e:?}"),
        },
        Some(DialogResult::ExportAse(path, name, colors)) => {
            if let Err(e) = io::ase::export(&path, &name, &colors) {
                eprintln!("Export .ase failed: {e:?}");
            }
        }
        None => {}
    }
}
