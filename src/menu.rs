//! Menu actions shared by the macOS menu bar ([`native`]) and the Win/Linux
//! menu pill: both push [`MenuAction`]s onto [`MenuQueue`], and
//! [`apply_menu_actions_system`] runs them.

#[cfg(target_os = "macos")]
pub mod native;

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use std::path::PathBuf;

use crate::GridResource;
use crate::camera::{CameraPreset, PendingViewPreset};
use crate::theme::{Preferences, PreferencesWindow, save_preferences};
use crate::tools::{CurrentColor, ExtraColors, RecentColors, color_pool};
use crate::ui::{
    CommandPalette, CurrentProjectPath, DialogResult, GuardedAction, PendingDialog, RecentFiles,
    UnsavedGuard, spawn_export, spawn_import, spawn_save, spawn_save_as,
};
use roxel::color_space::ColorSpace;
use roxel::history::History;

#[derive(Clone, Copy, Debug)]
pub enum MenuAction {
    NewProject,
    OpenProject,
    OpenRecent(usize),
    ClearRecent,
    SaveProject,
    SaveProjectAs,
    ExportVox,
    ExportObj,
    ExportPng,
    ExportShot,
    ExportSvg,
    ExportGltf,
    ExportGox,
    ImportVox,
    ImportQb,
    ImportGox,
    Undo,
    Redo,
    FillSelection,
    DeleteSelectionContents,
    ClearSelection,
    Cut,
    Copy,
    Paste,
    DoubleDensity,
    HalveDensity,
    Preferences,
    Changelog,
    ShowOnboarding,
    CheckForUpdates,
    ShowCommandPalette,
    Quit,
    ViewPreset(CameraPreset),
    FrameView,
    SetColorSpace(ColorSpace),
    ToggleFloorGrid,
    ToggleOriginAxes,
}

const CHANGELOG_URL: &str = "https://github.com/starzonmyarmz/roxel/blob/main/CHANGELOG.md";

#[derive(Resource, Default)]
pub struct MenuQueue(pub Vec<MenuAction>);

pub fn recent_item_label(index: usize, path: &std::path::Path) -> String {
    let name = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or_else(|| path.to_str().unwrap_or("file"));
    format!("{}  {}", index + 1, name)
}

#[derive(SystemParam)]
pub struct MenuActionParams<'w> {
    pub queue: ResMut<'w, MenuQueue>,
    pub pending: ResMut<'w, PendingDialog>,
    pub history: ResMut<'w, History>,
    pub grid: ResMut<'w, GridResource>,
    pub guard: ResMut<'w, UnsavedGuard>,
    pub prefs_window: ResMut<'w, PreferencesWindow>,
    pub cmd_palette: ResMut<'w, CommandPalette>,
    pub current_path: Res<'w, CurrentProjectPath>,
    pub recent: ResMut<'w, RecentFiles>,
    pub view_preset: ResMut<'w, PendingViewPreset>,
    pub frame_view: ResMut<'w, crate::camera::PendingFrameView>,
    pub prefs: ResMut<'w, Preferences>,
    pub updater: ResMut<'w, crate::updater::UpdateCheck>,
    pub selection: ResMut<'w, crate::select::Selection>,
    pub color: Res<'w, CurrentColor>,
    pub extras: Res<'w, ExtraColors>,
    pub recent_colors: ResMut<'w, RecentColors>,
    pub clipboard: ResMut<'w, crate::clipboard::Clipboard>,
    pub toasts: ResMut<'w, crate::ui::Toasts>,
    pub onboarding: ResMut<'w, crate::onboarding::Onboarding>,
    pub shot_panel: ResMut<'w, crate::shot::ShotPanel>,
}

