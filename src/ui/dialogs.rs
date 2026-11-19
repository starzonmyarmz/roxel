use crate::GridResource;
use crate::icon::set_finder_icon;
use crate::snapshot::{CapturedPreview, SavePreviewCapture, SnapshotRequest};
use crate::ui::palette::{Palette, PaletteChoice, Palettes, WorkingPalette};
use crate::ui::toast::Toasts;
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::tasks::{AsyncComputeTaskPool, Task, block_on, futures_lite::future};
use bevy::window::PrimaryWindow;
use bevy_panorbit_camera::PanOrbitCamera;
use roxel::grid::{Color8, VoxelGrid};
use roxel::history::History;
use roxel::io;
use std::path::{Path, PathBuf};

pub enum DialogResult {
    OpenProject(PathBuf),
    SaveProject(PathBuf),
    ExportVox(PathBuf),
    ExportObj(PathBuf),
    ExportPng(PathBuf),
    ExportShot(PathBuf),
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
            | DialogResult::ExportShot(p)
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
/// modified when the live state id differs.
#[derive(Resource, Default)]
pub struct DocStatus {
    pub saved_state_id: u64,
}

impl DocStatus {
    /// Mark the current grid state as the saved baseline.
    pub fn mark_saved(&mut self, state_id: u64) {
        self.saved_state_id = state_id;
    }

    pub fn is_modified(&self, history: &History) -> bool {
        history.state_id() != self.saved_state_id
    }
}

/// Tracks a deferred save-with-preview flow. Set by `poll_dialogs_system`
/// when a `SaveProject` result arrives; consumed by
/// `process_save_preview_system` after the snapshot is captured.
#[derive(Resource, Default)]
pub struct SavePreviewState {
    pub path: Option<PathBuf>,
}

/// Something that would throw away the open document.
#[derive(Clone, Debug, PartialEq)]
pub enum GuardedAction {
    New,
    /// `None` shows the Open dialog; `Some` opens that file (Open Recent, Finder).
    Open(Option<PathBuf>),
    Quit,
}

/// What the user picked in the unsaved-changes confirm.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ConfirmChoice {
    Save,
    Discard,
    Cancel,
}

/// Routes every New / Open / Quit through one unsaved-changes check. Triggers
/// call [`UnsavedGuard::request`]; `resolve_unsaved_guard_system` runs the
/// action at once on a clean document, or raises `confirming` for the modal.
/// Choosing Save parks the action in `after_save` until the save finishes.
#[derive(Resource, Default)]
pub struct UnsavedGuard {
    requested: Option<GuardedAction>,
    pub confirming: Option<GuardedAction>,
    approved: Option<GuardedAction>,
    after_save: Option<GuardedAction>,
}

impl UnsavedGuard {
    /// Whether [`request`](Self::request) would accept a new action.
    pub fn is_idle(&self) -> bool {
        self.requested.is_none()
            && self.confirming.is_none()
            && self.approved.is_none()
            && self.after_save.is_none()
    }

    /// Ask to run `action`. Ignored while another guarded action is pending.
    pub fn request(&mut self, action: GuardedAction) {
        if self.is_idle() {
            self.requested = Some(action);
        }
    }

    /// Apply the user's answer to the confirm modal. Returns `true` when the
    /// caller should start a save.
    pub fn resolve_confirm(&mut self, choice: ConfirmChoice) -> bool {
        let Some(action) = self.confirming.take() else {
            return false;
        };
        match choice {
            ConfirmChoice::Save => {
                self.after_save = Some(action);
                true
            }
            ConfirmChoice::Discard => {
                self.approved = Some(action);
                false
            }
            ConfirmChoice::Cancel => false,
        }
    }

