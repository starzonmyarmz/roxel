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

impl DialogResult {
    /// The filesystem path the user picked. Every variant carries one as its
    /// first field; used to remember the containing directory for next time.
    pub fn path(&self) -> &Path {
        match self {
            DialogResult::OpenProject(p)
            | DialogResult::SaveProject(p)
            | DialogResult::ExportVox(p)
            | DialogResult::ExportObj(p)
            | DialogResult::ExportPng(p)
            | DialogResult::ExportSvg(p)
            | DialogResult::ExportGltf(p)
            | DialogResult::ExportGox(p)
            | DialogResult::ImportVox(p)
            | DialogResult::ImportQb(p)
            | DialogResult::ImportGox(p)
            | DialogResult::ImportAse(p)
            | DialogResult::ExportAse(p, _, _) => p,
        }
    }
}

/// Build a fresh async file dialog rooted at the user's last-used directory
/// (`Preferences.last_dir`) when one is known, so Open/Save/Import/Export
/// don't restart at the home folder each session.
pub fn new_dialog(start_dir: &Option<PathBuf>) -> rfd::AsyncFileDialog {
    let dialog = rfd::AsyncFileDialog::new();
    match start_dir {
        Some(dir) => dialog.set_directory(dir),
        None => dialog,
    }
}

#[derive(Resource, Default)]
pub struct PendingDialog(pub Option<Task<Option<DialogResult>>>);

/// Path of the most recently saved or opened `.rox` project. `None` until
/// the user picks a target via Save As… or opens an existing project. A bare
/// "Save" reuses this path; a missing path falls through to Save As behavior.
#[derive(Resource, Default)]
pub struct CurrentProjectPath(pub Option<PathBuf>);

/// Tracks whether the open document has unsaved changes. `saved_state_id` is
/// the `History::state_id()` captured at the last save / open / new; the doc is
/// modified when the live state id differs. `forced_dirty` covers content that
/// has no clean baseline in the history stack — e.g. a `.vox`/`.qb`/`.gox`
/// import, which replaces the grid but leaves the undo stack empty (state id
/// `0`), so without this flag it would read as a clean, empty document.
#[derive(Resource, Default)]
pub struct DocStatus {
    pub saved_state_id: u64,
    pub forced_dirty: bool,
}

impl DocStatus {
    /// Mark the current grid state as the saved baseline (clears `forced_dirty`).
    pub fn mark_saved(&mut self, state_id: u64) {
        self.saved_state_id = state_id;
        self.forced_dirty = false;
    }

    pub fn is_modified(&self, history: &History) -> bool {
        self.forced_dirty || history.state_id() != self.saved_state_id
    }
}

/// A pending "Open project…" request, mirroring `NewProject`'s confirm flow.
/// Any Open trigger sets `requested`; `resolve_open_request_system` either
/// spawns the file dialog immediately (clean document) or raises `confirming`
/// so the discard-confirm modal can guard against losing unsaved work.
#[derive(Resource, Default)]
pub struct OpenRequest {
    pub requested: bool,
    pub confirming: bool,
}

/// Spawn the async "Open project…" file dialog rooted at `start_dir`. No-op
/// while another dialog is in flight. Shared by the request resolver and the
/// discard-confirm modal so every Open path funnels through one place.
pub fn spawn_open(pending: &mut PendingDialog, start_dir: Option<PathBuf>) {
    if pending.is_active() {
        return;
    }
    pending.spawn(async move {
        new_dialog(&start_dir)
            .add_filter("Roxel project", &["rox"])
            .pick_file()
            .await
            .map(|f| DialogResult::OpenProject(f.path().to_path_buf()))
    });
}

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
pub fn spawn_save_as(
    pending: &mut PendingDialog,
    current: &CurrentProjectPath,
    start_dir: Option<PathBuf>,
) {
    if pending.is_active() {
        return;
    }
    let suggested = save_as_default_name(current);
    pending.spawn(async move {
        new_dialog(&start_dir)
            .add_filter("Roxel project", &["rox"])
            .set_file_name(&suggested)
            .save_file()
            .await
            .map(|f| DialogResult::SaveProject(f.path().to_path_buf()))
    });
}

/// Save: writes to the last-saved path if one is known. Falls through to
/// Save As when the project has never been saved.
pub fn spawn_save(
    pending: &mut PendingDialog,
    current: &CurrentProjectPath,
    start_dir: Option<PathBuf>,
) {
    if pending.is_active() {
        return;
    }
    match current.0.clone() {
        Some(path) => {
            pending.spawn(async move { Some(DialogResult::SaveProject(path)) });
        }
        None => spawn_save_as(pending, current, start_dir),
    }
}