pub fn apply_menu_actions_system(mut p: MenuActionParams) {
    if p.queue.0.is_empty() {
        return;
    }
    let actions = std::mem::take(&mut p.queue.0);
    for action in actions {
        match action {
            MenuAction::NewProject => p.guard.request(GuardedAction::New),
            MenuAction::OpenProject => p.guard.request(GuardedAction::Open(None)),
            MenuAction::OpenRecent(i) => {
                if let Some(path) = p.recent.0.get(i).cloned() {
                    p.guard.request(GuardedAction::Open(Some(path)));
                }
            }
            MenuAction::Quit => p.guard.request(GuardedAction::Quit),
            MenuAction::ClearRecent => {
                p.recent.clear();
            }
            MenuAction::SaveProject => {
                spawn_save(&mut p.pending, &p.current_path, p.prefs.last_dir.clone())
            }
            MenuAction::SaveProjectAs => {
                spawn_save_as(&mut p.pending, &p.current_path, p.prefs.last_dir.clone())
            }
            MenuAction::ExportVox => spawn_export_vox(&mut p.pending, p.prefs.last_dir.clone()),
            MenuAction::ExportObj => spawn_export_obj(&mut p.pending, p.prefs.last_dir.clone()),
            MenuAction::ExportPng => spawn_export_png(&mut p.pending, p.prefs.last_dir.clone()),
            // Opens the tweak panel; its "Export…" button spawns the save dialog.
            MenuAction::ExportShot => {
                if p.grid.count() == 0 {
                    p.toasts.error("Nothing to export — the scene is empty");
                } else {
                    p.shot_panel.open_panel();
                }
            }
            MenuAction::ExportSvg => spawn_export_svg(&mut p.pending, p.prefs.last_dir.clone()),
            MenuAction::ExportGltf => spawn_export_gltf(&mut p.pending, p.prefs.last_dir.clone()),
            MenuAction::ExportGox => spawn_export_gox(&mut p.pending, p.prefs.last_dir.clone()),
            MenuAction::ImportVox => spawn_import_vox(&mut p.pending, p.prefs.last_dir.clone()),
            MenuAction::ImportQb => spawn_import_qb(&mut p.pending, p.prefs.last_dir.clone()),
            MenuAction::ImportGox => spawn_import_gox(&mut p.pending, p.prefs.last_dir.clone()),
            MenuAction::Undo => p.history.undo(&mut p.grid),
            MenuAction::Redo => p.history.redo(&mut p.grid),
            MenuAction::FillSelection => {
                if p.selection.aabb.is_some() {
                    let pool = color_pool(p.color.0, &p.extras.0);
                    let used = crate::select::recolor_selection(
                        &mut p.grid,
                        &mut p.history,
                        &p.selection,
                        &pool,
                    );
                    for c in used {
                        p.recent_colors.push(c);
                    }
                }
            }
            MenuAction::DeleteSelectionContents => {
                if p.selection.aabb.is_some() {
                    crate::select::clear_selection(&mut p.grid, &mut p.history, &p.selection);
                }
            }
            MenuAction::ClearSelection => p.selection.clear(),
            MenuAction::Copy => {
                if let Some(stamp) = crate::clipboard::copy_selection(&p.grid, &p.selection) {
                    let n = stamp.voxel_count();
                    p.clipboard.stamp = Some(stamp);
                    p.toasts.info(format!("Copied {n} voxels"));
                }
            }
            MenuAction::Cut => {
                if let Some(stamp) =
                    crate::clipboard::cut_selection(&mut p.grid, &mut p.history, &p.selection)
                {
                    let n = stamp.voxel_count();
                    p.clipboard.stamp = Some(stamp);
                    p.toasts.info(format!("Cut {n} voxels"));
                }
            }
            MenuAction::Paste => {
                if let Some(stamp) = p.clipboard.stamp.clone() {
                    crate::clipboard::execute_paste(
                        &mut p.grid,
                        &mut p.history,
                        &mut p.selection,
                        &mut p.toasts,
                        &stamp,
                        None,
                    );
                }
            }
            MenuAction::DoubleDensity => crate::resample::apply_resample(
                &mut p.grid,
                &mut p.history,
                &mut p.toasts,
                crate::resample::ResampleOp::Double,
            ),
            MenuAction::HalveDensity => crate::resample::apply_resample(
                &mut p.grid,
                &mut p.history,
                &mut p.toasts,
                crate::resample::ResampleOp::Halve,
            ),
            MenuAction::Preferences => {
                p.prefs_window.open = !p.prefs_window.open;
            }
            MenuAction::Changelog => crate::updater::open_url(CHANGELOG_URL),
            MenuAction::ShowOnboarding => {
                p.onboarding.start();
            }
            MenuAction::CheckForUpdates => {
                crate::updater::start_check(&mut p.updater, true);
            }
            MenuAction::ViewPreset(preset) => {
                p.view_preset.0 = Some(preset);
            }
            MenuAction::FrameView => {
                p.frame_view.0 = true;
            }
            MenuAction::ShowCommandPalette => {
                if p.cmd_palette.open {
                    p.cmd_palette.open = false;
                } else {
                    p.cmd_palette.open = true;
                    p.cmd_palette.search.clear();
                    p.cmd_palette.selected = 0;
                    p.cmd_palette.just_opened = true;
                }
            }
            MenuAction::SetColorSpace(space) => {
                if p.prefs.color_space != space {
                    p.prefs.color_space = space;
                    save_preferences(&p.prefs);
                }
            }
            MenuAction::ToggleFloorGrid => {
                p.prefs.show_floor_grid = !p.prefs.show_floor_grid;
                save_preferences(&p.prefs);
            }
            MenuAction::ToggleOriginAxes => {
                p.prefs.show_origin_axes = !p.prefs.show_origin_axes;
                save_preferences(&p.prefs);
            }
        }
    }
}

