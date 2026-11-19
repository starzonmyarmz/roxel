//! The confirm/settings modals drawn at the tail of `ui_system`: Preferences,
//! the unsaved-changes confirm, and the discard-edits confirm. Pulled
//! out of `ui.rs` so the inspector module stays focused on panel layout. Each
//! is a free function over the resources it touches; `ui_system` calls them
//! when the corresponding modal is open.

use crate::shot::{GradientMode, ResPreset, ShotPanel, ShotParams};
use crate::theme::{
    CanvasBgPref, Preferences, PreferencesWindow, Theme, ThemePref, canvas_match_color,
    save_preferences,
};
use crate::ui::color_picker;
use crate::ui::dialogs::{self, ConfirmChoice, DialogResult, GuardedAction, PendingDialog};
use crate::ui::palette::{
    self, DiscardConfirm, PaletteChoice, PaletteRenameState, Palettes, WorkingPalette,
};
use crate::ui::tokens::{font, radius, size, space, swatch, width};
use crate::ui::widgets;
use bevy_egui::egui;
use roxel::io;
use std::path::PathBuf;

const UI_SCALE_LABELS: [(u16, &str); 6] = [
    (80, "80%"),
    (90, "90%"),
    (100, "100%"),
    (110, "110%"),
    (125, "125%"),
    (150, "150%"),
];

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
            widgets::prefs_row(ui, theme, "UI scale", |ui| {
                ui.horizontal_wrapped(|ui| {
                    for (scale, label) in UI_SCALE_LABELS {
                        widgets::chip_button(ui, theme, &mut prefs.ui_scale, scale, label);
                    }
                });
            });
            widgets::prefs_row(ui, theme, "Motion", |ui| {
                ui.checkbox(&mut prefs.reduce_motion, "Reduce motion");
            });
        });

        let color_space = prefs.color_space;
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
                    color_picker::space_color_swatch(
                        ui,
                        theme,
                        rgb,
                        color_space,
                        swatch::PREVIEW,
                        radius::XS,
                    );
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

/// Heading and prompt for the unsaved-changes confirm.
fn unsaved_copy(action: &GuardedAction, doc_name: &str) -> (&'static str, String) {
    match action {
        GuardedAction::New => (
            "New project",
            format!("Save changes to “{doc_name}” before starting a new project?"),
        ),
        GuardedAction::Open(_) => (
            "Open project",
            format!("Save changes to “{doc_name}” before opening another project?"),
        ),
        GuardedAction::Quit => (
            "Quit Roxel",
            format!("Save changes to “{doc_name}” before quitting?"),
        ),
    }
}

/// Unsaved-changes confirm for New / Open / Quit. Enter picks Save, never the
/// destructive choice; Esc cancels. Returns `None` while still open. Built with
/// an anchored `Area` instead of `egui::Window` so the modal sizes tight to its
/// content (egui::Window kept ballooning vertically here).
pub fn draw_unsaved_confirm(
    ctx: &egui::Context,
    theme: &Theme,
    action: &GuardedAction,
    doc_name: &str,
) -> Option<ConfirmChoice> {
    let (heading, prompt) = unsaved_copy(action, doc_name);
    let mut clicked = None;
    egui::Area::new("unsaved_confirm_modal".into())
        .order(egui::Order::Foreground)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .show(ctx, |ui| {
            widgets::modal_frame(theme, crate::ui::tokens::pad::MODAL).show(ui, |ui| {
                ui.set_width(width::MODAL_DISCARD);
                ui.vertical(|ui| {
                    ui.label(widgets::modal_heading(theme, heading));
                    ui.add_space(space::XS);
                    widgets::hint_label(ui, theme, &prompt);
                    ui.add_space(space::SM);
                    ui.horizontal(|ui| {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.spacing_mut().item_spacing.x = space::SX;
                            if widgets::dialog_button(ui, theme, "Save", true).clicked() {
                                clicked = Some(ConfirmChoice::Save);
                            }
                            if widgets::dialog_button(ui, theme, "Cancel", false).clicked() {
                                clicked = Some(ConfirmChoice::Cancel);
                            }
                            if widgets::dialog_button(ui, theme, "Don’t Save", false).clicked() {
                                clicked = Some(ConfirmChoice::Discard);
                            }
                        });
                    });
                });
            });
        });
    if clicked.is_some() {
        return clicked;
    }
    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        return Some(ConfirmChoice::Cancel);
    }
    if ctx.input(|i| i.key_pressed(egui::Key::Enter)) {
        return Some(ConfirmChoice::Save);
    }
    None
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