    /// Advance the guard one frame. `modified` is the document's dirty state,
    /// `dialog_busy` whether a file dialog or load is in flight, and `saving`
    /// whether a save is still running. Returns an action that may run now.
    pub fn step(
        &mut self,
        modified: bool,
        dialog_busy: bool,
        saving: bool,
    ) -> Option<GuardedAction> {
        if let Some(action) = self.approved.take() {
            return Some(action);
        }
        if self.after_save.is_some() {
            if saving {
                return None;
            }
            // Save finished. Still modified means it was cancelled or failed.
            let action = self.after_save.take();
            return if modified { None } else { action };
        }
        if dialog_busy {
            return None;
        }
        let action = self.requested.take()?;
        if modified {
            self.confirming = Some(action);
            None
        } else {
            Some(action)
        }
    }
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

/// Spawn an async "Import…" file dialog: one filter (`label`/`ext`) and a
/// picker, wrapping the chosen path with `wrap`. No-op while another dialog is
/// in flight. Shared by the command palette, the macOS native menu, and the
/// Win/Linux pill so every import path funnels through one spawn.
pub fn spawn_import(
    pending: &mut PendingDialog,
    label: &'static str,
    ext: &'static str,
    wrap: fn(PathBuf) -> DialogResult,
    start_dir: Option<PathBuf>,
) {
    if pending.is_active() {
        return;
    }
    pending.spawn(async move {
        new_dialog(&start_dir)
            .add_filter(label, &[ext])
            .pick_file()
            .await
            .map(|f| wrap(f.path().to_path_buf()))
    });
}

/// Spawn an async "Export…" save dialog: one filter (`label`/`ext`), a
/// suggested `default_name`, wrapping the chosen path with `wrap`. No-op while
/// another dialog is in flight. Shared sibling of [`spawn_import`].
pub fn spawn_export(
    pending: &mut PendingDialog,
    label: &'static str,
    ext: &'static str,
    default_name: &'static str,
    wrap: fn(PathBuf) -> DialogResult,
    start_dir: Option<PathBuf>,
) {
    if pending.is_active() {
        return;
    }
    pending.spawn(async move {
        new_dialog(&start_dir)
            .add_filter(label, &[ext])
            .set_file_name(default_name)
            .save_file()
            .await
            .map(|f| wrap(f.path().to_path_buf()))
    });
}

/// Spawn the "Export .ase…" save dialog for palette `name` with `colors`.
pub fn spawn_export_ase(
    pending: &mut PendingDialog,
    name: String,
    colors: Vec<[u8; 4]>,
    start_dir: Option<PathBuf>,
) {
    if pending.is_active() {
        return;
    }
    let default_filename = format!("{}.ase", super::palette::sanitize_filename(&name));
    pending.spawn(async move {
        new_dialog(&start_dir)
            .add_filter("Adobe Swatch Exchange", &["ase"])
            .set_file_name(&default_filename)
            .save_file()
            .await
            .map(|f| DialogResult::ExportAse(f.path().to_path_buf(), name, colors))
    });
}

/// Most-recent-first list of `.rox` paths the user has opened or saved.
/// Capped at [`roxel::io::recent::MAX_RECENT`]; persisted to
/// `dirs::config_dir()/roxel/recent.ron` whenever an entry is pushed.
#[derive(Resource, Default)]
pub struct RecentFiles(pub Vec<PathBuf>);

impl RecentFiles {
    pub fn loaded() -> Self {
        Self(roxel::io::recent::load())
    }
    pub fn push(&mut self, path: PathBuf) {
        roxel::io::recent::push(&mut self.0, path);
        roxel::io::recent::save(&self.0);
    }
    pub fn clear(&mut self) {
        self.0.clear();
        roxel::io::recent::save(&self.0);
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

#[cfg_attr(target_os = "macos", allow(dead_code))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileShortcut {
    New,
    Open,
    Save,
    SaveAs,
    Preferences,
}

/// The file chord pressed this frame, if any. `just` reports keys pressed
/// this frame; `ctrl` and `shift` report held modifiers.
#[cfg_attr(target_os = "macos", allow(dead_code))]
pub fn file_shortcut(
    ctrl: bool,
    shift: bool,
    just: impl Fn(KeyCode) -> bool,
) -> Option<FileShortcut> {
    if !ctrl {
        return None;
    }
    if just(KeyCode::KeyS) {
        return Some(if shift {
            FileShortcut::SaveAs
        } else {
            FileShortcut::Save
        });
    }
    if shift {
        return None;
    }
    [
        (KeyCode::KeyN, FileShortcut::New),
        (KeyCode::KeyO, FileShortcut::Open),
        (KeyCode::Comma, FileShortcut::Preferences),
    ]
    .into_iter()
    .find(|(key, _)| just(*key))
    .map(|(_, s)| s)
}

#[cfg(not(target_os = "macos"))]
#[derive(SystemParam)]
pub struct FileShortcutState<'w> {
    pending: ResMut<'w, PendingDialog>,
    current: Res<'w, CurrentProjectPath>,
    prefs: Res<'w, crate::theme::Preferences>,
    prefs_window: ResMut<'w, crate::theme::PreferencesWindow>,
}

/// Ctrl+N/O/S/Shift+S/, on Windows and Linux. macOS gets these from its
/// native menu accelerators.
#[cfg(not(target_os = "macos"))]
pub fn file_shortcut_system(
    keys: Res<ButtonInput<KeyCode>>,
    mut contexts: bevy_egui::EguiContexts,
    modal_active: Res<super::ModalActive>,
    mut guard: ResMut<UnsavedGuard>,
    mut file: FileShortcutState,
) {
    let FileShortcutState {
        pending,
        current,
        prefs,
        prefs_window,
    } = &mut file;
    let ctrl = keys.any_pressed([KeyCode::ControlLeft, KeyCode::ControlRight]);
    let shift = keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]);
    let Some(shortcut) = file_shortcut(ctrl, shift, |k| keys.just_pressed(k)) else {
        return;
    };
    let typing = contexts
        .ctx_mut()
        .map(|c| c.wants_keyboard_input())
        .unwrap_or(false);
    let closes_prefs = shortcut == FileShortcut::Preferences && prefs_window.open;
    if typing || (modal_active.0 && !closes_prefs) {
        return;
    }
    match shortcut {
        FileShortcut::New => guard.request(GuardedAction::New),
        FileShortcut::Open => guard.request(GuardedAction::Open(None)),
        FileShortcut::Save => spawn_save(pending, current, prefs.last_dir.clone()),
        FileShortcut::SaveAs => spawn_save_as(pending, current, prefs.last_dir.clone()),
        FileShortcut::Preferences => prefs_window.open = !prefs_window.open,
    }
}

