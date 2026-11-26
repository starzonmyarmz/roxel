//! Keyboard-shortcuts cheat sheet (Help → Keyboard Shortcuts). Lists
//! every action chord from the `Action` table plus the contextual canvas keys
//! that live in their own systems.

use bevy::prelude::*;
use bevy_egui::egui;

use crate::actions::{Action, Category, Platform};
use crate::theme::Theme;
use crate::ui::tokens::{height, space, width};
use crate::ui::widgets::{self, KeySeg};

#[derive(Resource, Default)]
pub struct ShortcutsWindow(pub bool);

pub struct ShortcutGroup {
    pub title: &'static str,
    pub rows: Vec<(Vec<KeySeg>, &'static str)>,
}

fn group_title(c: Category) -> &'static str {
    match c {
        Category::File => "File",
        Category::Edit => "Edit",
        Category::Tools => "Tools",
        Category::Shape => "Shape",
        Category::View => "View",
        Category::Palette => "Palette",
        Category::Preferences => "Preferences",
        Category::Plugins => "Plugins",
        Category::Help => "Help",
    }
}

const CATEGORY_ORDER: [Category; 9] = [
    Category::Tools,
    Category::Edit,
    Category::View,
    Category::File,
    Category::Shape,
    Category::Palette,
    Category::Preferences,
    Category::Plugins,
    Category::Help,
];

fn canvas_rows(platform: Platform) -> Vec<(Vec<KeySeg>, &'static str)> {
    use KeySeg::{Shift, Text, Word};
    let alt = || match platform {
        Platform::Mac => KeySeg::Option,
        Platform::Other => Text("Alt".into()),
    };
    vec![
        (vec![Word("Left-drag")], "Use the current tool"),
        (vec![Word("Right-drag")], "Orbit"),
        (vec![Word("Scroll")], "Zoom"),
        (vec![Text("Space".into()), Word("+ drag")], "Pan"),
        (
            vec![Text("Z".into()), Word("+ click")],
            "Zoom in at the cursor",
        ),
        (
            vec![alt(), Text("Z".into()), Word("+ click")],
            "Zoom out at the cursor",
        ),
        (vec![Shift, Word("+ click")], "Line from the last voxel"),
        (vec![alt(), Word("(hold)")], "Eyedropper while held"),
        (
            vec![Word("Double-click")],
            "Select or paint a connected region",
        ),
        (
            vec![Text("Arrow keys".into())],
            "Nudge the selection (Move)",
        ),
        (
            vec![Shift, Text("↑".into()), Word("/"), Shift, Text("↓".into())],
            "Nudge the selection up or down",
        ),
        (vec![Text("Esc".into())], "Cancel, or clear the selection"),
    ]
}

/// Every group the sheet shows, in order: canvas keys first, then each action
/// category that has at least one chord.
pub fn shortcut_groups(platform: Platform) -> Vec<ShortcutGroup> {
    let mut groups = vec![ShortcutGroup {
        title: "Canvas",
        rows: canvas_rows(platform),
    }];
    for category in CATEGORY_ORDER {
        let rows: Vec<_> = Action::STATIC
            .iter()
            .filter(|a| a.spec().category == category)
            .flat_map(|a| {
                let label = a.spec().label;
                a.chords()
                    .map(move |c| (widgets::chord_segs(c, platform), label))
            })
            .collect();
        if !rows.is_empty() {
            groups.push(ShortcutGroup {
                title: group_title(category),
                rows,
            });
        }
    }
    groups
}

/// Returns `true` once dismissed (Close, Esc or the window's close button).
pub fn draw(ctx: &egui::Context, theme: &Theme) -> bool {
    let mut open = true;
    let mut close = false;
    widgets::modal_window(theme, "Keyboard Shortcuts", &mut open).show(ctx, |ui| {
        ui.set_width(width::MODAL_SHORTCUTS);
        egui::ScrollArea::vertical()
            .max_height(height::MODAL_SHORTCUTS_MAX)
            .show(ui, |ui| {
                let gutter = egui::Frame::default().inner_margin(egui::Margin {
                    right: space::PANEL_INSET as i8,
                    ..egui::Margin::ZERO
                });
                gutter.show(ui, |ui| {
                    for group in shortcut_groups(Platform::CURRENT) {
                        widgets::section(ui, theme, group.title, |ui| {
                            for (keys, label) in &group.rows {
                                widgets::key_row(ui, theme, label, keys);
                            }
                        });
                    }
                });
            });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if widgets::dialog_button(ui, theme, "Close", true).clicked() {
                close = true;
            }
        });
    });
    let esc = ctx.input(|i| i.key_pressed(egui::Key::Escape));
    close || esc || !open
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_action_chord_is_on_the_sheet() {
        for platform in [Platform::Mac, Platform::Other] {
            let groups = shortcut_groups(platform);
            for action in Action::STATIC {
                for chord in action.chords() {
                    let keys = widgets::chord_segs(chord, platform);
                    assert!(
                        groups
                            .iter()
                            .flat_map(|g| &g.rows)
                            .any(|(k, l)| *k == keys && *l == action.spec().label),
                        "{action:?} ({keys:?}) missing from the sheet"
                    );
                }
            }
        }
    }

    #[test]
    fn canvas_keys_come_first_and_groups_are_never_empty() {
        let groups = shortcut_groups(Platform::Other);
        assert_eq!(groups[0].title, "Canvas");
        assert!(groups.iter().all(|g| !g.rows.is_empty()));
    }

    #[test]
    fn canvas_keys_follow_platform_modifiers() {
        let segs = |p| -> Vec<KeySeg> { canvas_rows(p).into_iter().flat_map(|(k, _)| k).collect() };
        assert!(segs(Platform::Mac).contains(&KeySeg::Option));
        assert!(!segs(Platform::Other).contains(&KeySeg::Option));
        assert!(segs(Platform::Other).contains(&KeySeg::Text("Alt".into())));
    }
}
