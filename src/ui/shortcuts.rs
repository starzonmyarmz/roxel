//! Keyboard-shortcuts cheat sheet (Help → Keyboard Shortcuts). Lists
//! every action chord from the `Action` table plus the contextual canvas keys
//! that live in their own systems.

use bevy::prelude::*;
use bevy_egui::egui;
use std::borrow::Cow;

use crate::actions::{Action, Category, Platform};
use crate::theme::Theme;
use crate::ui::tokens::{height, width};
use crate::ui::widgets;

#[derive(Resource, Default)]
pub struct ShortcutsWindow(pub bool);

pub struct ShortcutGroup {
    pub title: &'static str,
    pub rows: Vec<(Cow<'static, str>, Cow<'static, str>)>,
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
        Category::Help => "Help",
    }
}

const CATEGORY_ORDER: [Category; 8] = [
    Category::Tools,
    Category::Edit,
    Category::View,
    Category::File,
    Category::Shape,
    Category::Palette,
    Category::Preferences,
    Category::Help,
];

fn canvas_rows(platform: Platform) -> Vec<(Cow<'static, str>, Cow<'static, str>)> {
    let mac = platform == Platform::Mac;
    [
        ("Left-drag", "Use the current tool"),
        ("Right-drag", "Orbit"),
        ("Scroll", "Zoom"),
        ("Space + drag", "Pan"),
        ("Z + click", "Zoom in at the cursor"),
        (
            if mac { "⌥Z + click" } else { "Alt+Z + click" },
            "Zoom out at the cursor",
        ),
        (
            if mac { "⇧ + click" } else { "Shift + click" },
            "Line from the last voxel",
        ),
        (
            if mac { "⌥ (hold)" } else { "Alt (hold)" },
            "Eyedropper while held",
        ),
        ("Double-click", "Select or paint a connected region"),
        ("Arrow keys", "Nudge the selection (Move)"),
        (
            if mac {
                "⇧↑ / ⇧↓"
            } else {
                "Shift+↑ / Shift+↓"
            },
            "Nudge the selection up or down",
        ),
        ("Esc", "Cancel, or clear the selection"),
    ]
    .into_iter()
    .map(|(k, l)| (k.into(), l.into()))
    .collect()
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
                    .map(move |c| (c.display(platform).into(), label.into()))
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
                for group in shortcut_groups(Platform::CURRENT) {
                    widgets::section(ui, theme, group.title, |ui| {
                        for (keys, label) in group.rows {
                            widgets::stat_row(ui, theme, &label, keys.into_owned());
                        }
                    });
                }
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
                    let keys = chord.display(platform);
                    assert!(
                        groups
                            .iter()
                            .flat_map(|g| &g.rows)
                            .any(|(k, l)| *k == keys && *l == action.spec().label),
                        "{action:?} ({keys}) missing from the sheet"
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
        let keys = |p| -> Vec<String> {
            canvas_rows(p)
                .into_iter()
                .map(|(k, _)| k.into_owned())
                .collect()
        };
        assert!(keys(Platform::Mac).iter().any(|k| k.contains('⌥')));
        assert!(keys(Platform::Other).iter().all(|k| !k.contains('⌥')));
        assert!(keys(Platform::Other).iter().any(|k| k.contains("Alt")));
    }
}