// Per-format menu items delegate to the shared `spawn_export`/`spawn_import`
// helpers (`ui::dialogs`) — same spawn path as the command palette and the
// Win/Linux pill, so a format's filter/extension lives in exactly one place.
fn spawn_export_vox(pending: &mut PendingDialog, start_dir: Option<PathBuf>) {
    spawn_export(
        pending,
        "MagicaVoxel",
        "vox",
        "model.vox",
        DialogResult::ExportVox,
        start_dir,
    );
}

fn spawn_export_obj(pending: &mut PendingDialog, start_dir: Option<PathBuf>) {
    spawn_export(
        pending,
        "Wavefront OBJ",
        "obj",
        "model.obj",
        DialogResult::ExportObj,
        start_dir,
    );
}

fn spawn_export_png(pending: &mut PendingDialog, start_dir: Option<PathBuf>) {
    spawn_export(
        pending,
        "PNG image",
        "png",
        "roxel.png",
        DialogResult::ExportPng,
        start_dir,
    );
}

fn spawn_export_svg(pending: &mut PendingDialog, start_dir: Option<PathBuf>) {
    spawn_export(
        pending,
        "SVG image",
        "svg",
        "roxel.svg",
        DialogResult::ExportSvg,
        start_dir,
    );
}

fn spawn_import_vox(pending: &mut PendingDialog, start_dir: Option<PathBuf>) {
    spawn_import(
        pending,
        "MagicaVoxel",
        "vox",
        DialogResult::ImportVox,
        start_dir,
    );
}

fn spawn_import_qb(pending: &mut PendingDialog, start_dir: Option<PathBuf>) {
    spawn_import(pending, "Qubicle", "qb", DialogResult::ImportQb, start_dir);
}

fn spawn_import_gox(pending: &mut PendingDialog, start_dir: Option<PathBuf>) {
    spawn_import(pending, "Goxel", "gox", DialogResult::ImportGox, start_dir);
}

fn spawn_export_gltf(pending: &mut PendingDialog, start_dir: Option<PathBuf>) {
    spawn_export(
        pending,
        "glTF binary",
        "glb",
        "model.glb",
        DialogResult::ExportGltf,
        start_dir,
    );
}

fn spawn_export_gox(pending: &mut PendingDialog, start_dir: Option<PathBuf>) {
    spawn_export(
        pending,
        "Goxel",
        "gox",
        "model.gox",
        DialogResult::ExportGox,
        start_dir,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn recent_label_is_one_indexed_with_filename() {
        let label = recent_item_label(0, Path::new("/home/user/scene.rox"));
        assert_eq!(label, "1  scene.rox");
        let label = recent_item_label(4, Path::new("/tmp/dragon.rox"));
        assert_eq!(label, "5  dragon.rox");
    }

    #[test]
    fn recent_label_strips_directories() {
        let label = recent_item_label(1, Path::new("/a/b/c/model.rox"));
        assert_eq!(label, "2  model.rox");
    }

    #[test]
    fn recent_label_bare_filename_no_dir() {
        let label = recent_item_label(0, Path::new("scene.rox"));
        assert_eq!(label, "1  scene.rox");
    }

    #[test]
    fn recent_label_falls_back_to_full_path_when_no_filename() {
        // A path ending in `..` has no `file_name()`; falls back to the
        // full path string rather than panicking.
        let label = recent_item_label(0, Path::new("/a/b/.."));
        assert_eq!(label, "1  /a/b/..");
    }
}
