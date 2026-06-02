//! The three confirm/settings modals drawn at the tail of `ui_system`:
//! Preferences, the new-project sheet, and the discard-edits confirm. Pulled
//! out of `ui.rs` so the inspector module stays focused on panel layout. Each
//! is a free function over the resources it touches; `ui_system` calls them
//! when the corresponding modal is open.

use crate::theme::{
    CanvasBgPref, Preferences, PreferencesWindow, Theme, ThemePref, canvas_match_color,
    save_preferences,
};
use crate::ui::palette::{
    self, DiscardConfirm, PaletteChoice, PaletteRenameState, Palettes, WorkingPalette,
};
use crate::ui::tokens::{font, space, width};
use crate::ui::widgets;
use bevy_egui::egui;
use roxel::grid::NewProject;
use roxel::io;

/// Preferences modal: appearance, canvas, visibility, and color-format rows.
/// Persists `Preferences` only when a field actually changed this frame.
pub fn draw_preferences(
    ctx: &egui::Context,
    theme: &Theme,
    prefs: &mut Preferences,
    prefs_window: &mut PreferencesWindow,
) {
    let before = prefs.clone();
    let mut open_flag = true;
    widgets::modal_window(theme, "Preferences", &mut open_flag).show(ctx, |ui| {
        ui.set_min_width(width::MODAL_PREFS);
        widgets::section(ui, theme, "Appearance", |ui| {
            widgets::prefs_row(ui, theme, "Theme", |ui| {
                widgets::chip_button(ui, theme, &mut prefs.theme, ThemePref::System, "System");
                widgets::chip_button(ui, theme, &mut prefs.theme, ThemePref::Light, "Light");
                widgets::chip_button(ui, theme, &mut prefs.theme, ThemePref::Dark, "Dark");
            });
        });

        widgets::section(ui, theme, "Canvas", |ui| {
            let mut is_custom = matches!(prefs.canvas_bg, CanvasBgPref::Custom(_));
            widgets::prefs_row(ui, theme, "Background", |ui| {
                if ui.radio(!is_custom, "Match theme").clicked() {
                    prefs.canvas_bg = CanvasBgPref::MatchTheme;
                    is_custom = false;
                }
                if ui.radio(is_custom, "Custom").clicked() {
                    let seed = match prefs.canvas_bg {
                        CanvasBgPref::Custom(rgb) => rgb,
                        CanvasBgPref::MatchTheme => canvas_match_color(theme.mode),
                    };
                    prefs.canvas_bg = CanvasBgPref::Custom(seed);
                }
            });
            if let CanvasBgPref::Custom(ref mut rgb) = prefs.canvas_bg {
                ui.add_space(space::XS);
                ui.horizontal(|ui| {
                    ui.add_space(space::PREFS_INDENT);
                    ui.color_edit_button_srgb(rgb);
                    widgets::hex_label(ui, theme, *rgb, true);
                });
            }
        });

        // Floor grid + origin axes moved to the View menu (native menu on macOS,
        // the floating menu pill on Win/Linux). The floating-menu-bar toggle must
        // stay here — it can't live in the menu it would hide.
        #[cfg(not(target_os = "macos"))]
        widgets::section(ui, theme, "Visibility", |ui| {
            ui.checkbox(&mut prefs.show_floating_menu_bar, "Show floating menu bar");
        });
        // Color-space format moved to the View menu (native menu on macOS, the
        // floating menu pill on Win/Linux) — it's a per-view readout choice, not
        // an app preference, so it no longer earns a Preferences row.

        widgets::section(ui, theme, "Updates", |ui| {
            ui.checkbox(&mut prefs.auto_update_check, "Check for updates on launch");
        });
    });
    let esc = ctx.input(|i| i.key_pressed(egui::Key::Escape));
    if !open_flag || esc {
        prefs_window.open = false;
    }
    if *prefs != before {
        save_preferences(prefs);
    }
}

/// New-project confirm sheet. Open-world has no grid size to pick — this is
/// just "do you want to throw away unsaved work?". Built with an anchored
/// `Area` instead of `egui::Window` so the modal sizes tight to its content
/// (egui::Window kept ballooning vertically here).
pub fn draw_new_project(ctx: &egui::Context, theme: &Theme, new_project: &mut NewProject) {
    let mut create_clicked = false;
    let mut cancel_clicked = false;
    egui::Area::new("new_project_modal".into())
        .order(egui::Order::Foreground)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .show(ctx, |ui| {
            widgets::modal_frame(theme, crate::ui::tokens::pad::MODAL).show(ui, |ui| {
                ui.set_width(width::MODAL_NEW);
                ui.vertical(|ui| {
                    ui.label(
                        egui::RichText::new("New project")
                            .family(egui::FontFamily::Name(
                                crate::theme::INTER_SEMIBOLD_FAMILY.into(),
                            ))
                            .size(font::HEADING)
                            .color(theme.text),
                    );
                    ui.add_space(space::XS);
                    widgets::hint_label(ui, theme, "Discard unsaved work and start over?");
                    ui.add_space(space::SM);
                    ui.horizontal(|ui| {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.spacing_mut().item_spacing.x = space::SX;
                            if widgets::dialog_button(ui, theme, "Create", true).clicked() {
                                create_clicked = true;
                            }
                            if widgets::dialog_button(ui, theme, "Cancel", false).clicked() {
                                cancel_clicked = true;
                            }
                        });
                    });
                });
            });
        });
    let esc = ctx.input(|i| i.key_pressed(egui::Key::Escape));
    let enter = ctx.input(|i| i.key_pressed(egui::Key::Enter));
    if create_clicked || enter {
        new_project.apply = true;
        new_project.dialog_open = false;
    } else if cancel_clicked || esc {
        new_project.dialog_open = false;
    }
}

