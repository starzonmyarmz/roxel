use bevy::ecs::system::{NonSendMarker, SystemParam};
use bevy::prelude::*;
use muda::accelerator::{Accelerator, Code, Modifiers};
use muda::{AboutMetadata, CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu};
use std::collections::HashMap;

use std::path::PathBuf;

use crate::camera::{CameraPreset, PendingViewPreset};
use crate::color_space::ColorSpace;
use crate::grid::{NewProject, VoxelGrid};
use crate::history::History;
use crate::io::recent::MAX_RECENT;
use crate::theme::{Preferences, PreferencesWindow, save_preferences};
use crate::tools::{CurrentColor, ExtraColors, RecentColors, color_pool};
use crate::ui::{
    CommandPalette, CurrentProjectPath, DialogResult, PendingDialog, RecentFiles, new_dialog,
    spawn_save, spawn_save_as,
};

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
    ViewPreset(CameraPreset),
    FrameView,
    SetColorSpace(ColorSpace),
    ToggleFloorGrid,
    ToggleOriginAxes,
}

const CHANGELOG_URL: &str = "https://github.com/starzonmyarmz/roxel/blob/main/CHANGELOG.md";

#[derive(Resource, Default)]
pub struct MenuQueue(pub Vec<MenuAction>);

pub struct MenuStore {
    _menu: Menu,
    actions: HashMap<String, MenuAction>,
    undo_item: MenuItem,
    redo_item: MenuItem,
    fill_selection_item: MenuItem,
    delete_selection_item: MenuItem,
    clear_selection_item: MenuItem,
    cut_item: MenuItem,
    copy_item: MenuItem,
    paste_item: MenuItem,
    double_density_item: MenuItem,
    halve_density_item: MenuItem,
    recent_sub: Submenu,
    recent_items: Vec<MenuItem>,
    clear_recent_item: MenuItem,
    recent_snapshot: Vec<PathBuf>,
    cs_items: Vec<CheckMenuItem>,
    floor_grid_item: CheckMenuItem,
    origin_axes_item: CheckMenuItem,
}

pub fn install_menu_system(world: &mut World, mut done: Local<bool>) {
    if *done {
        return;
    }
    let store = build_menu();
    world.insert_non_send_resource(store);
    *done = true;
}