/// Confirm before deleting a user palette. Cancel is the default (Enter or
/// Esc); only a click on Delete deletes. Returns `Some(true)` to delete,
/// `Some(false)` to cancel, `None` while still open.
pub fn draw_delete_palette(ctx: &egui::Context, theme: &Theme, name: &str) -> Option<bool> {
    let mut open = true;
    let mut clicked = None;
    widgets::modal_window(theme, "Delete palette?", &mut open).show(ctx, |ui| {
        ui.set_width(width::MODAL_DISCARD);
        widgets::hint_label(
            ui,
            theme,
            &format!("Delete “{name}”? This can’t be undone."),
        );
        ui.add_space(space::SM);
        ui.horizontal(|ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = space::SX;
                if widgets::dialog_button(ui, theme, "Cancel", true).clicked() {
                    clicked = Some(false);
                }
                if widgets::dialog_button(ui, theme, "Delete", false).clicked() {
                    clicked = Some(true);
                }
            });
        });
    });
    if clicked.is_some() {
        return clicked;
    }
    let dismiss =
        ctx.input(|i| i.key_pressed(egui::Key::Escape) || i.key_pressed(egui::Key::Enter));
    if dismiss || !open {
        return Some(false);
    }
    None
}

/// About Roxel: version and copyright, with a Changelog link. Returns `true`
/// once dismissed (Close, Esc, Enter or the window's close button).
pub fn draw_about(ctx: &egui::Context, theme: &Theme) -> bool {
    let mut open = true;
    let mut close = false;
    widgets::modal_window(theme, "About Roxel", &mut open).show(ctx, |ui| {
        ui.set_width(width::MODAL_ABOUT);
        ui.label(
            egui::RichText::new(format!("Version {}", env!("CARGO_PKG_VERSION")))
                .size(font::BODY)
                .color(theme.text),
        );
        widgets::hint_label(ui, theme, crate::menu::COPYRIGHT);
        ui.add_space(space::SM);
        ui.horizontal(|ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = space::SX;
                if widgets::dialog_button(ui, theme, "Close", true).clicked() {
                    close = true;
                }
                if widgets::dialog_button(ui, theme, "Changelog", false).clicked() {
                    crate::updater::open_url(crate::actions::CHANGELOG_URL);
                }
            });
        });
    });
    let keyed = ctx.input(|i| i.key_pressed(egui::Key::Escape) || i.key_pressed(egui::Key::Enter));
    close || keyed || !open
}

/// A `prefs_row`-style label + content row whose content **wraps** — chips that
/// don't fit the knob column flow onto a second line instead of being clipped.
fn labeled_wrap(ui: &mut egui::Ui, theme: &Theme, label: &str, add: impl FnOnce(&mut egui::Ui)) {
    ui.horizontal_top(|ui| {
        ui.allocate_ui_with_layout(
            size::PREFS_LABEL,
            egui::Layout::left_to_right(egui::Align::Min),
            |ui| {
                ui.set_min_width(size::PREFS_LABEL.x);
                ui.add(egui::Label::new(
                    egui::RichText::new(label)
                        .color(theme.text_dim)
                        .size(font::SMALL),
                ));
            },
        );
        ui.horizontal_wrapped(|ui| add(ui));
    });
}

/// A labeled slider that resets to `default` on double-click. Returns the
/// response so the caller can also test `.dragged()` (used to debounce the
/// expensive scene-knob re-render until the drag stops).
fn shot_slider(
    ui: &mut egui::Ui,
    theme: &Theme,
    label: &str,
    value: &mut f32,
    range: std::ops::RangeInclusive<f32>,
    default: f32,
) -> egui::Response {
    let mut out = None;
    widgets::prefs_row(ui, theme, label, |ui| {
        let r = ui.add(egui::Slider::new(value, range).show_value(false));
        // The slider's own drag sense swallows `double_clicked()`, so detect the
        // double-click via the pointer while hovering the widget.
        if r.hovered()
            && ui.input(|i| {
                i.pointer
                    .button_double_clicked(egui::PointerButton::Primary)
            })
        {
            *value = default;
        }
        out = Some(r);
    });
    out.expect("prefs_row runs its closure")
}

