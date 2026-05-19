use crate::grid::VoxelGrid;
use crate::history::History;
use crate::io;
use crate::snapshot::SnapshotRequest;
use crate::ui::palette::{Palette, PaletteChoice, Palettes};
use crate::ui::toast::Toasts;
use bevy::prelude::*;
use bevy::tasks::{AsyncComputeTaskPool, Task, block_on, futures_lite::future};
use bevy::window::PrimaryWindow;
use bevy_panorbit_camera::PanOrbitCamera;
use std::path::{Path, PathBuf};

pub enum DialogResult {
    OpenProject(PathBuf),
    SaveProject(PathBuf),
    ExportVox(PathBuf),
    ExportObj(PathBuf),
    ExportFbx(PathBuf),
    ExportPng(PathBuf),
    ExportSvg(PathBuf),
    ExportGltf(PathBuf),
    ExportGox(PathBuf),
    ImportVox(PathBuf),
    ImportQb(PathBuf),
    ImportGox(PathBuf),
    ImportAse(PathBuf),
    ExportAse(PathBuf, String, Vec<[u8; 4]>),
}

#[derive(Resource, Default)]
pub struct PendingDialog(pub Option<Task<Option<DialogResult>>>);

/// Path of the most recently saved or opened `.rox` project. `None` until
/// the user picks a target via Save As… or opens an existing project. A bare
/// "Save" reuses this path; a missing path falls through to Save As behavior.
#[derive(Resource, Default)]
pub struct CurrentProjectPath(pub Option<PathBuf>);

/// Most-recent-first list of `.rox` paths the user has opened or saved.
/// Capped at [`crate::io::recent::MAX_RECENT`]; persisted to
/// `dirs::config_dir()/roxel/recent.ron` whenever an entry is pushed.
#[derive(Resource, Default)]
pub struct RecentFiles(pub Vec<PathBuf>);

impl RecentFiles {
    pub fn loaded() -> Self {
        Self(crate::io::recent::load())
    }
    pub fn push(&mut self, path: PathBuf) {
        crate::io::recent::push(&mut self.0, path);
        crate::io::recent::save(&self.0);
    }
    pub fn clear(&mut self) {
        self.0.clear();
        crate::io::recent::save(&self.0);
    }
}

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

/// Signals that a non-`.rox` import just populated cells. Read and
/// cleared by `apply_import_system` in main.rs.
#[derive(Resource, Default)]
pub struct PendingImport(pub bool);

/// Suggested file name for the Save As dialog: reuse the current path's file
/// name when there is one, otherwise fall back to "scene.rox".
fn save_as_default_name(current: &CurrentProjectPath) -> String {
    current
        .0
        .as_ref()
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
        .unwrap_or("scene.rox")
        .to_string()
}

/// Save As: always opens the file dialog. Pre-fills with the current project
/// file name if known so the user can overwrite without retyping.
pub fn spawn_save_as(pending: &mut PendingDialog, current: &CurrentProjectPath) {
    if pending.is_active() {
        return;
    }
    let suggested = save_as_default_name(current);
    pending.spawn(async move {
        rfd::AsyncFileDialog::new()
            .add_filter("Roxel project", &["rox"])
            .set_file_name(&suggested)
            .save_file()
            .await
            .map(|f| DialogResult::SaveProject(f.path().to_path_buf()))
    });
}

/// Save: writes to the last-saved path if one is known. Falls through to
/// Save As when the project has never been saved.
pub fn spawn_save(pending: &mut PendingDialog, current: &CurrentProjectPath) {
    if pending.is_active() {
        return;
    }
    match current.0.clone() {
        Some(path) => {
            pending.spawn(async move { Some(DialogResult::SaveProject(path)) });
        }
        None => spawn_save_as(pending, current),
    }
}

fn file_label(p: &Path) -> String {
    p.file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("file")
        .to_string()
}