/// Open-project guard. Shown when the user triggers "Open…" with unsaved
/// changes in the current document. Returns `Some(true)` to proceed (discard +
/// open the file dialog), `Some(false)` to cancel, `None` while still open.
pub fn draw_open_confirm(ctx: &egui::Context, theme: &Theme) -> Option<bool> {
    let mut open_clicked = false;
    let mut cancel_clicked = false;
    egui::Area::new("open_confirm_modal".into())
        .order(egui::Order::Foreground)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .show(ctx, |ui| {
            widgets::modal_frame(theme, crate::ui::tokens::pad::MODAL).show(ui, |ui| {
                ui.set_width(width::MODAL_NEW);
                ui.vertical(|ui| {
                    ui.label(
                        egui::RichText::new("Open project")
                            .family(egui::FontFamily::Name(
                                crate::theme::INTER_SEMIBOLD_FAMILY.into(),
                            ))
                            .size(font::HEADING)
                            .color(theme.text),
                    );
                    ui.add_space(space::XS);
                    widgets::hint_label(ui, theme, "Discard unsaved changes and open another?");
                    ui.add_space(space::SM);
                    ui.horizontal(|ui| {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.spacing_mut().item_spacing.x = space::SX;
                            if widgets::dialog_button(ui, theme, "Open", true).clicked() {
                                open_clicked = true;
                            }
                            if widgets::dialog_button(ui, theme, "Cancel", false).clicked() {
                                cancel_clicked = true;
                            }
                        });
                    });
                });
            });
        });
    let esc = ctx.input(|i| i.key_pressed(egui::Key::Escape));
    let enter = ctx.input(|i| i.key_pressed(egui::Key::Enter));
    if open_clicked || enter {
        Some(true)
    } else if cancel_clicked || esc {
        Some(false)
    } else {
        None
    }
}

/// Discard-edits confirm: switching away from a dirty built-in palette stages a
/// `target` index so the user can keep their scratch edits first. `target` is
/// the palette index they were switching to.
#[allow(clippy::too_many_arguments)]
pub fn draw_discard(
    ctx: &egui::Context,
    theme: &Theme,
    target: usize,
    palettes: &mut Palettes,
    palette_choice: &mut PaletteChoice,
    working: &mut WorkingPalette,
    discard: &mut DiscardConfirm,
    palette_rename: &mut PaletteRenameState,
) {
    let name = palettes.0[palette_choice.0.min(palettes.0.len().saturating_sub(1))]
        .name
        .clone();
    let mut open = true;
    let mut save_clicked = false;
    let mut discard_clicked = false;
    let mut cancel_clicked = false;
    widgets::modal_window(theme, "Discard edits?", &mut open).show(ctx, |ui| {
        ui.set_width(width::MODAL_DISCARD);
        widgets::hint_label(
            ui,
            theme,
            &format!("“{name}” has unsaved swatches. Save them as a new palette, or discard?"),
        );
        ui.add_space(space::SM);
        ui.horizontal(|ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = space::SX;
                if widgets::dialog_button(ui, theme, "Save as new", true).clicked() {
                    save_clicked = true;
                }
                if widgets::dialog_button(ui, theme, "Discard", false).clicked() {
                    discard_clicked = true;
                }
                if widgets::dialog_button(ui, theme, "Cancel", false).clicked() {
                    cancel_clicked = true;
                }
            });
        });
    });
    let esc = ctx.input(|i| i.key_pressed(egui::Key::Escape));
    let enter = ctx.input(|i| i.key_pressed(egui::Key::Enter));
    if save_clicked || enter {
        let i = palette::save_as_new(palettes, palette_choice, working);
        palette_rename.editing = Some(i);
        palette_rename.buf = palettes.0[i].name.clone();
        io::palettes::save(&palettes.0);
        discard.pending = None;
    } else if discard_clicked {
        working.clear();
        palette_choice.0 = target.min(palettes.0.len().saturating_sub(1));
        discard.pending = None;
    } else if cancel_clicked || esc || !open {
        // Esc cancels — keeps the scratch edits and the current palette.
        discard.pending = None;
    }
}
