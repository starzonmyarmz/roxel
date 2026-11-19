//! The menu bar layout, shared by the macOS menu bar ([`native`]) and the
//! Win/Linux menu pill. Items are [`Action`]s, so both read labels, shortcuts
//! and enabled state from the action table and push onto `ActionQueue`.

#[cfg(target_os = "macos")]
pub mod native;

use bevy::prelude::*;

use crate::actions::{Action, ExportFormat, ImportFormat};
use crate::camera::CameraPreset;
use roxel::color_space::ColorSpace;

pub const COPYRIGHT: &str = "© 2026 Daniel Marino";

/// Whether the About Roxel modal is showing. macOS uses the native About
/// panel in its menu bar.
#[derive(Resource, Default)]
pub struct AboutWindow(pub bool);

pub fn recent_item_label(index: usize, path: &std::path::Path) -> String {
    let name = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or_else(|| path.to_str().unwrap_or("file"));
    format!("{}  {}", index + 1, name)
}

/// macOS-only menu items AppKit provides.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SystemItem {
    About,
    Services,
    Hide,
    HideOthers,
    ShowAll,
    CloseWindow,
    Minimize,
    Maximize,
    BringAllToFront,
}

#[derive(Clone, Debug, PartialEq)]
pub enum MenuEntry {
    Item(Action),
    Separator,
    Submenu(&'static str, Vec<MenuEntry>),
    /// The Open Recent submenu: one item per recent file, then Clear Menu.
    OpenRecent,
    System(SystemItem),
}

use MenuEntry::{Item, Separator, Submenu};

fn import_menu() -> MenuEntry {
    Submenu(
        "Import",
        [ImportFormat::Vox, ImportFormat::Qb, ImportFormat::Gox]
            .map(|f| Item(Action::Import(f)))
            .to_vec(),
    )
}

fn export_menu() -> MenuEntry {
    Submenu(
        "Export",
        vec![
            Item(Action::Export(ExportFormat::Vox)),
            Item(Action::Export(ExportFormat::Gox)),
            Item(Action::Export(ExportFormat::Obj)),
            Item(Action::Export(ExportFormat::Gltf)),
            Item(Action::Export(ExportFormat::Png)),
            Item(Action::ExportShot),
            Item(Action::Export(ExportFormat::Svg)),
        ],
    )
}

fn edit_menu() -> Vec<MenuEntry> {
    vec![
        Item(Action::Undo),
        Item(Action::Redo),
        Separator,
        Item(Action::FillSelection),
        Item(Action::DeleteSelectionContents),
        Item(Action::SelectAll),
        Item(Action::ClearSelection),
        Separator,
        Item(Action::Cut),
        Item(Action::Copy),
        Item(Action::Paste),
        Separator,
        Item(Action::DoubleDensity),
        Item(Action::HalveDensity),
    ]
}

fn view_menu() -> Vec<MenuEntry> {
    let preset = |p| Item(Action::ViewPreset(p));
    vec![
        Submenu(
            "Camera",
            vec![
                Item(Action::FrameView),
                Separator,
                preset(CameraPreset::Front),
                preset(CameraPreset::Back),
                preset(CameraPreset::Right),
                preset(CameraPreset::Left),
                preset(CameraPreset::Top),
                Separator,
                preset(CameraPreset::Iso),
            ],
        ),
        Separator,
        Item(Action::ToggleFloorGrid),
        Item(Action::ToggleOriginAxes),
        Separator,
        Submenu(
            "Color Format",
            ColorSpace::ALL
                .map(|c| Item(Action::SetColorSpace(c)))
                .to_vec(),
        ),
    ]
}

/// The menu bar for this platform: macOS puts app-level items in a Roxel
/// menu and adds a Window menu; the Win/Linux pill folds them into File and
/// Help.
pub fn menu_bar() -> Vec<(&'static str, Vec<MenuEntry>)> {
    use MenuEntry::{OpenRecent, System};
    if cfg!(target_os = "macos") {
        vec![
            (
                "Roxel",
                vec![
                    System(SystemItem::About),
                    Item(Action::CheckForUpdates),
                    Separator,
                    Item(Action::ShowCommandPalette),
                    Item(Action::Preferences),
                    Separator,
                    System(SystemItem::Services),
                    Separator,
                    System(SystemItem::Hide),
                    System(SystemItem::HideOthers),
                    System(SystemItem::ShowAll),
                    Separator,
                    Item(Action::Quit),
                ],
            ),
            (
                "File",
                vec![
                    Item(Action::NewProject),
                    Separator,
                    Item(Action::OpenProject),
                    OpenRecent,
                    Item(Action::SaveProject),
                    Item(Action::SaveProjectAs),
                    Separator,
                    import_menu(),
                    export_menu(),
                    Separator,
                    System(SystemItem::CloseWindow),
                ],
            ),
            ("Edit", edit_menu()),
            ("View", view_menu()),
            (
                "Window",
                vec![
                    System(SystemItem::Minimize),
                    System(SystemItem::Maximize),
                    Separator,
                    System(SystemItem::BringAllToFront),
                ],
            ),
            (
                "Help",
                vec![Item(Action::ShowOnboarding), Item(Action::Changelog)],
            ),
        ]
    } else {
        vec![
            (
                "File",
                vec![
                    Item(Action::NewProject),
                    Item(Action::OpenProject),
                    OpenRecent,
                    Item(Action::SaveProject),
                    Item(Action::SaveProjectAs),
                    Separator,
                    import_menu(),
                    export_menu(),
                    Separator,
                    Item(Action::ShowCommandPalette),
                    Item(Action::Preferences),
                    Separator,
                    Item(Action::Quit),
                ],
            ),
            ("Edit", edit_menu()),
            ("View", view_menu()),
            (
                "Help",
                vec![
                    Item(Action::ShowOnboarding),
                    Item(Action::Changelog),
                    Separator,
                    Item(Action::CheckForUpdates),
                    Item(Action::About),
                ],
            ),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn actions(entries: &[MenuEntry], out: &mut Vec<Action>) {
        for e in entries {
            match e {
                Item(a) => out.push(*a),
                Submenu(_, inner) => actions(inner, out),
                _ => {}
            }
        }
    }

    #[test]
    fn menu_bar_lists_each_action_once() {
        let mut all = Vec::new();
        for (_, entries) in menu_bar() {
            actions(&entries, &mut all);
        }
        for (i, a) in all.iter().enumerate() {
            assert!(!all[..i].contains(a), "{a:?} appears twice");
        }
        assert!(all.contains(&Action::Quit));
        assert!(all.contains(&Action::ExportShot));
    }

    #[test]
    fn menu_items_come_from_the_action_table() {
        let mut all = Vec::new();
        for (_, entries) in menu_bar() {
            actions(&entries, &mut all);
        }
        for a in all {
            assert!(Action::STATIC.contains(&a), "{a:?} missing from STATIC");
            assert!(!a.spec().menu_label.is_empty());
        }
    }

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