fn build_menu() -> MenuStore {
    let menu = Menu::new();
    let mut actions: HashMap<String, MenuAction> = HashMap::new();

    let app_menu = Submenu::new("Roxel", true);
    let prefs_item = MenuItem::new(
        "Preferences…",
        true,
        Some(Accelerator::new(Some(Modifiers::SUPER), Code::Comma)),
    );
    actions.insert(prefs_item.id().0.clone(), MenuAction::Preferences);
    let cmd_palette_item = MenuItem::new(
        "Command Palette…",
        true,
        Some(Accelerator::new(Some(Modifiers::SUPER), Code::KeyK)),
    );
    actions.insert(
        cmd_palette_item.id().0.clone(),
        MenuAction::ShowCommandPalette,
    );
    let check_updates_item = MenuItem::new("Check for Updates…", true, None);
    actions.insert(
        check_updates_item.id().0.clone(),
        MenuAction::CheckForUpdates,
    );
    app_menu
        .append_items(&[
            &PredefinedMenuItem::about(
                Some("About Roxel"),
                Some(AboutMetadata {
                    name: Some("Roxel".into()),
                    version: Some(env!("CARGO_PKG_VERSION").into()),
                    copyright: Some("© 2026 Daniel Marino".into()),
                    ..Default::default()
                }),
            ),
            &check_updates_item,
            &PredefinedMenuItem::separator(),
            &cmd_palette_item,
            &prefs_item,
            &PredefinedMenuItem::separator(),
            &PredefinedMenuItem::services(None),
            &PredefinedMenuItem::separator(),
            &PredefinedMenuItem::hide(None),
            &PredefinedMenuItem::hide_others(None),
            &PredefinedMenuItem::show_all(None),
            &PredefinedMenuItem::separator(),
            &PredefinedMenuItem::quit(None),
        ])
        .expect("append app menu");
    menu.append(&app_menu).expect("append app submenu");

    let file = Submenu::new("File", true);
    let new_item = MenuItem::new(
        "New…",
        true,
        Some(Accelerator::new(Some(Modifiers::SUPER), Code::KeyN)),
    );
    let open_item = MenuItem::new(
        "Open…",
        true,
        Some(Accelerator::new(Some(Modifiers::SUPER), Code::KeyO)),
    );
    let recent_sub = Submenu::new("Open Recent", false);
    let recent_items: Vec<MenuItem> = (0..MAX_RECENT)
        .map(|_| MenuItem::new("", false, None))
        .collect();
    let clear_recent_item = MenuItem::new("Clear Menu", true, None);
    let save_item = MenuItem::new(
        "Save",
        true,
        Some(Accelerator::new(Some(Modifiers::SUPER), Code::KeyS)),
    );
    let save_as_item = MenuItem::new(
        "Save As…",
        true,
        Some(Accelerator::new(
            Some(Modifiers::SUPER | Modifiers::SHIFT),
            Code::KeyS,
        )),
    );

    let import_sub = Submenu::new("Import", true);
    let imp_vox = MenuItem::new("MagicaVoxel (.vox)…", true, None);
    let imp_qb = MenuItem::new("Qubicle (.qb)…", true, None);
    let imp_gox = MenuItem::new("Goxel (.gox)…", true, None);
    import_sub
        .append_items(&[&imp_vox, &imp_qb, &imp_gox])
        .expect("append import submenu");

    let export_sub = Submenu::new("Export", true);
    let exp_vox = MenuItem::new("MagicaVoxel (.vox)…", true, None);
    let exp_gox = MenuItem::new("Goxel (.gox)…", true, None);
    let exp_obj = MenuItem::new("Wavefront (.obj)…", true, None);
    let exp_gltf = MenuItem::new("glTF (.glb)…", true, None);
    let exp_png = MenuItem::new("Transparent PNG…", true, None);
    let exp_svg = MenuItem::new("SVG…", true, None);
    export_sub
        .append_items(&[&exp_vox, &exp_gox, &exp_obj, &exp_gltf, &exp_png, &exp_svg])
        .expect("append export submenu");

    file.append_items(&[
        &new_item,
        &PredefinedMenuItem::separator(),
        &open_item,
        &recent_sub,
        &save_item,
        &save_as_item,
        &PredefinedMenuItem::separator(),
        &import_sub,
        &export_sub,
        &PredefinedMenuItem::separator(),
        &PredefinedMenuItem::close_window(None),
    ])
    .expect("append file menu");
    menu.append(&file).expect("append file submenu");

    actions.insert(new_item.id().0.clone(), MenuAction::NewProject);
    actions.insert(open_item.id().0.clone(), MenuAction::OpenProject);
    for (i, item) in recent_items.iter().enumerate() {
        actions.insert(item.id().0.clone(), MenuAction::OpenRecent(i));
    }
    actions.insert(clear_recent_item.id().0.clone(), MenuAction::ClearRecent);
    actions.insert(save_item.id().0.clone(), MenuAction::SaveProject);
    actions.insert(save_as_item.id().0.clone(), MenuAction::SaveProjectAs);
    actions.insert(imp_vox.id().0.clone(), MenuAction::ImportVox);
    actions.insert(imp_qb.id().0.clone(), MenuAction::ImportQb);
    actions.insert(imp_gox.id().0.clone(), MenuAction::ImportGox);
    actions.insert(exp_vox.id().0.clone(), MenuAction::ExportVox);
    actions.insert(exp_gox.id().0.clone(), MenuAction::ExportGox);
    actions.insert(exp_obj.id().0.clone(), MenuAction::ExportObj);
    actions.insert(exp_gltf.id().0.clone(), MenuAction::ExportGltf);
    actions.insert(exp_png.id().0.clone(), MenuAction::ExportPng);
    actions.insert(exp_svg.id().0.clone(), MenuAction::ExportSvg);

    let edit = Submenu::new("Edit", true);
    let undo_item = MenuItem::new(
        "Undo",
        false,
        Some(Accelerator::new(Some(Modifiers::SUPER), Code::KeyZ)),
    );
    let redo_item = MenuItem::new(
        "Redo",
        false,
        Some(Accelerator::new(
            Some(Modifiers::SUPER | Modifiers::SHIFT),
            Code::KeyZ,
        )),
    );
    // Fill (F) and Delete (Backspace) carry native key-equivalents so the menu
    // shows the right-aligned shortcut. A plain key-equivalent is routed by
    // AppKit before the key reaches the winit view and is blind to egui text
    // focus, so `update_menu_enabled_system` disables these items whenever egui
    // wants the keyboard (hex field needs "f", etc.) — a disabled item won't
    // fire its equivalent. On Win/Linux there's no native menu, so `F` is owned
    // by `selection_key_action_system` instead. Clear is left unbound: Esc is
    // overloaded (modals, flyby, drag-cancel) and must stay owned by
    // tool_input_system.
    let fill_selection_item = MenuItem::new(
        "Fill Selection",
        false,
        Some(Accelerator::new(None, Code::KeyF)),
    );
    let delete_selection_item = MenuItem::new(
        "Delete Selection Contents",
        false,
        Some(Accelerator::new(None, Code::Backspace)),
    );
    let clear_selection_item = MenuItem::new("Clear Selection", false, None);
    let cut_item = MenuItem::new(
        "Cut",
        false,
        Some(Accelerator::new(Some(Modifiers::SUPER), Code::KeyX)),
    );
    let copy_item = MenuItem::new(
        "Copy",
        false,
        Some(Accelerator::new(Some(Modifiers::SUPER), Code::KeyC)),
    );
    let paste_item = MenuItem::new(
        "Paste",
        false,
        Some(Accelerator::new(Some(Modifiers::SUPER), Code::KeyV)),
    );
    let double_density_item = MenuItem::new("Double Density", false, None);
    let halve_density_item = MenuItem::new("Halve Density", false, None);
    edit.append_items(&[
        &undo_item,
        &redo_item,
        &PredefinedMenuItem::separator(),
        &fill_selection_item,
        &delete_selection_item,
        &clear_selection_item,
        &PredefinedMenuItem::separator(),
        &cut_item,
        &copy_item,
        &paste_item,
        &PredefinedMenuItem::separator(),
        &double_density_item,
        &halve_density_item,
    ])
    .expect("append edit menu");
    menu.append(&edit).expect("append edit submenu");

    actions.insert(undo_item.id().0.clone(), MenuAction::Undo);
    actions.insert(redo_item.id().0.clone(), MenuAction::Redo);
    actions.insert(
        fill_selection_item.id().0.clone(),
        MenuAction::FillSelection,
    );
    actions.insert(
        delete_selection_item.id().0.clone(),
        MenuAction::DeleteSelectionContents,
    );
    actions.insert(
        clear_selection_item.id().0.clone(),
        MenuAction::ClearSelection,
    );
    actions.insert(cut_item.id().0.clone(), MenuAction::Cut);
    actions.insert(copy_item.id().0.clone(), MenuAction::Copy);
    actions.insert(paste_item.id().0.clone(), MenuAction::Paste);
    actions.insert(
        double_density_item.id().0.clone(),
        MenuAction::DoubleDensity,
    );
    actions.insert(halve_density_item.id().0.clone(), MenuAction::HalveDensity);

    let view = Submenu::new("View", true);
    let frame_item = MenuItem::new(
        "Frame View",
        true,
        Some(Accelerator::new(Some(Modifiers::SUPER), Code::Digit0)),
    );
    let view_front = MenuItem::new(
        "Front",
        true,
        Some(Accelerator::new(Some(Modifiers::SUPER), Code::Digit1)),
    );
    let view_back = MenuItem::new(
        "Back",
        true,
        Some(Accelerator::new(
            Some(Modifiers::SUPER | Modifiers::SHIFT),
            Code::Digit1,
        )),
    );
    let view_right = MenuItem::new(
        "Right",
        true,
        Some(Accelerator::new(Some(Modifiers::SUPER), Code::Digit3)),
    );
    let view_left = MenuItem::new(
        "Left",
        true,
        Some(Accelerator::new(
            Some(Modifiers::SUPER | Modifiers::SHIFT),
            Code::Digit3,
        )),
    );
    let view_iso = MenuItem::new(
        "Isometric",
        true,
        Some(Accelerator::new(Some(Modifiers::SUPER), Code::Digit5)),
    );
    let view_top = MenuItem::new(
        "Top",
        true,
        Some(Accelerator::new(Some(Modifiers::SUPER), Code::Digit7)),
    );
    let floor_grid_item = CheckMenuItem::new("Floor Grid", true, true, None);
    let origin_axes_item = CheckMenuItem::new("Origin Axes", true, true, None);
    let color_space_sub = Submenu::new("Color Format", true);
    let cs_items: Vec<CheckMenuItem> = ColorSpace::ALL
        .iter()
        .map(|s| CheckMenuItem::new(s.label(), true, *s == ColorSpace::default(), None))
        .collect();
    for item in &cs_items {
        let _ = color_space_sub.append(item);
    }

    let camera_sub = Submenu::new("Camera", true);
    camera_sub
        .append_items(&[
            &frame_item,
            &PredefinedMenuItem::separator(),
            &view_front,
            &view_back,
            &view_right,
            &view_left,
            &view_top,
            &PredefinedMenuItem::separator(),
            &view_iso,
        ])
        .expect("append camera menu");

    view.append_items(&[
        &camera_sub,
        &PredefinedMenuItem::separator(),
        &floor_grid_item,
        &origin_axes_item,
        &PredefinedMenuItem::separator(),
        &color_space_sub,
    ])
    .expect("append view menu");
    menu.append(&view).expect("append view submenu");

    actions.insert(frame_item.id().0.clone(), MenuAction::FrameView);
    actions.insert(
        view_front.id().0.clone(),
        MenuAction::ViewPreset(CameraPreset::Front),
    );
    actions.insert(
        view_back.id().0.clone(),
        MenuAction::ViewPreset(CameraPreset::Back),
    );
    actions.insert(
        view_right.id().0.clone(),
        MenuAction::ViewPreset(CameraPreset::Right),
    );
    actions.insert(
        view_left.id().0.clone(),
        MenuAction::ViewPreset(CameraPreset::Left),
    );
    actions.insert(
        view_top.id().0.clone(),
        MenuAction::ViewPreset(CameraPreset::Top),
    );
    actions.insert(
        view_iso.id().0.clone(),
        MenuAction::ViewPreset(CameraPreset::Iso),
    );
    for (i, space) in ColorSpace::ALL.iter().enumerate() {
        actions.insert(
            cs_items[i].id().0.clone(),
            MenuAction::SetColorSpace(*space),
        );
    }
    actions.insert(floor_grid_item.id().0.clone(), MenuAction::ToggleFloorGrid);
    actions.insert(
        origin_axes_item.id().0.clone(),
        MenuAction::ToggleOriginAxes,
    );

    let window = Submenu::new("Window", true);
    window
        .append_items(&[
            &PredefinedMenuItem::minimize(None),
            &PredefinedMenuItem::maximize(None),
            &PredefinedMenuItem::separator(),
            &PredefinedMenuItem::bring_all_to_front(None),
        ])
        .expect("append window menu");
    menu.append(&window).expect("append window submenu");

    let help = Submenu::new("Help", true);
    let tour_item = MenuItem::new("Show Onboarding Tour…", true, None);
    let changelog_item = MenuItem::new("Changelog", true, None);
    help.append_items(&[&tour_item, &changelog_item])
        .expect("append help menu");
    menu.append(&help).expect("append help submenu");
    actions.insert(tour_item.id().0.clone(), MenuAction::ShowOnboarding);
    actions.insert(changelog_item.id().0.clone(), MenuAction::Changelog);

    menu.init_for_nsapp();

    MenuStore {
        _menu: menu,
        actions,
        undo_item,
        redo_item,
        fill_selection_item,
        delete_selection_item,
        clear_selection_item,
        cut_item,
        copy_item,
        paste_item,
        double_density_item,
        halve_density_item,
        recent_sub,
        recent_items,
        clear_recent_item,
        recent_snapshot: Vec::new(),
        cs_items,
        floor_grid_item,
        origin_axes_item,
    }
}

