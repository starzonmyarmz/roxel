//! The macOS menu bar, built with muda from [`super::menu_bar`]. Clicks and
//! key equivalents push their [`Action`] onto [`ActionQueue`].

use bevy::ecs::system::NonSendMarker;
use bevy::prelude::*;
use muda::accelerator::{Accelerator, Code, Modifiers};
use muda::{AboutMetadata, CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu};
use std::collections::HashMap;
use std::path::PathBuf;
use std::str::FromStr;

use super::{MenuEntry, SystemItem, menu_bar, recent_item_label};
use crate::actions::{Action, ActionQueue, ActionStateParams, Chord, Origin};
use crate::ui::RecentFiles;
use roxel::io::recent::MAX_RECENT;

enum NativeItem {
    /// `label` mirrors the item text so per-frame label sync skips unchanged items.
    Plain {
        item: MenuItem,
        label: String,
    },
    Check(CheckMenuItem),
}

pub struct MenuStore {
    _menu: Menu,
    actions: HashMap<String, Action>,
    items: Vec<(Action, NativeItem)>,
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

fn accelerator(chord: Chord) -> Option<Accelerator> {
    let code = Code::from_str(&format!("{:?}", chord.key)).ok()?;
    let mut mods = Modifiers::empty();
    if chord.cmd {
        mods |= Modifiers::SUPER;
    }
    if chord.shift {
        mods |= Modifiers::SHIFT;
    }
    Some(Accelerator::new((!mods.is_empty()).then_some(mods), code))
}

fn system_item(item: SystemItem) -> PredefinedMenuItem {
    match item {
        SystemItem::About => PredefinedMenuItem::about(
            Some("About Roxel"),
            Some(AboutMetadata {
                name: Some("Roxel".into()),
                version: Some(env!("CARGO_PKG_VERSION").into()),
                copyright: Some(super::COPYRIGHT.into()),
                ..Default::default()
            }),
        ),
        SystemItem::Services => PredefinedMenuItem::services(None),
        SystemItem::Hide => PredefinedMenuItem::hide(None),
        SystemItem::HideOthers => PredefinedMenuItem::hide_others(None),
        SystemItem::ShowAll => PredefinedMenuItem::show_all(None),
        SystemItem::CloseWindow => PredefinedMenuItem::close_window(None),
        SystemItem::Minimize => PredefinedMenuItem::minimize(None),
        SystemItem::Maximize => PredefinedMenuItem::maximize(None),
        SystemItem::BringAllToFront => PredefinedMenuItem::bring_all_to_front(None),
    }
}

struct Builder {
    actions: HashMap<String, Action>,
    items: Vec<(Action, NativeItem)>,
    recent_sub: Option<Submenu>,
}

impl Builder {
    fn fill(&mut self, sub: &Submenu, entries: &[MenuEntry]) {
        for entry in entries {
            match entry {
                MenuEntry::Item(action) => {
                    let spec = action.spec();
                    let accel = spec.shortcut.and_then(accelerator);
                    let native = if action.is_check() {
                        NativeItem::Check(CheckMenuItem::new(spec.menu_label, true, false, accel))
                    } else {
                        NativeItem::Plain {
                            item: MenuItem::new(spec.menu_label, true, accel),
                            label: spec.menu_label.to_string(),
                        }
                    };
                    let (id, appended) = match &native {
                        NativeItem::Plain { item, .. } => (item.id().0.clone(), sub.append(item)),
                        NativeItem::Check(item) => (item.id().0.clone(), sub.append(item)),
                    };
                    appended.expect("append menu item");
                    self.actions.insert(id, *action);
                    self.items.push((*action, native));
                }
                MenuEntry::Separator => {
                    sub.append(&PredefinedMenuItem::separator())
                        .expect("append separator");
                }
                MenuEntry::Submenu(title, inner) => {
                    let child = Submenu::new(*title, true);
                    self.fill(&child, inner);
                    sub.append(&child).expect("append submenu");
                }
                MenuEntry::OpenRecent => {
                    let child = Submenu::new("Open Recent", false);
                    sub.append(&child).expect("append recent submenu");
                    self.recent_sub = Some(child);
                }
                MenuEntry::System(item) => {
                    sub.append(&system_item(*item)).expect("append system item");
                }
            }
        }
    }
}

fn build_menu() -> MenuStore {
    let menu = Menu::new();
    let mut b = Builder {
        actions: HashMap::new(),
        items: Vec::new(),
        recent_sub: None,
    };
    for (title, entries) in menu_bar() {
        let sub = Submenu::new(title, true);
        b.fill(&sub, &entries);
        menu.append(&sub).expect("append menu");
    }

    let recent_items: Vec<MenuItem> = (0..MAX_RECENT)
        .map(|_| MenuItem::new("", false, None))
        .collect();
    for (i, item) in recent_items.iter().enumerate() {
        b.actions.insert(item.id().0.clone(), Action::OpenRecent(i));
    }
    let clear_recent_item = MenuItem::new(Action::ClearRecent.spec().menu_label, true, None);
    b.actions
        .insert(clear_recent_item.id().0.clone(), Action::ClearRecent);

    menu.init_for_nsapp();

    MenuStore {
        _menu: menu,
        actions: b.actions,
        items: b.items,
        recent_sub: b.recent_sub.expect("menu bar has an Open Recent submenu"),
        recent_items,
        clear_recent_item,
        recent_snapshot: Vec::new(),
    }
}

pub fn poll_menu_events_system(
    _marker: NonSendMarker,
    store: Option<NonSend<MenuStore>>,
    mut queue: ResMut<ActionQueue>,
) {
    let Some(store) = store else { return };
    let rx = MenuEvent::receiver();
    while let Ok(ev) = rx.try_recv() {
        if let Some(action) = store.actions.get(&ev.id.0) {
            queue.push(*action, Origin::Canvas);
        }
    }
}

pub fn update_menu_enabled_system(
    _marker: NonSendMarker,
    store: Option<NonSendMut<MenuStore>>,
    mut contexts: bevy_egui::EguiContexts,
    state: ActionStateParams,
) {
    let Some(mut store) = store else { return };
    // AppKit routes a key equivalent before the key reaches the winit view,
    // blind to egui text focus. A disabled item doesn't fire, so the key
    // still reaches a focused text field.
    let typing = contexts
        .ctx_mut()
        .map(|c| c.wants_keyboard_input())
        .unwrap_or(false);
    let s = state.get();
    for (action, item) in &mut store.items {
        let enabled = action.enabled(&s) && !(typing && action.clashes_with_typing());
        match item {
            NativeItem::Plain { item, label } => {
                if item.is_enabled() != enabled {
                    item.set_enabled(enabled);
                }
                let want = action.menu_label(&s);
                if *label != want {
                    item.set_text(&want);
                    *label = want.into_owned();
                }
            }
            NativeItem::Check(item) => {
                if item.is_enabled() != enabled {
                    item.set_enabled(enabled);
                }
                let checked = action.checked(&s);
                if item.is_checked() != checked {
                    item.set_checked(checked);
                }
            }
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

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::prelude::KeyCode;

    #[test]
    fn every_menu_shortcut_maps_to_an_accelerator() {
        for action in Action::STATIC {
            if let Some(chord) = action.shortcut() {
                assert!(accelerator(chord).is_some(), "{action:?}: {chord:?}");
            }
        }
    }

    #[test]
    fn accelerator_carries_modifiers() {
        let accel = accelerator(Chord::cmd_shift(KeyCode::KeyS)).expect("accel");
        assert_eq!(
            accel,
            Accelerator::new(Some(Modifiers::SUPER | Modifiers::SHIFT), Code::KeyS)
        );
        let plain = accelerator(Chord::key(KeyCode::Backspace)).expect("accel");
        assert_eq!(plain, Accelerator::new(None, Code::Backspace));
    }
}
