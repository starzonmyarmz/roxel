use bevy::ecs::system::{NonSendMarker, SystemParam};
use bevy::prelude::*;
use muda::accelerator::{Accelerator, Code, Modifiers};
use muda::{AboutMetadata, Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu};
use std::collections::HashMap;

use std::path::PathBuf;

use crate::grid::{NewProject, VoxelGrid};
use crate::history::History;
use crate::io::recent::MAX_RECENT;
use crate::theme::PreferencesWindow;
use crate::ui::{
    CommandPalette, CurrentProjectPath, DialogResult, PendingDialog, RecentFiles, spawn_save,
    spawn_save_as,
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
    ExportFbx,
    ExportPng,
    ExportSvg,
    ExportGltf,
    ExportGox,
    ImportVox,
    ImportQb,
    ImportGox,
    Undo,
    Redo,
    Preferences,
    Changelog,
    ShowCommandPalette,
}

const CHANGELOG_URL: &str = "https://github.com/starzonmyarmz/roxel/blob/main/CHANGELOG.md";

#[derive(Resource, Default)]
pub struct MenuQueue(pub Vec<MenuAction>);

pub struct MenuStore {
    _menu: Menu,
    actions: HashMap<String, MenuAction>,
    undo_item: MenuItem,
    redo_item: MenuItem,
    recent_sub: Submenu,
    recent_items: Vec<MenuItem>,
    clear_recent_item: MenuItem,
    recent_snapshot: Vec<PathBuf>,
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
    let exp_fbx = MenuItem::new("Autodesk (.fbx)…", true, None);
    let exp_gltf = MenuItem::new("glTF (.glb)…", true, None);
    let exp_png = MenuItem::new("Transparent PNG…", true, None);
    let exp_svg = MenuItem::new("SVG…", true, None);
    export_sub
        .append_items(&[
            &exp_vox, &exp_gox, &exp_obj, &exp_fbx, &exp_gltf, &exp_png, &exp_svg,
        ])
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
    actions.insert(exp_fbx.id().0.clone(), MenuAction::ExportFbx);
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
    edit.append_items(&[&undo_item, &redo_item])
        .expect("append edit menu");
    menu.append(&edit).expect("append edit submenu");

    actions.insert(undo_item.id().0.clone(), MenuAction::Undo);
    actions.insert(redo_item.id().0.clone(), MenuAction::Redo);

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
    let changelog_item = MenuItem::new("Changelog", true, None);
    help.append_items(&[&changelog_item])
        .expect("append help menu");
    menu.append(&help).expect("append help submenu");
    actions.insert(changelog_item.id().0.clone(), MenuAction::Changelog);

    menu.init_for_nsapp();