pub fn poll_menu_events_system(
    _marker: NonSendMarker,
    store: Option<NonSend<MenuStore>>,
    mut queue: ResMut<MenuQueue>,
) {
    let Some(store) = store else { return };
    let rx = MenuEvent::receiver();
    while let Ok(ev) = rx.try_recv() {
        if let Some(action) = store.actions.get(&ev.id.0) {
            queue.0.push(*action);
        }
    }
}

#[derive(SystemParam)]
pub struct MenuEnableState<'w> {
    history: Res<'w, History>,
    selection: Res<'w, crate::select::Selection>,
    select_state: Res<'w, crate::select::SelectState>,
    clipboard: Res<'w, crate::clipboard::Clipboard>,
    grid: Res<'w, VoxelGrid>,
    prefs: Res<'w, Preferences>,
}

pub fn update_menu_enabled_system(
    _marker: NonSendMarker,
    store: Option<NonSend<MenuStore>>,
    mut contexts: bevy_egui::EguiContexts,
    state: MenuEnableState,
) {
    let Some(store) = store else { return };
    // A focused egui text field (hex input, palette rename, command palette)
    // must keep its keystrokes — disable the plain-key selection items so
    // AppKit doesn't fire their F / Backspace equivalents over the field.
    let egui_wants = contexts
        .ctx_mut()
        .map(|c| c.wants_keyboard_input())
        .unwrap_or(false);
    let undo_on = !state.history.undo.is_empty();
    let redo_on = !state.history.redo.is_empty();
    let has_sel = state.selection.aabb.is_some();
    let has_clip = state.clipboard.has_stamp();
    let has_voxels = state.grid.count() > 0;
    // Idle gate mirrors selection_key_action_system: Backspace only deletes
    // when no select drag is mid-flight.
    let idle = state.select_state.phase == crate::select::SelectPhase::Idle;
    let fill_on = has_sel && !egui_wants;
    let delete_on = has_sel && idle && !egui_wants;
    if store.undo_item.is_enabled() != undo_on {
        store.undo_item.set_enabled(undo_on);
    }
    if store.redo_item.is_enabled() != redo_on {
        store.redo_item.set_enabled(redo_on);
    }
    if store.fill_selection_item.is_enabled() != fill_on {
        store.fill_selection_item.set_enabled(fill_on);
    }
    if store.delete_selection_item.is_enabled() != delete_on {
        store.delete_selection_item.set_enabled(delete_on);
    }
    if store.clear_selection_item.is_enabled() != has_sel {
        store.clear_selection_item.set_enabled(has_sel);
    }
    if store.cut_item.is_enabled() != has_sel {
        store.cut_item.set_enabled(has_sel);
    }
    if store.copy_item.is_enabled() != has_sel {
        store.copy_item.set_enabled(has_sel);
    }
    if store.paste_item.is_enabled() != has_clip {
        store.paste_item.set_enabled(has_clip);
    }
    if store.double_density_item.is_enabled() != has_voxels {
        store.double_density_item.set_enabled(has_voxels);
    }
    if store.halve_density_item.is_enabled() != has_voxels {
        store.halve_density_item.set_enabled(has_voxels);
    }
    if store.floor_grid_item.is_checked() != state.prefs.show_floor_grid {
        store
            .floor_grid_item
            .set_checked(state.prefs.show_floor_grid);
    }
    if store.origin_axes_item.is_checked() != state.prefs.show_origin_axes {
        store
            .origin_axes_item
            .set_checked(state.prefs.show_origin_axes);
    }
    for (i, space) in ColorSpace::ALL.iter().enumerate() {
        let on = *space == state.prefs.color_space;
        if store.cs_items[i].is_checked() != on {
            store.cs_items[i].set_checked(on);
        }
    }
}