pub fn poll_dialogs_system(
    mut pending: ResMut<PendingDialog>,
    mut grid: ResMut<VoxelGrid>,
    mut history: ResMut<History>,
    mut palettes: ResMut<Palettes>,
    mut palette_choice: ResMut<PaletteChoice>,
    mut snapshot: ResMut<SnapshotRequest>,
    mut pending_import: ResMut<PendingImport>,
    mut toasts: ResMut<Toasts>,
    mut current_path: ResMut<CurrentProjectPath>,
    mut recent_files: ResMut<RecentFiles>,
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
        Some(DialogResult::OpenProject(path)) => match io::project::load(&path, &mut grid) {
            Ok(()) => {
                history.undo.clear();
                history.redo.clear();
                pending_import.0 = true;
                toasts.success(format!("Opened {}", file_label(&path)));
                current_path.0 = Some(path.clone());
                recent_files.push(path);
            }
            Err(e) => toasts.error(format!("Open failed: {e}")),
        },
        Some(DialogResult::SaveProject(path)) => match io::project::save(&path, &grid) {
            Ok(()) => {
                toasts.success(format!("Saved {}", file_label(&path)));
                current_path.0 = Some(path.clone());
                recent_files.push(path);
            }
            Err(e) => toasts.error(format!("Save failed: {e}")),
        },
        Some(DialogResult::ExportVox(path)) => match io::vox::export(&path, &grid) {
            Ok(()) => toasts.success(format!("Exported {}", file_label(&path))),
            Err(e) => toasts.error(format!("Export .vox failed: {e}")),
        },
        Some(DialogResult::ExportObj(path)) => match io::obj::export(&path, &grid) {
            Ok(()) => toasts.success(format!("Exported {}", file_label(&path))),
            Err(e) => toasts.error(format!("Export .obj failed: {e}")),
        },
        Some(DialogResult::ExportFbx(path)) => match io::fbx::export(&path, &grid) {
            Ok(()) => toasts.success(format!("Exported {}", file_label(&path))),
            Err(e) => toasts.error(format!("Export .fbx failed: {e}")),
        },
        Some(DialogResult::ExportPng(path)) => {
            // PNG export is async — the snapshot system finishes the save and
            // posts its own toast.
            snapshot.0 = Some(path);
        }
        Some(DialogResult::ExportSvg(path)) => match (camera.single(), windows.single()) {
            (Ok((xform, projection)), Ok(window)) => {
                let viewport = Vec2::new(window.width(), window.height());
                match io::svg::export(&path, &grid, xform, projection, viewport) {
                    Ok(()) => toasts.success(format!("Exported {}", file_label(&path))),
                    Err(e) => toasts.error(format!("Export .svg failed: {e}")),
                }
            }
            (Err(e), _) => toasts.error(format!("Export .svg failed: no camera ({e})")),
            (_, Err(e)) => toasts.error(format!("Export .svg failed: no window ({e})")),
        },
        Some(DialogResult::ExportGltf(path)) => match io::gltf::export(&path, &grid) {
            Ok(()) => toasts.success(format!("Exported {}", file_label(&path))),
            Err(e) => toasts.error(format!("Export .glb failed: {e}")),
        },
        Some(DialogResult::ExportGox(path)) => match io::gox::export(&path, &grid) {
            Ok(()) => toasts.success(format!("Exported {}", file_label(&path))),
            Err(e) => toasts.error(format!("Export .gox failed: {e}")),
        },
        Some(DialogResult::ImportGox(path)) => match io::gox::import(&path, &mut grid) {
            Ok(()) => {
                history.undo.clear();
                history.redo.clear();
                pending_import.0 = true;
                toasts.success(format!("Imported {}", file_label(&path)));
            }
            Err(e) => toasts.error(format!("Import .gox failed: {e}")),
        },
        Some(DialogResult::ImportVox(path)) => match io::vox::import(&path, &mut grid) {
            Ok(()) => {
                history.undo.clear();
                history.redo.clear();
                pending_import.0 = true;
                toasts.success(format!("Imported {}", file_label(&path)));
            }
            Err(e) => toasts.error(format!("Import .vox failed: {e}")),
        },
        Some(DialogResult::ImportQb(path)) => match io::qb::import(&path, &mut grid) {
            Ok(()) => {
                history.undo.clear();
                history.redo.clear();
                pending_import.0 = true;
                toasts.success(format!("Imported {}", file_label(&path)));
            }
            Err(e) => toasts.error(format!("Import .qb failed: {e}")),
        },
        Some(DialogResult::ImportAse(path)) => match io::ase::import(&path) {
            Ok((name, colors)) => {
                if colors.is_empty() {
                    toasts.error("Import .ase: no usable colors found");
                } else {
                    palettes.0.push(Palette {
                        name,
                        colors,
                        builtin: false,
                    });
                    palette_choice.0 = palettes.0.len() - 1;
                    io::palettes::save(&palettes.0);
                    toasts.success(format!("Imported {}", file_label(&path)));
                }
            }
            Err(e) => toasts.error(format!("Import .ase failed: {e}")),
        },
        Some(DialogResult::ExportAse(path, name, colors)) => {
            match io::ase::export(&path, &name, &colors) {
                Ok(()) => toasts.success(format!("Exported {}", file_label(&path))),
                Err(e) => toasts.error(format!("Export .ase failed: {e}")),
            }
        }
        None => {}
    }
}