    MenuStore {
        _menu: menu,
        actions,
        undo_item,
        redo_item,
        recent_sub,
        recent_items,
        clear_recent_item,
        recent_snapshot: Vec::new(),
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

pub fn update_menu_enabled_system(
    _marker: NonSendMarker,
    store: Option<NonSend<MenuStore>>,
    history: Res<History>,
) {
    let Some(store) = store else { return };
    let undo_on = !history.undo.is_empty();
    let redo_on = !history.redo.is_empty();
    if store.undo_item.is_enabled() != undo_on {
        store.undo_item.set_enabled(undo_on);
    }
    if store.redo_item.is_enabled() != redo_on {
        store.redo_item.set_enabled(redo_on);
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
    pub recent: ResMut<'w, RecentFiles>,
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
            MenuAction::OpenProject => spawn_open(&mut p.pending),
            MenuAction::OpenRecent(i) => {
                if let Some(path) = p.recent.0.get(i).cloned() {
                    spawn_open_path(&mut p.pending, path);
                }
            }
            MenuAction::ClearRecent => {
                p.recent.clear();
            }
            MenuAction::SaveProject => spawn_save(&mut p.pending, &p.current_path),
            MenuAction::SaveProjectAs => spawn_save_as(&mut p.pending, &p.current_path),
            MenuAction::ExportVox => spawn_export_vox(&mut p.pending),
            MenuAction::ExportObj => spawn_export_obj(&mut p.pending),
            MenuAction::ExportFbx => spawn_export_fbx(&mut p.pending),
            MenuAction::ExportPng => spawn_export_png(&mut p.pending),
            MenuAction::ExportSvg => spawn_export_svg(&mut p.pending),
            MenuAction::ExportGltf => spawn_export_gltf(&mut p.pending),
            MenuAction::ExportGox => spawn_export_gox(&mut p.pending),
            MenuAction::ImportVox => spawn_import_vox(&mut p.pending),
            MenuAction::ImportQb => spawn_import_qb(&mut p.pending),
            MenuAction::ImportGox => spawn_import_gox(&mut p.pending),
            MenuAction::Undo => p.history.undo(&mut p.grid),
            MenuAction::Redo => p.history.redo(&mut p.grid),
            MenuAction::Preferences => {
                p.prefs_window.open = !p.prefs_window.open;
            }
            MenuAction::Changelog => open_changelog(),
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
        }
    }
}

fn open_changelog() {
    let _ = std::process::Command::new("open")
        .arg(CHANGELOG_URL)
        .spawn();
}

fn spawn_open(pending: &mut PendingDialog) {
    if pending.is_active() {
        return;
    }
    pending.spawn(async move {
        rfd::AsyncFileDialog::new()
            .add_filter("Roxel project", &["rox"])
            .pick_file()
            .await
            .map(|f| DialogResult::OpenProject(f.path().to_path_buf()))
    });
}

fn spawn_open_path(pending: &mut PendingDialog, path: PathBuf) {
    if pending.is_active() {
        return;
    }
    pending.spawn(async move { Some(DialogResult::OpenProject(path)) });
}

fn spawn_export_vox(pending: &mut PendingDialog) {
    if pending.is_active() {
        return;
    }
    pending.spawn(async move {
        rfd::AsyncFileDialog::new()
            .add_filter("MagicaVoxel", &["vox"])
            .set_file_name("model.vox")
            .save_file()
            .await
            .map(|f| DialogResult::ExportVox(f.path().to_path_buf()))
    });
}

fn spawn_export_obj(pending: &mut PendingDialog) {
    if pending.is_active() {
        return;
    }
    pending.spawn(async move {
        rfd::AsyncFileDialog::new()
            .add_filter("Wavefront OBJ", &["obj"])
            .set_file_name("model.obj")
            .save_file()
            .await
            .map(|f| DialogResult::ExportObj(f.path().to_path_buf()))
    });
}

fn spawn_export_fbx(pending: &mut PendingDialog) {
    if pending.is_active() {
        return;
    }
    pending.spawn(async move {
        rfd::AsyncFileDialog::new()
            .add_filter("Autodesk FBX", &["fbx"])
            .set_file_name("model.fbx")
            .save_file()
            .await
            .map(|f| DialogResult::ExportFbx(f.path().to_path_buf()))
    });
}

fn spawn_export_png(pending: &mut PendingDialog) {
    if pending.is_active() {
        return;
    }
    pending.spawn(async move {
        rfd::AsyncFileDialog::new()
            .add_filter("PNG image", &["png"])
            .set_file_name("roxel.png")
            .save_file()
            .await
            .map(|f| DialogResult::ExportPng(f.path().to_path_buf()))
    });
}

fn spawn_export_svg(pending: &mut PendingDialog) {
    if pending.is_active() {
        return;
    }
    pending.spawn(async move {
        rfd::AsyncFileDialog::new()
            .add_filter("SVG image", &["svg"])
            .set_file_name("roxel.svg")
            .save_file()
            .await
            .map(|f| DialogResult::ExportSvg(f.path().to_path_buf()))
    });
}

fn spawn_import_vox(pending: &mut PendingDialog) {
    if pending.is_active() {
        return;
    }
    pending.spawn(async move {
        rfd::AsyncFileDialog::new()
            .add_filter("MagicaVoxel", &["vox"])
            .pick_file()
            .await
            .map(|f| DialogResult::ImportVox(f.path().to_path_buf()))
    });
}

fn spawn_import_qb(pending: &mut PendingDialog) {
    if pending.is_active() {
        return;
    }
    pending.spawn(async move {
        rfd::AsyncFileDialog::new()
            .add_filter("Qubicle", &["qb"])
            .pick_file()
            .await
            .map(|f| DialogResult::ImportQb(f.path().to_path_buf()))
    });
}

fn spawn_import_gox(pending: &mut PendingDialog) {
    if pending.is_active() {
        return;
    }
    pending.spawn(async move {
        rfd::AsyncFileDialog::new()
            .add_filter("Goxel", &["gox"])
            .pick_file()
            .await
            .map(|f| DialogResult::ImportGox(f.path().to_path_buf()))
    });
}

fn spawn_export_gltf(pending: &mut PendingDialog) {
    if pending.is_active() {
        return;
    }
    pending.spawn(async move {
        rfd::AsyncFileDialog::new()
            .add_filter("glTF binary", &["glb"])
            .set_file_name("model.glb")
            .save_file()
            .await
            .map(|f| DialogResult::ExportGltf(f.path().to_path_buf()))
    });
}

fn spawn_export_gox(pending: &mut PendingDialog) {
    if pending.is_active() {
        return;
    }
    pending.spawn(async move {
        rfd::AsyncFileDialog::new()
            .add_filter("Goxel", &["gox"])
            .set_file_name("model.gox")
            .save_file()
            .await
            .map(|f| DialogResult::ExportGox(f.path().to_path_buf()))
    });
}