pub fn update_recent_menu_system(
    _marker: NonSendMarker,
    store: Option<NonSendMut<MenuStore>>,
    recent: Res<RecentFiles>,
) {
    let Some(mut store) = store else { return };
    if store.recent_snapshot == recent.0 {
        return;
    }
    while store.recent_sub.remove_at(0).is_some() {}
    if recent.0.is_empty() {
        store.recent_sub.set_enabled(false);
    } else {
        for (i, path) in recent.0.iter().enumerate() {
            let slot = &store.recent_items[i];
            slot.set_text(recent_item_label(i, path));
            slot.set_enabled(true);
            let _ = store.recent_sub.append(slot);
        }
        let _ = store.recent_sub.append(&PredefinedMenuItem::separator());
        let _ = store.recent_sub.append(&store.clear_recent_item);
        store.recent_sub.set_enabled(true);
    }
    store.recent_snapshot = recent.0.clone();
}

fn recent_item_label(index: usize, path: &std::path::Path) -> String {
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
    pub grid: ResMut<'w, VoxelGrid>,
    pub new_project: ResMut<'w, NewProject>,
    pub prefs_window: ResMut<'w, PreferencesWindow>,
    pub cmd_palette: ResMut<'w, CommandPalette>,
    pub current_path: Res<'w, CurrentProjectPath>,
    pub open_request: ResMut<'w, crate::ui::OpenRequest>,
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
}