fn file_label(p: &Path) -> String {
    p.file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("file")
        .to_string()
}

#[allow(clippy::too_many_arguments)]
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
    mut doc: ResMut<DocStatus>,
    mut recent_files: ResMut<RecentFiles>,
    mut prefs: ResMut<crate::theme::Preferences>,
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

    // Remember the directory the user landed in so the next dialog opens there.
    if let Some(result) = result.as_ref()
        && let Some(parent) = result.path().parent()
        && prefs.last_dir.as_deref() != Some(parent)
    {
        prefs.last_dir = Some(parent.to_path_buf());
        crate::theme::save_preferences(&prefs);
    }
    match result {
        Some(DialogResult::OpenProject(path)) => match io::project::load(&path, &mut grid) {
            Ok(()) => {
                history.undo.clear();
                history.redo.clear();
                doc.mark_saved(history.state_id());
                pending_import.0 = true;
                toasts.success(format!("Opened {}", file_label(&path)));
                current_path.0 = Some(path.clone());
                recent_files.push(path);
            }
            Err(e) => toasts.error(format!("Open failed: {e}")),
        },
        Some(DialogResult::SaveProject(path)) => match io::project::save(&path, &grid) {
            Ok(()) => {
                doc.mark_saved(history.state_id());
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
                doc.forced_dirty = true;
                current_path.0 = None;
                pending_import.0 = true;
                toasts.success(format!("Imported {}", file_label(&path)));
            }
            Err(e) => toasts.error(format!("Import .gox failed: {e}")),
        },
        Some(DialogResult::ImportVox(path)) => match io::vox::import(&path, &mut grid) {
            Ok(()) => {
                history.undo.clear();
                history.redo.clear();
                doc.forced_dirty = true;
                current_path.0 = None;
                pending_import.0 = true;
                toasts.success(format!("Imported {}", file_label(&path)));
            }
            Err(e) => toasts.error(format!("Import .vox failed: {e}")),
        },
        Some(DialogResult::ImportQb(path)) => match io::qb::import(&path, &mut grid) {
            Ok(()) => {
                history.undo.clear();
                history.redo.clear();
                doc.forced_dirty = true;
                current_path.0 = None;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dialog_result_path_returns_embedded_path() {
        let p = PathBuf::from("/tmp/scenes/model.vox");
        assert_eq!(DialogResult::ExportVox(p.clone()).path(), p.as_path());
        assert_eq!(DialogResult::OpenProject(p.clone()).path(), p.as_path());
        assert_eq!(
            DialogResult::ExportAse(p.clone(), "pal".into(), vec![]).path(),
            p.as_path()
        );
    }

    #[test]
    fn dialog_result_path_parent_is_the_last_dir() {
        // poll_dialogs_system records this parent as Preferences.last_dir.
        let r = DialogResult::SaveProject(PathBuf::from("/tmp/scenes/a.rox"));
        assert_eq!(r.path().parent(), Some(std::path::Path::new("/tmp/scenes")));
    }

    #[test]
    fn doc_status_default_is_clean() {
        let doc = DocStatus::default();
        let history = History::default();
        assert!(!doc.is_modified(&history));
    }

    #[test]
    fn doc_status_modified_after_edit_clean_after_save() {
        use bevy::math::IVec3;
        let mut grid = VoxelGrid::default();
        let mut history = History::default();
        let mut doc = DocStatus::default();

        history.begin();
        history.record(&mut grid, IVec3::new(0, 0, 0), Some([1, 1, 1, 255]));
        history.end();
        assert!(doc.is_modified(&history), "an edit should mark modified");

        doc.mark_saved(history.state_id());
        assert!(!doc.is_modified(&history), "save should clear modified");

        // A further edit dirties again; undoing back to the saved state cleans.
        history.begin();
        history.record(&mut grid, IVec3::new(1, 0, 0), Some([2, 2, 2, 255]));
        history.end();
        assert!(doc.is_modified(&history));
        history.undo(&mut grid);
        assert!(!doc.is_modified(&history));
    }

    #[test]
    fn doc_status_forced_dirty_overrides_clean_state() {
        let history = History::default();
        // e.g. after a .vox import (empty undo stack but unsaved content).
        let mut doc = DocStatus {
            forced_dirty: true,
            ..Default::default()
        };
        assert!(doc.is_modified(&history));
        doc.mark_saved(history.state_id());
        assert!(!doc.is_modified(&history));
    }
}
