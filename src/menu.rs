use bevy::ecs::system::{NonSendMarker, SystemParam};
use bevy::prelude::*;
use muda::accelerator::{Accelerator, Code, Modifiers};
use muda::{AboutMetadata, Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu};
use std::collections::HashMap;

use crate::grid::{NewProject, VoxelGrid};
use crate::history::History;
use crate::theme::PreferencesWindow;
use crate::ui::{DialogResult, PendingDialog};

#[derive(Clone, Copy, Debug)]
pub enum MenuAction {
    NewProject,
    OpenProject,
    SaveProject,
    ExportVox,
    ExportObj,
    ExportFbx,
    ExportPng,
    ExportSvg,
    Undo,
    Redo,
    Preferences,
}

#[derive(Resource, Default)]
pub struct MenuQueue(pub Vec<MenuAction>);

pub struct MenuStore {
    _menu: Menu,
    actions: HashMap<String, MenuAction>,
    undo_item: MenuItem,
    redo_item: MenuItem,
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
    let save_item = MenuItem::new(
        "Save…",
        true,
        Some(Accelerator::new(Some(Modifiers::SUPER), Code::KeyS)),
    );

    let export_sub = Submenu::new("Export", true);
    let exp_vox = MenuItem::new("MagicaVoxel (.vox)…", true, None);
    let exp_obj = MenuItem::new("Wavefront (.obj)…", true, None);
    let exp_fbx = MenuItem::new("Autodesk (.fbx)…", true, None);
    let exp_png = MenuItem::new("Transparent PNG…", true, None);
    let exp_svg = MenuItem::new("SVG…", true, None);
    export_sub
        .append_items(&[&exp_vox, &exp_obj, &exp_fbx, &exp_png, &exp_svg])
        .expect("append export submenu");

    file.append_items(&[
        &new_item,
        &PredefinedMenuItem::separator(),
        &open_item,
        &save_item,
        &PredefinedMenuItem::separator(),
        &export_sub,
        &PredefinedMenuItem::separator(),
        &PredefinedMenuItem::close_window(None),
    ])
    .expect("append file menu");
    menu.append(&file).expect("append file submenu");

    actions.insert(new_item.id().0.clone(), MenuAction::NewProject);
    actions.insert(open_item.id().0.clone(), MenuAction::OpenProject);
    actions.insert(save_item.id().0.clone(), MenuAction::SaveProject);
    actions.insert(exp_vox.id().0.clone(), MenuAction::ExportVox);
    actions.insert(exp_obj.id().0.clone(), MenuAction::ExportObj);
    actions.insert(exp_fbx.id().0.clone(), MenuAction::ExportFbx);
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

    menu.init_for_nsapp();

    MenuStore {
        _menu: menu,
        actions,
        undo_item,
        redo_item,
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

#[derive(SystemParam)]
pub struct MenuActionParams<'w> {
    pub queue: ResMut<'w, MenuQueue>,
    pub pending: ResMut<'w, PendingDialog>,
    pub history: ResMut<'w, History>,
    pub grid: ResMut<'w, VoxelGrid>,
    pub new_project: ResMut<'w, NewProject>,
    pub prefs_window: ResMut<'w, PreferencesWindow>,
}

pub fn apply_menu_actions_system(mut p: MenuActionParams) {
    if p.queue.0.is_empty() {
        return;
    }
    let actions = std::mem::take(&mut p.queue.0);
    for action in actions {
        match action {
            MenuAction::NewProject => {
                p.new_project.picker_size = p.grid.size;
                p.new_project.dialog_open = true;
            }
            MenuAction::OpenProject => spawn_open(&mut p.pending),
            MenuAction::SaveProject => spawn_save(&mut p.pending),
            MenuAction::ExportVox => spawn_export_vox(&mut p.pending),
            MenuAction::ExportObj => spawn_export_obj(&mut p.pending),
            MenuAction::ExportFbx => spawn_export_fbx(&mut p.pending),
            MenuAction::ExportPng => spawn_export_png(&mut p.pending),
            MenuAction::ExportSvg => spawn_export_svg(&mut p.pending),
            MenuAction::Undo => p.history.undo(&mut p.grid),
            MenuAction::Redo => p.history.redo(&mut p.grid),
            MenuAction::Preferences => {
                p.prefs_window.open = !p.prefs_window.open;
            }
        }
    }
}

fn spawn_open(pending: &mut PendingDialog) {
    if pending.is_active() {
        return;
    }
    pending.spawn(async move {
        rfd::AsyncFileDialog::new()
            .add_filter("Roxel project", &["roxel"])
            .pick_file()
            .await
            .map(|f| DialogResult::OpenProject(f.path().to_path_buf()))
    });
}

fn spawn_save(pending: &mut PendingDialog) {
    if pending.is_active() {
        return;
    }
    pending.spawn(async move {
        rfd::AsyncFileDialog::new()
            .add_filter("Roxel project", &["roxel"])
            .set_file_name("scene.roxel")
            .save_file()
            .await
            .map(|f| DialogResult::SaveProject(f.path().to_path_buf()))
    });
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