/// Export-Shot tweak panel: a live, low-res preview (left) plus the
/// art-direction knobs that drive it (right). Post-only knobs (gradient,
/// background color, dither, vignette) recomposite the cached captures
/// instantly; the scene knobs (aspect, saturation, lift) trigger a GPU
/// re-render — debounced so dragging a slider re-renders after a short pause
/// (or on release), not every frame. "Export…" spawns the save dialog → full
/// [`ResPreset`]-resolution render.
pub fn draw_shot_panel(
    ctx: &egui::Context,
    theme: &Theme,
    panel: &mut ShotPanel,
    pending: &mut PendingDialog,
    last_dir: &Option<PathBuf>,
    color_space: roxel::color_space::ColorSpace,
) {
    let before = panel.params.clone();
    let defaults = ShotParams::default();
    let mut open_flag = true;
    let mut export_clicked = false;
    let mut reset_clicked = false;
    // True while a *scene* slider (saturation / lift) is being dragged — defers
    // the GPU re-render until the drag stops.
    let mut scene_dragging = false;

    widgets::modal_window(theme, "Export Shot", &mut open_flag).show(ctx, |ui| {
        // Fix the modal + preview column to exact widths. Without a hard cap the
        // section dividers (which claim `available_width()`) make the auto-sizing
        // window expand to fill the screen.
        ui.set_width(width::MODAL_SHOT);
        ui.horizontal_top(|ui| {
            // ---------- Left: live preview ----------
            ui.vertical(|ui| {
                ui.set_width(width::SHOT_PREVIEW);
                ui.vertical_centered(|ui| {
                    if let Some(tex) = panel.preview_tex {
                        let (w, h) = panel.preview_dims;
                        let long = width::SHOT_PREVIEW;
                        let size = if w >= h {
                            egui::vec2(long, long * h as f32 / w.max(1) as f32)
                        } else {
                            egui::vec2(long * w as f32 / h.max(1) as f32, long)
                        };
                        ui.add(egui::Image::new(egui::load::SizedTexture::new(tex, size)));
                    } else {
                        ui.add_space(space::LG);
                        ui.spinner();
                        ui.add_space(space::LG);
                    }
                });
            });

            ui.add_space(space::MD);

            // ---------- Right: knobs ----------
            ui.vertical(|ui| {
                ui.set_width(width::SHOT_CONTROLS);
                // Clip to this column so the section dividers (which span
                // `clip_rect().x_range()`) don't bleed left across the preview.
                ui.set_clip_rect(ui.max_rect());

                widgets::section(ui, theme, "Format", |ui| {
                    labeled_wrap(ui, theme, "Aspect", |ui| {
                        for preset in ResPreset::ALL {
                            widgets::chip_button(
                                ui,
                                theme,
                                &mut panel.params.resolution,
                                preset,
                                preset.label(),
                            );
                        }
                    });
                });

                widgets::section(ui, theme, "Background", |ui| {
                    labeled_wrap(ui, theme, "Style", |ui| {
                        for g in GradientMode::ALL {
                            widgets::chip_button(
                                ui,
                                theme,
                                &mut panel.params.gradient,
                                g,
                                g.label(),
                            );
                        }
                    });
                    // Always shown and enabled — Strength/Direction are no-ops for
                    // Solid (`gradient_color` ignores them), so leaving them live
                    // keeps the modal height constant (no jump) and avoids the
                    // washed-out disabled-chip look.
                    shot_slider(
                        ui,
                        theme,
                        "Strength",
                        &mut panel.params.gradient_strength,
                        0.0..=1.0,
                        defaults.gradient_strength,
                    );
                    labeled_wrap(ui, theme, "Direction", |ui| {
                        widgets::chip_button(
                            ui,
                            theme,
                            &mut panel.params.gradient_flip,
                            false,
                            "Light to dark",
                        );
                        widgets::chip_button(
                            ui,
                            theme,
                            &mut panel.params.gradient_flip,
                            true,
                            "Dark to light",
                        );
                    });
                    labeled_wrap(ui, theme, "Color", |ui| {
                        let mut custom = panel.params.bg_override.is_some();
                        if ui.radio(!custom, "Auto").clicked() {
                            panel.params.bg_override = None;
                            custom = false;
                        }
                        if ui.radio(custom, "Custom").clicked()
                            && panel.params.bg_override.is_none()
                        {
                            panel.params.bg_override = Some(panel.auto_bg());
                        }
                        if let Some(ref mut rgb) = panel.params.bg_override {
                            color_picker::space_color_swatch(
                                ui,
                                theme,
                                rgb,
                                color_space,
                                swatch::PREVIEW,
                                radius::XS,
                            );
                        }
                    });
                    shot_slider(
                        ui,
                        theme,
                        "Grain",
                        &mut panel.params.dither,
                        0.0..=24.0,
                        defaults.dither,
                    );
                });

                widgets::section(ui, theme, "Look", |ui| {
                    scene_dragging |= shot_slider(
                        ui,
                        theme,
                        "Saturation",
                        &mut panel.params.saturation,
                        0.0..=1.5,
                        defaults.saturation,
                    )
                    .dragged();
                    shot_slider(
                        ui,
                        theme,
                        "Vignette",
                        &mut panel.params.vignette,
                        0.0..=1.0,
                        defaults.vignette,
                    );
                    scene_dragging |= shot_slider(
                        ui,
                        theme,
                        "Lift",
                        &mut panel.params.lift,
                        0.0..=8.0,
                        defaults.lift,
                    )
                    .dragged();
                });

                ui.add_space(space::SM);
                ui.horizontal(|ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.spacing_mut().item_spacing.x = space::SX;
                        if widgets::dialog_button(ui, theme, "Export…", true).clicked() {
                            export_clicked = true;
                        }
                        if widgets::dialog_button(ui, theme, "Reset", false).clicked() {
                            reset_clicked = true;
                        }
                    });
                });
            });
        });
    });

    if reset_clicked {
        panel.params = defaults;
    }

    // Debounce window for the expensive scene re-render: fires after the drag
    // pauses this long, or immediately on release.
    const SCENE_DEBOUNCE: f64 = 0.1;
    let now = ctx.input(|i| i.time);

    // Route knob changes: scene knobs → GPU re-render (debounced while a slider
    // is dragged), everything else → instant CPU recomposite.
    if panel.params != before {
        if panel.params.scene_differs(&before) {
            if scene_dragging {
                panel.defer_scene_render(now);
            } else {
                panel.note_change(true);
            }
        } else {
            panel.note_change(false);
        }
    }
    if scene_dragging {
        // Mid-drag: fire once the value has been still for the debounce window.
        // Request a repaint so the timer still elapses during a motionless hold.
        if panel.scene_due(now, SCENE_DEBOUNCE) {
            panel.flush_scene_render();
        } else if panel.has_pending_scene() {
            ctx.request_repaint_after(std::time::Duration::from_secs_f64(SCENE_DEBOUNCE));
        }
    } else {
        // Released (or never dragging) → fire any deferred render now.
        panel.flush_scene_render();
    }

    let esc = ctx.input(|i| i.key_pressed(egui::Key::Escape));
    if export_clicked {
        let start_dir = last_dir.clone();
        pending.spawn(async move {
            dialogs::new_dialog(&start_dir)
                .add_filter("PNG image", &["png"])
                .set_file_name("roxel-shot.png")
                .save_file()
                .await
                .map(|f| DialogResult::ExportShot(f.path().to_path_buf()))
        });
        panel.open = false;
    } else if !open_flag || esc {
        panel.open = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ui_scale_chips_cover_every_offered_scale() {
        let scales: Vec<u16> = UI_SCALE_LABELS.iter().map(|(s, _)| *s).collect();
        assert_eq!(scales, crate::theme::UI_SCALES);
        for (scale, label) in UI_SCALE_LABELS {
            assert_eq!(label, format!("{scale}%"));
        }
    }

    #[test]
    fn unsaved_copy_names_the_action_and_document() {
        let (h, p) = unsaved_copy(&GuardedAction::Quit, "castle.rox");
        assert_eq!(h, "Quit Roxel");
        assert_eq!(p, "Save changes to “castle.rox” before quitting?");
        let (h, _) = unsaved_copy(&GuardedAction::Open(Some("/x.rox".into())), "Untitled");
        assert_eq!(h, "Open project");
        let (h, p) = unsaved_copy(&GuardedAction::New, "Untitled");
        assert_eq!(h, "New project");
        assert!(p.contains("“Untitled”"));
    }
}