fn file_label(p: &Path) -> String {
    p.file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("file")
        .to_string()
}

#[derive(SystemParam)]
pub struct DialogDoc<'w> {
    grid: ResMut<'w, GridResource>,
    history: ResMut<'w, History>,
    current_path: ResMut<'w, CurrentProjectPath>,
    doc: ResMut<'w, DocStatus>,
    recent_files: ResMut<'w, RecentFiles>,
    pending_import: ResMut<'w, PendingImport>,
}

#[derive(SystemParam)]
pub struct DialogSinks<'w> {
    palettes: ResMut<'w, Palettes>,
    palette_choice: ResMut<'w, PaletteChoice>,
    working_palette: Res<'w, WorkingPalette>,
    snapshot: ResMut<'w, SnapshotRequest>,
    shot: ResMut<'w, crate::shot::ShotRequest>,
    prefs: ResMut<'w, crate::theme::Preferences>,
    toasts: ResMut<'w, Toasts>,
}

#[derive(SystemParam)]
pub struct DialogView<'w, 's> {
    camera: Query<'w, 's, (&'static GlobalTransform, &'static Projection), With<PanOrbitCamera>>,
    windows: Query<'w, 's, &'static Window, With<PrimaryWindow>>,
}

pub fn poll_dialogs_system(
    mut pending: ResMut<PendingDialog>,
    doc_params: DialogDoc,
    sinks: DialogSinks,
    view: DialogView,
    mut save_preview_state: ResMut<SavePreviewState>,
    _capt_preview: ResMut<CapturedPreview>,
    mut save_preview_capture: ResMut<SavePreviewCapture>,
) {
    let DialogDoc {
        mut grid,
        mut history,
        mut current_path,
        mut doc,
        mut recent_files,
        mut pending_import,
    } = doc_params;
    let DialogSinks {
        mut palettes,
        mut palette_choice,
        working_palette,
        mut snapshot,
        mut shot,
        mut prefs,
        mut toasts,
    } = sinks;
    let DialogView { camera, windows } = view;
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
            Ok(project) => {
                if let Some(saved) = project.palette {
                    let before = palette_choice.0;
                    let added = crate::ui::palette::adopt_project_palette(
                        &mut palettes,
                        &mut palette_choice,
                        &working_palette,
                        saved,
                    );
                    let name = &palettes.0[palette_choice.0].name;
                    if added {
                        io::palettes::save(&palettes.0);
                        toasts.info(format!("Added palette {name}"));
                    } else if palette_choice.0 != before {
                        toasts.info(format!("Switched to palette {name}"));
                    }
                }
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
        Some(DialogResult::SaveProject(path)) => {
            // Defer the save until a model-framing preview snapshot is
            // captured. The actual save (with embedded PNG if available) runs
            // in `process_save_preview_system`.
            save_preview_state.path = Some(path.clone());
            save_preview_capture.0 = true;
            snapshot.0 = Some(path);
        }
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
        Some(DialogResult::ExportShot(path)) => {
            // Shot export is async — `shot_system` builds the scene, captures,
            // composites the watermark, writes the PNG, and posts its own toast.
            shot.0 = Some(path);
        }
        Some(DialogResult::ExportSvg(path)) => match (camera.single(), windows.single()) {
            (Ok((xform, projection)), Ok(window)) => {
                let viewport = Vec2::new(window.width(), window.height());
                let view = xform.to_matrix().inverse();
                let view_proj = projection.get_clip_from_view() * view;
                let camera_pos = xform.translation();
                match io::svg::export(&path, &grid, view, view_proj, camera_pos, viewport) {
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
        Some(
            r @ (DialogResult::ImportVox(_)
            | DialogResult::ImportQb(_)
            | DialogResult::ImportGox(_)),
        ) => {
            if finish_import(
                &r,
                &mut grid,
                &mut history,
                &mut palettes,
                &mut palette_choice,
                &mut toasts,
            ) {
                pending_import.0 = true;
            }
        }
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

/// A parsed model and, for `.vox`, the colors it uses.
type ImportedModel = (VoxelGrid, Vec<Color8>);

/// Parse a `.vox`/`.qb`/`.gox` import into a fresh grid, so a file that fails
/// halfway never touches the scene. Returns the format's extension and, for
/// `.vox`, the colors the model uses. `None` for any other result.
fn read_import(result: &DialogResult) -> Option<(&'static str, anyhow::Result<ImportedModel>)> {
    let mut scratch = VoxelGrid::default();
    let (ext, read) = match result {
        DialogResult::ImportVox(p) => ("vox", io::vox::import(p, &mut scratch)),
        DialogResult::ImportQb(p) => ("qb", io::qb::import(p, &mut scratch).map(|()| Vec::new())),
        DialogResult::ImportGox(p) => {
            ("gox", io::gox::import(p, &mut scratch).map(|()| Vec::new()))
        }
        _ => return None,
    };
    Some((ext, read.map(|colors| (scratch, colors))))
}

/// Merge `imported` into the scene as one undoable stroke. Returns how many
/// cells changed.
fn merge_import(grid: &mut VoxelGrid, history: &mut History, imported: &VoxelGrid) -> usize {
    history.begin();
    for (p, c) in imported.iter_occupied() {
        history.record(grid, p, Some(c));
    }
    let changed = history.current.as_ref().map_or(0, |s| s.deltas.len());
    history.end();
    changed
}

/// Finish a model import: merge it into the scene through `History` (so it can
/// be undone and marks the document modified), add a `.vox` file's colors as a
/// new palette, and toast. Returns whether anything was imported; a model that
/// changes no cells adds no palette and says so.
fn finish_import(
    result: &DialogResult,
    grid: &mut VoxelGrid,
    history: &mut History,
    palettes: &mut Palettes,
    palette_choice: &mut PaletteChoice,
    toasts: &mut Toasts,
) -> bool {
    let Some((ext, read)) = read_import(result) else {
        return false;
    };
    let path = result.path();
    let (imported, colors) = match read {
        Ok(r) => r,
        Err(e) => {
            toasts.error(format!("Import .{ext} failed: {e}"));
            return false;
        }
    };
    if merge_import(grid, history, &imported) == 0 {
        toasts.info(format!("Nothing to import from {}", file_label(path)));
        return false;
    }
    if !colors.is_empty() {
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Imported");
        let name = crate::ui::palette::unique_palette_name(&palettes.0, stem);
        palettes.0.push(Palette {
            name,
            colors,
            builtin: false,
        });
        palette_choice.0 = palettes.0.len() - 1;
        io::palettes::save(&palettes.0);
    }
    toasts.success(format!("Imported {}", file_label(path)));
    true
}

/// Completes a deferred save-with-preview. Runs every frame and returns
/// early until both [`SavePreviewState`] and [`CapturedPreview`] are
/// populated (the preview snapshot has been captured or gave up).
#[allow(clippy::too_many_arguments)]
pub fn process_save_preview_system(
    // Pins this system to the main thread so `set_finder_icon`'s
    // `MainThreadMarker` succeeds — without it Bevy schedules the system on a
    // worker thread, the marker is `None`, and the Finder-icon write is skipped.
    _marker: bevy::ecs::system::NonSendMarker,
    mut state: ResMut<SavePreviewState>,
    mut captured: ResMut<CapturedPreview>,
    mut snap_capture: ResMut<SavePreviewCapture>,
    grid: ResMut<GridResource>,
    history: ResMut<History>,
    mut current_path: ResMut<CurrentProjectPath>,
    mut doc: ResMut<DocStatus>,
    mut recent_files: ResMut<RecentFiles>,
    mut toasts: ResMut<Toasts>,
    palette: (Res<Palettes>, Res<PaletteChoice>, Res<WorkingPalette>),
) {
    let path = match state.path.clone() {
        Some(p) => p,
        None => return,
    };
    let png_bytes = match captured.0.take() {
        Some(b) => b,
        None => return,
    };

    // Preview captured (may be empty = model had no voxels).
    state.path = None;
    let preview = (!png_bytes.is_empty()).then_some(png_bytes.as_slice());
    let (palettes, choice, working) = palette;
    let active = crate::ui::palette::active_palette(&palettes, &choice, &working);
    let result = io::project::save(&path, &grid, active.as_ref(), preview);

    match result {
        Ok(()) => {
            doc.mark_saved(history.state_id());
            toasts.success(format!("Saved {}", file_label(&path)));
            if !png_bytes.is_empty() {
                set_finder_icon(&path, &png_bytes);
            }
            current_path.0 = Some(path.clone());
            recent_files.push(path);
        }
        Err(e) => toasts.error(format!("Save failed: {e}")),
    }
    snap_capture.0 = false;
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

    fn chord(ctrl: bool, shift: bool, key: KeyCode) -> Option<FileShortcut> {
        file_shortcut(ctrl, shift, |k| k == key)
    }

    #[test]
    fn file_shortcut_maps_ctrl_chords() {
        assert_eq!(chord(true, false, KeyCode::KeyN), Some(FileShortcut::New));
        assert_eq!(chord(true, false, KeyCode::KeyO), Some(FileShortcut::Open));
        assert_eq!(chord(true, false, KeyCode::KeyS), Some(FileShortcut::Save));
        assert_eq!(chord(true, true, KeyCode::KeyS), Some(FileShortcut::SaveAs));
        assert_eq!(
            chord(true, false, KeyCode::Comma),
            Some(FileShortcut::Preferences)
        );
    }

    #[test]
    fn file_shortcut_needs_ctrl_and_ignores_other_keys() {
        assert_eq!(chord(false, false, KeyCode::KeyS), None);
        assert_eq!(chord(false, false, KeyCode::KeyN), None);
        assert_eq!(chord(true, false, KeyCode::KeyK), None);
        assert_eq!(chord(true, true, KeyCode::KeyN), None);
    }

    #[test]
    fn dialog_result_path_parent_is_the_last_dir() {
        // poll_dialogs_system records this parent as Preferences.last_dir.
        let r = DialogResult::SaveProject(PathBuf::from("/tmp/scenes/a.rox"));
        assert_eq!(r.path().parent(), Some(std::path::Path::new("/tmp/scenes")));
    }

    fn quit() -> GuardedAction {
        GuardedAction::Quit
    }

    #[test]
    fn guard_runs_immediately_when_clean() {
        let mut g = UnsavedGuard::default();
        g.request(GuardedAction::New);
        assert_eq!(g.step(false, false, false), Some(GuardedAction::New));
        assert_eq!(g.step(false, false, false), None);
    }

    #[test]
    fn guard_confirms_when_modified_and_cancel_drops_it() {
        let mut g = UnsavedGuard::default();
        g.request(quit());
        assert_eq!(g.step(true, false, false), None);
        assert_eq!(g.confirming, Some(quit()));
        assert!(!g.resolve_confirm(ConfirmChoice::Cancel));
        assert_eq!(g.confirming, None);
        assert_eq!(g.step(true, false, false), None);
    }

    #[test]
    fn guard_discard_runs_the_action() {
        let mut g = UnsavedGuard::default();
        let open = GuardedAction::Open(Some(PathBuf::from("/a.rox")));
        g.request(open.clone());
        g.step(true, false, false);
        assert!(!g.resolve_confirm(ConfirmChoice::Discard));
        assert_eq!(g.step(true, false, false), Some(open));
    }

    #[test]
    fn guard_save_waits_for_the_save_then_runs() {
        let mut g = UnsavedGuard::default();
        g.request(quit());
        g.step(true, false, false);
        assert!(g.resolve_confirm(ConfirmChoice::Save));
        assert_eq!(g.step(true, true, true), None, "save still running");
        assert_eq!(g.step(false, false, false), Some(quit()));
    }

    #[test]
    fn guard_save_cancelled_or_failed_drops_the_action() {
        let mut g = UnsavedGuard::default();
        g.request(quit());
        g.step(true, false, false);
        g.resolve_confirm(ConfirmChoice::Save);
        assert_eq!(g.step(true, false, false), None);
        assert_eq!(g.step(false, false, false), None, "not run later either");
    }

    #[test]
    fn guard_waits_for_a_busy_dialog_before_handling_a_request() {
        let mut g = UnsavedGuard::default();
        g.request(GuardedAction::Open(None));
        assert_eq!(g.step(false, true, true), None);
        assert_eq!(g.step(false, false, false), Some(GuardedAction::Open(None)));
    }

    #[test]
    fn guard_is_busy_until_the_action_runs_or_is_cancelled() {
        let mut g = UnsavedGuard::default();
        assert!(g.is_idle());
        g.request(quit());
        assert!(!g.is_idle());
        g.step(true, false, false);
        assert!(!g.is_idle(), "confirming");
        g.resolve_confirm(ConfirmChoice::Save);
        assert!(!g.is_idle(), "waiting on save");
        g.step(false, false, false);
        assert!(g.is_idle());
    }

    #[test]
    fn guard_ignores_new_requests_while_one_is_pending() {
        let mut g = UnsavedGuard::default();
        g.request(GuardedAction::New);
        g.step(true, false, false);
        g.request(quit());
        assert!(!g.resolve_confirm(ConfirmChoice::Cancel));
        assert_eq!(g.step(true, false, false), None);
        assert_eq!(g.confirming, None);
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
        use roxel::grid::VoxelGrid;
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
    fn merge_import_is_one_undoable_stroke_that_keeps_existing_work() {
        use bevy::math::IVec3;
        let mut grid = VoxelGrid::default();
        let mut history = History::default();
        let doc = DocStatus::default();

        history.begin();
        history.record(&mut grid, IVec3::new(9, 0, 9), Some([1, 1, 1, 255]));
        history.end();
        let undo_before = history.undo.len();

        let mut imported = VoxelGrid::default();
        imported.set(IVec3::new(0, 0, 0), Some([2, 2, 2, 255]));
        imported.set(IVec3::new(1, 0, 0), Some([3, 3, 3, 255]));
        assert_eq!(merge_import(&mut grid, &mut history, &imported), 2);

        assert_eq!(history.undo.len(), undo_before + 1);
        assert!(doc.is_modified(&history));
        assert_eq!(grid.get(IVec3::new(9, 0, 9)), Some([1, 1, 1, 255]));
        assert_eq!(grid.count(), 3);

        history.undo(&mut grid);
        assert_eq!(grid.count(), 1);
        assert_eq!(grid.get(IVec3::new(0, 0, 0)), None);
    }

    #[test]
    fn import_that_changes_nothing_says_so_and_adds_no_palette() {
        use bevy::math::IVec3;
        let mut grid = VoxelGrid::default();
        grid.set(IVec3::ZERO, Some([10, 20, 30, 255]));
        let path = std::env::temp_dir().join(format!("roxel-test-{}-same.vox", std::process::id()));
        io::vox::export(&path, &grid).unwrap();

        let mut history = History::default();
        let mut palettes = Palettes::default();
        let palettes_before = palettes.0.len();
        let mut choice = PaletteChoice::default();
        let mut toasts = Toasts::default();
        let ok = finish_import(
            &DialogResult::ImportVox(path.clone()),
            &mut grid,
            &mut history,
            &mut palettes,
            &mut choice,
            &mut toasts,
        );
        let _ = std::fs::remove_file(&path);

        assert!(!ok);
        assert!(history.undo.is_empty());
        assert_eq!(palettes.0.len(), palettes_before);
        assert_eq!(toasts.0.len(), 1);
        assert!(toasts.0[0].message.starts_with("Nothing to import"));
    }

    #[test]
    fn failed_import_leaves_scene_and_history_untouched() {
        use bevy::math::IVec3;
        let mut grid = VoxelGrid::default();
        let mut history = History::default();
        history.begin();
        history.record(&mut grid, IVec3::new(0, 0, 0), Some([1, 1, 1, 255]));
        history.end();

        let path =
            std::env::temp_dir().join(format!("roxel-test-{}-truncated.qb", std::process::id()));
        std::fs::write(&path, [1u8, 0, 0, 0, 0, 0]).unwrap();
        let mut palettes = Palettes::default();
        let mut choice = PaletteChoice::default();
        let mut toasts = Toasts::default();
        let ok = finish_import(
            &DialogResult::ImportQb(path.clone()),
            &mut grid,
            &mut history,
            &mut palettes,
            &mut choice,
            &mut toasts,
        );
        let _ = std::fs::remove_file(&path);

        assert!(!ok);
        assert_eq!(grid.count(), 1);
        assert_eq!(history.undo.len(), 1);
        assert_eq!(toasts.0.len(), 1);
    }
}