pub fn apply_menu_actions_system(mut p: MenuActionParams) {
    if p.queue.0.is_empty() {
        return;
    }
    let actions = std::mem::take(&mut p.queue.0);
    for action in actions {
        match action {
            MenuAction::NewProject => {
                p.new_project.dialog_open = true;
            }
            MenuAction::OpenProject => p.open_request.requested = true,
            MenuAction::OpenRecent(i) => {
                if let Some(path) = p.recent.0.get(i).cloned() {
                    spawn_open_path(&mut p.pending, path);
                }
            }
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
            MenuAction::Changelog => open_changelog(),
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

fn open_changelog() {
    let _ = std::process::Command::new("open")
        .arg(CHANGELOG_URL)
        .spawn();
}

fn spawn_open_path(pending: &mut PendingDialog, path: PathBuf) {
    if pending.is_active() {
        return;
    }
    pending.spawn(async move { Some(DialogResult::OpenProject(path)) });
}

fn spawn_export_vox(pending: &mut PendingDialog, start_dir: Option<PathBuf>) {
    if pending.is_active() {
        return;
    }
    pending.spawn(async move {
        new_dialog(&start_dir)
            .add_filter("MagicaVoxel", &["vox"])
            .set_file_name("model.vox")
            .save_file()
            .await
            .map(|f| DialogResult::ExportVox(f.path().to_path_buf()))
    });
}

fn spawn_export_obj(pending: &mut PendingDialog, start_dir: Option<PathBuf>) {
    if pending.is_active() {
        return;
    }
    pending.spawn(async move {
        new_dialog(&start_dir)
            .add_filter("Wavefront OBJ", &["obj"])
            .set_file_name("model.obj")
            .save_file()
            .await
            .map(|f| DialogResult::ExportObj(f.path().to_path_buf()))
    });
}

fn spawn_export_png(pending: &mut PendingDialog, start_dir: Option<PathBuf>) {
    if pending.is_active() {
        return;
    }
    pending.spawn(async move {
        new_dialog(&start_dir)
            .add_filter("PNG image", &["png"])
            .set_file_name("roxel.png")
            .save_file()
            .await
            .map(|f| DialogResult::ExportPng(f.path().to_path_buf()))
    });
}

fn spawn_export_svg(pending: &mut PendingDialog, start_dir: Option<PathBuf>) {
    if pending.is_active() {
        return;
    }
    pending.spawn(async move {
        new_dialog(&start_dir)
            .add_filter("SVG image", &["svg"])
            .set_file_name("roxel.svg")
            .save_file()
            .await
            .map(|f| DialogResult::ExportSvg(f.path().to_path_buf()))
    });
}

fn spawn_import_vox(pending: &mut PendingDialog, start_dir: Option<PathBuf>) {
    if pending.is_active() {
        return;
    }
    pending.spawn(async move {
        new_dialog(&start_dir)
            .add_filter("MagicaVoxel", &["vox"])
            .pick_file()
            .await
            .map(|f| DialogResult::ImportVox(f.path().to_path_buf()))
    });
}

fn spawn_import_qb(pending: &mut PendingDialog, start_dir: Option<PathBuf>) {
    if pending.is_active() {
        return;
    }
    pending.spawn(async move {
        new_dialog(&start_dir)
            .add_filter("Qubicle", &["qb"])
            .pick_file()
            .await
            .map(|f| DialogResult::ImportQb(f.path().to_path_buf()))
    });
}

fn spawn_import_gox(pending: &mut PendingDialog, start_dir: Option<PathBuf>) {
    if pending.is_active() {
        return;
    }
    pending.spawn(async move {
        new_dialog(&start_dir)
            .add_filter("Goxel", &["gox"])
            .pick_file()
            .await
            .map(|f| DialogResult::ImportGox(f.path().to_path_buf()))
    });
}

fn spawn_export_gltf(pending: &mut PendingDialog, start_dir: Option<PathBuf>) {
    if pending.is_active() {
        return;
    }
    pending.spawn(async move {
        new_dialog(&start_dir)
            .add_filter("glTF binary", &["glb"])
            .set_file_name("model.glb")
            .save_file()
            .await
            .map(|f| DialogResult::ExportGltf(f.path().to_path_buf()))
    });
}

fn spawn_export_gox(pending: &mut PendingDialog, start_dir: Option<PathBuf>) {
    if pending.is_active() {
        return;
    }
    pending.spawn(async move {
        new_dialog(&start_dir)
            .add_filter("Goxel", &["gox"])
            .set_file_name("model.gox")
            .save_file()
            .await
            .map(|f| DialogResult::ExportGox(f.path().to_path_buf()))
    });
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
