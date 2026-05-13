mod dialogs;
mod icons;
mod palette;
mod widgets;

pub use dialogs::{DialogResult, PendingDialog, poll_dialogs_system};
pub use palette::{Palette, PaletteChoice, Palettes};

use crate::gizmo::{GizmoDrag, GizmoRect};
use crate::grid::{ALLOWED_SIZES, NewProject, VoxelGrid};
use crate::history::History;
use crate::io;
use crate::shapes::ShapePrimitive;
use crate::theme::{
    CanvasBgPref, NUNITO_700_FAMILY, Preferences, PreferencesWindow, Theme, ThemePref,
    apply_egui_style, save_preferences,
};
use crate::tools::{CurrentColor, RecentColors, ShapeOptions, Tool, ToolState};
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};
use bevy_panorbit_camera::PanOrbitCamera;
use palette::PaletteParams;


#[derive(SystemParam)]
pub struct ZoomReadout<'w, 's> {
    cameras: Query<'w, 's, &'static PanOrbitCamera>,
}

#[derive(SystemParam)]
pub struct PrefsParams<'w> {
    pub prefs: ResMut<'w, Preferences>,
    pub window: ResMut<'w, PreferencesWindow>,
}

#[derive(SystemParam)]
pub struct GizmoView<'w> {
    pub rect: Res<'w, GizmoRect>,
    pub drag: Res<'w, GizmoDrag>,
}

pub fn ui_system(
    mut contexts: EguiContexts,
    mut tool: ResMut<ToolState>,
    mut color: ResMut<CurrentColor>,
    recent: Res<RecentColors>,
    #[cfg_attr(target_os = "macos", allow(unused_mut))] mut grid: ResMut<VoxelGrid>,
    #[cfg_attr(target_os = "macos", allow(unused_mut))] mut history: ResMut<History>,
    mut pending: ResMut<PendingDialog>,
    palette_params: PaletteParams,
    theme: Res<Theme>,
    prefs_params: PrefsParams,
    mut shape_options: ResMut<ShapeOptions>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    zoom: ZoomReadout,
    mut new_project: ResMut<NewProject>,
    gizmo_view: GizmoView,
) -> Result {
    let ctx = contexts.ctx_mut()?;
    egui_extras::install_image_loaders(ctx);
    apply_egui_style(ctx, &theme);

    let PaletteParams {
        mut palettes,
        choice: mut palette_choice,
        rename: mut palette_rename,
    } = palette_params;
    let PrefsParams {
        mut prefs,
        window: mut prefs_window,
    } = prefs_params;

    // Local bindings shadow the previous module-level constants so that the
    // rest of this function can stay as it was.
    #[allow(non_snake_case)]
    let BG = theme.bg;
    #[allow(non_snake_case)]
    let PANEL = theme.panel;
    #[allow(non_snake_case)]
    let ACCENT = theme.accent;
    #[allow(non_snake_case, unused_variables)]
    let TEXT = theme.text;
    #[allow(non_snake_case)]
    let TEXT_DIM = theme.text_dim;
    #[allow(non_snake_case)]
    let BORDER = theme.border;

    // ---------- Top bar ----------
    // On macOS the native menu bar (see `menu.rs`) replaces these controls.
    #[cfg(not(target_os = "macos"))]
    egui::TopBottomPanel::top("top_bar")
        .frame(
            egui::Frame::default()
                .fill(PANEL)
                .inner_margin(egui::Margin::symmetric(12, 8))
                .stroke(egui::Stroke::new(0.5, BORDER)),
        )
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                if widgets::icon_button(ui, &theme, icons::file_plus(), "New")
                    .on_hover_text("Start a new project")
                    .clicked()
                {
                    new_project.picker_size = grid.size;
                    new_project.dialog_open = true;
                }
                let dialog_busy = pending.is_active();
                if ui
                    .add_enabled(!dialog_busy, egui::Button::image_and_text(
                        egui::Image::new(icons::folder_open())
                            .fit_to_exact_size(egui::vec2(14.0, 14.0))
                            .tint(if dialog_busy { TEXT_DIM } else { TEXT }),
                        egui::RichText::new("Open…").size(13.0),
                    ))
                    .clicked()
                {
                    pending.spawn(async move {
                        rfd::AsyncFileDialog::new()
                            .add_filter("Roxel project", &["roxel"])
                            .pick_file()
                            .await
                            .map(|f| DialogResult::OpenProject(f.path().to_path_buf()))
                    });
                }
                if ui
                    .add_enabled(!dialog_busy, egui::Button::image_and_text(
                        egui::Image::new(icons::save())
                            .fit_to_exact_size(egui::vec2(14.0, 14.0))
                            .tint(if dialog_busy { TEXT_DIM } else { TEXT }),
                        egui::RichText::new("Save…").size(13.0),
                    ))
                    .clicked()
                {
                    pending.spawn(async move {
                        rfd::AsyncFileDialog::new()
                            .add_filter("Roxel project", &["roxel"])
                            .set_file_name("scene.roxel")
                            .save_file()
                            .await
                            .map(|f| DialogResult::SaveProject(f.path().to_path_buf()))
                    });
                }
                ui.menu_image_text_button(
                    egui::Image::new(icons::download())
                        .fit_to_exact_size(egui::vec2(14.0, 14.0))
                        .tint(TEXT),
                    egui::RichText::new("Export").size(13.0),
                    |ui| {
                        ui.set_min_width(180.0);
                    if ui.add_enabled(!dialog_busy, egui::Button::new("MagicaVoxel .vox…")).clicked() {
                        pending.spawn(async move {
                            rfd::AsyncFileDialog::new()
                                .add_filter("MagicaVoxel", &["vox"])
                                .set_file_name("model.vox")
                                .save_file()
                                .await
                                .map(|f| DialogResult::ExportVox(f.path().to_path_buf()))
                        });
                        ui.close();
                    }
                    if ui.add_enabled(!dialog_busy, egui::Button::new("Wavefront .obj…")).clicked() {
                        pending.spawn(async move {
                            rfd::AsyncFileDialog::new()
                                .add_filter("Wavefront OBJ", &["obj"])
                                .set_file_name("model.obj")
                                .save_file()
                                .await
                                .map(|f| DialogResult::ExportObj(f.path().to_path_buf()))
                        });
                        ui.close();
                    }
                    if ui.add_enabled(!dialog_busy, egui::Button::new("Autodesk .fbx…")).clicked() {
                        pending.spawn(async move {
                            rfd::AsyncFileDialog::new()
                                .add_filter("Autodesk FBX", &["fbx"])
                                .set_file_name("model.fbx")
                                .save_file()
                                .await
                                .map(|f| DialogResult::ExportFbx(f.path().to_path_buf()))
                        });
                        ui.close();
                    }
                    if ui.add_enabled(!dialog_busy, egui::Button::new("Transparent PNG…")).clicked() {
                        pending.spawn(async move {
                            rfd::AsyncFileDialog::new()
                                .add_filter("PNG image", &["png"])
                                .set_file_name("roxel.png")
                                .save_file()
                                .await
                                .map(|f| DialogResult::ExportPng(f.path().to_path_buf()))
                        });
                        ui.close();
                    }
                    if ui.add_enabled(!dialog_busy, egui::Button::new("SVG…")).clicked() {
                        pending.spawn(async move {
                            rfd::AsyncFileDialog::new()
                                .add_filter("SVG image", &["svg"])
                                .set_file_name("roxel.svg")
                                .save_file()
                                .await
                                .map(|f| DialogResult::ExportSvg(f.path().to_path_buf()))
                        });
                        ui.close();
                    }
                });

                ui.add_space(8.0);
                widgets::vertical_rule(ui, &theme);
                ui.add_space(4.0);

                let undo_enabled = !history.undo.is_empty();
                if ui
                    .add_enabled(
                        undo_enabled,
                        egui::Button::image_and_text(
                            egui::Image::new(icons::undo())
                                .fit_to_exact_size(egui::vec2(14.0, 14.0))
                                .tint(if undo_enabled { TEXT } else { TEXT_DIM }),
                            egui::RichText::new("Undo").size(13.0),
                        ),
                    )
                    .on_hover_text("Cmd+Z / Ctrl+Z")
                    .clicked()
                {
                    history.undo(&mut grid);
                }
                let redo_enabled = !history.redo.is_empty();
                if ui
                    .add_enabled(
                        redo_enabled,
                        egui::Button::image_and_text(
                            egui::Image::new(icons::redo())
                                .fit_to_exact_size(egui::vec2(14.0, 14.0))
                                .tint(if redo_enabled { TEXT } else { TEXT_DIM }),
                            egui::RichText::new("Redo").size(13.0),
                        ),
                    )
                    .on_hover_text("Cmd+Shift+Z / Ctrl+Shift+Z")
                    .clicked()
                {
                    history.redo(&mut grid);
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .add(egui::Button::new(
                            egui::RichText::new("Preferences…").size(13.0),
                        ))
                        .on_hover_text("Appearance and other settings")
                        .clicked()
                    {
                        prefs_window.open = !prefs_window.open;
                    }
                });
            });
        });

    // ---------- Bottom status bar ----------
    egui::TopBottomPanel::bottom("status_bar")
        .frame(
            egui::Frame::default()
                .fill(BG)
                .inner_margin(egui::Margin::symmetric(12, 6))
                .stroke(egui::Stroke::new(0.5, BORDER)),
        )
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(widgets::tool_label(tool.current))
                            .color(ACCENT)
                            .size(12.0),
                    )
                    .selectable(true),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(format!("Grid {g}×{g}×{g}", g = grid.size))
                                .color(TEXT_DIM)
                                .size(12.0),
                        )
                        .selectable(true),
                    );
                    ui.add_space(12.0);
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(format!(
                                "{n} voxel{s}",
                                n = grid.count(),
                                s = if grid.count() == 1 { "" } else { "s" }
                            ))
                            .color(TEXT_DIM)
                            .size(12.0),
                        )
                        .selectable(true),
                    );
                    if let Some((_, fit_radius)) = crate::camera::fit_view(&grid)
                        && let Some(cam) = zoom.cameras.iter().next()
                    {
                        let zoom_pct =
                            (fit_radius / cam.target_radius.max(0.0001) * 100.0).round() as i32;
                        ui.add_space(12.0);
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(format!("Zoom {zoom_pct}%"))
                                    .color(TEXT_DIM)
                                    .size(12.0),
                            )
                            .selectable(true),
                        );
                    }
                });
            });
        });

    // ---------- Left tool rail ----------
    let left_resp = egui::SidePanel::left("tools")
        .resizable(false)
        .exact_width(56.0)
        .frame(
            egui::Frame::default()
                .fill(PANEL)
                .inner_margin(egui::Margin::symmetric(8, 10)),
        )
        .show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                widgets::tool_button(ui, &theme, &mut tool, Tool::Brush, "Brush", "B");
                ui.add_space(4.0);
                widgets::tool_button(ui, &theme, &mut tool, Tool::Erase, "Erase", "E");
                ui.add_space(4.0);
                widgets::tool_button(ui, &theme, &mut tool, Tool::Paint, "Paint", "P");
                ui.add_space(4.0);
                widgets::tool_button(ui, &theme, &mut tool, Tool::Shape, "Shape", "S");
                ui.add_space(4.0);
                widgets::tool_button(ui, &theme, &mut tool, Tool::Eyedropper, "Pick", "I");
            });
        });
    let left_rect = left_resp.response.rect;
    ctx.layer_painter(egui::LayerId::new(
        egui::Order::Middle,
        egui::Id::new("left_panel_edge"),
    ))
    .vline(
        left_rect.right(),
        left_rect.y_range(),
        egui::Stroke::new(0.5, BORDER),
    );

    // ---------- Right inspector ----------
    let right_resp = egui::SidePanel::right("right_panel")
        .resizable(true)
        .default_width(260.0)
        .min_width(240.0)
        .frame(
            egui::Frame::default()
                .fill(PANEL)
                .inner_margin(egui::Margin {
                    left: 12,
                    right: 0,
                    top: 12,
                    bottom: 12,
                }),
        )
        .show(ctx, |ui| {
            egui::ScrollArea::vertical().auto_shrink([false, true]).show(ui, |ui| {
                let inner_frame = egui::Frame::default()
                    .inner_margin(egui::Margin {
                        left: 0,
                        right: 12,
                        top: 0,
                        bottom: 0,
                    });
                inner_frame.show(ui, |ui| {
                // Color section
                widgets::section(ui, &theme, "Color", |ui| {
                    let mut srgba = egui::Color32::from_rgba_unmultiplied(
                        color.0[0], color.0[1], color.0[2], color.0[3],
                    );
                    let swatch_w = ui.available_width();
                    let swatch_resp = ui
                        .add_sized(
                            [swatch_w, 56.0],
                            egui::Button::new("")
                                .fill(srgba)
                                .stroke(egui::Stroke::new(0.5, theme.border))
                                .corner_radius(egui::CornerRadius::same(8)),
                        )
                        .on_hover_text("Click to edit color");
                    egui::Popup::menu(&swatch_resp)
                        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
                        .show(|ui| {
                            ui.spacing_mut().slider_width = 275.0;
                            if egui::color_picker::color_picker_color32(
                                ui,
                                &mut srgba,
                                egui::color_picker::Alpha::Opaque,
                            ) {
                                color.0 = [srgba.r(), srgba.g(), srgba.b(), 255];
                            }
                        });
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(format!(
                                    "#{:02X}{:02X}{:02X}",
                                    color.0[0], color.0[1], color.0[2]
                                ))
                                .monospace()
                                .size(13.0),
                            )
                            .selectable(true),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(format!(
                                        "{}, {}, {}",
                                        color.0[0], color.0[1], color.0[2]
                                    ))
                                    .color(TEXT_DIM)
                                    .size(11.0),
                                )
                                .selectable(true),
                            );
                        });
                    });
                });

                if palette_choice.0 >= palettes.0.len() {
                    palette_choice.0 = 0;
                }
                let dialog_busy = pending.is_active();

                // Palette section
                widgets::section(ui, &theme, "Palette", |ui| {
                    let mut active_idx = palette_choice.0;
                    let mut active_is_builtin = palettes.0[active_idx].builtin;

                    // Buffer so widget frames don't overflow and auto-grow the SidePanel.
                    let row_w = (ui.available_width() - 2.0).max(80.0);
                    let selected_label = if active_is_builtin {
                        format!("🔒 {}", palettes.0[active_idx].name)
                    } else {
                        palettes.0[active_idx].name.clone()
                    };
                    egui::ComboBox::from_id_salt("palette_combo")
                        .selected_text(selected_label)
                        .width(row_w)
                        .show_ui(ui, |ui| {
                            for (i, p) in palettes.0.iter().enumerate() {
                                let label = if p.builtin {
                                    format!("🔒 {}", p.name)
                                } else {
                                    p.name.clone()
                                };
                                ui.selectable_value(&mut palette_choice.0, i, label);
                            }
                        });

                    if palette_rename.editing == Some(active_idx) && !active_is_builtin {
                        ui.add_space(4.0);
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 4.0;
                            let resp = ui.add(
                                egui::TextEdit::singleline(&mut palette_rename.buf)
                                    .desired_width((row_w - 76.0).max(60.0)),
                            );
                            if !resp.has_focus() && !resp.lost_focus() {
                                resp.request_focus();
                            }
                            let commit = widgets::icon_only_button(ui, &theme, icons::check(), true)
                                .on_hover_text("Save name")
                                .clicked()
                                || (resp.lost_focus()
                                    && ui.input(|i| i.key_pressed(egui::Key::Enter)));
                            let cancel = widgets::icon_only_button(ui, &theme, icons::x(), true)
                                .on_hover_text("Cancel")
                                .clicked()
                                || ui.input(|i| i.key_pressed(egui::Key::Escape));
                            if commit {
                                let trimmed = palette_rename.buf.trim();
                                if !trimmed.is_empty() {
                                    palettes.0[active_idx].name = trimmed.to_string();
                                    io::palettes::save(&palettes.0);
                                }
                                palette_rename.editing = None;
                                palette_rename.buf.clear();
                            } else if cancel {
                                palette_rename.editing = None;
                                palette_rename.buf.clear();
                            }
                        });
                    }

                    ui.add_space(6.0);
                    // Toolbar: palette mgmt left, .ase IO right
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 4.0;
                        if widgets::icon_only_button(ui, &theme, icons::plus(), true)
                            .on_hover_text("New palette")
                            .clicked()
                        {
                            let name = palette::next_palette_name(&palettes.0);
                            palettes.0.push(Palette {
                                name,
                                colors: Vec::new(),
                                builtin: false,
                            });
                            palette_choice.0 = palettes.0.len() - 1;
                            palette_rename.editing = Some(palette_choice.0);
                            palette_rename.buf =
                                palettes.0[palette_choice.0].name.clone();
                            io::palettes::save(&palettes.0);
                        }
                        if widgets::icon_only_button(ui, &theme, icons::copy(), true)
                            .on_hover_text("Duplicate palette")
                            .clicked()
                        {
                            let src = &palettes.0[active_idx];
                            let base = if src.builtin {
                                src.name.clone()
                            } else {
                                format!("{} copy", src.name)
                            };
                            let name = palette::unique_palette_name(&palettes.0, &base);
                            let copy = Palette {
                                name,
                                colors: src.colors.clone(),
                                builtin: false,
                            };
                            palettes.0.push(copy);
                            palette_choice.0 = palettes.0.len() - 1;
                            io::palettes::save(&palettes.0);
                        }
                        if widgets::icon_only_button(ui, &theme, icons::pencil(), !active_is_builtin)
                            .on_hover_text(if active_is_builtin {
                                "Built-in palettes are read-only"
                            } else {
                                "Rename palette"
                            })
                            .clicked()
                        {
                            palette_rename.editing = Some(active_idx);
                            palette_rename.buf = palettes.0[active_idx].name.clone();
                        }
                        let has_user =
                            palettes.0.iter().filter(|p| !p.builtin).count() > 0;
                        let del_enabled = !active_is_builtin && has_user;
                        if widgets::icon_only_button(ui, &theme, icons::trash(), del_enabled)
                            .on_hover_text(if active_is_builtin {
                                "Built-in palettes can't be deleted"
                            } else {
                                "Delete palette"
                            })
                            .clicked()
                        {
                            palettes.0.remove(active_idx);
                            if palette_choice.0 >= palettes.0.len() {
                                palette_choice.0 =
                                    palettes.0.len().saturating_sub(1);
                            }
                            palette_rename.editing = None;
                            io::palettes::save(&palettes.0);
                        }

                        ui.with_layout(
                            egui::Layout::right_to_left(egui::Align::Center),
                            |ui| {
                                ui.spacing_mut().item_spacing.x = 4.0;
                                let safe_idx = palette_choice
                                    .0
                                    .min(palettes.0.len().saturating_sub(1));
                                let current = &palettes.0[safe_idx];
                                let export_name = current.name.clone();
                                let export_colors = current.colors.clone();
                                let default_filename = format!(
                                    "{}.ase",
                                    palette::sanitize_filename(&current.name)
                                );
                                if widgets::icon_only_button(
                                    ui,
                                    &theme,
                                    icons::download(),
                                    !dialog_busy,
                                )
                                .on_hover_text("Export .ase…")
                                .clicked()
                                {
                                    pending.spawn(async move {
                                        rfd::AsyncFileDialog::new()
                                            .add_filter(
                                                "Adobe Swatch Exchange",
                                                &["ase"],
                                            )
                                            .set_file_name(&default_filename)
                                            .save_file()
                                            .await
                                            .map(|f| {
                                                DialogResult::ExportAse(
                                                    f.path().to_path_buf(),
                                                    export_name,
                                                    export_colors,
                                                )
                                            })
                                    });
                                }
                                if widgets::icon_only_button(
                                    ui,
                                    &theme,
                                    icons::upload(),
                                    !dialog_busy,
                                )
                                .on_hover_text("Import .ase…")
                                .clicked()
                                {
                                    pending.spawn(async move {
                                        rfd::AsyncFileDialog::new()
                                            .add_filter(
                                                "Adobe Swatch Exchange",
                                                &["ase"],
                                            )
                                            .pick_file()
                                            .await
                                            .map(|f| {
                                                DialogResult::ImportAse(
                                                    f.path().to_path_buf(),
                                                )
                                            })
                                    });
                                }
                            },
                        );
                    });

                    // Refresh — toolbar may have added/removed palettes.
                    active_idx = palette_choice
                        .0
                        .min(palettes.0.len().saturating_sub(1));
                    palette_choice.0 = active_idx;
                    active_is_builtin = palettes.0[active_idx].builtin;

                    ui.add_space(6.0);
                    let add_enabled = !active_is_builtin
                        && !palettes.0[active_idx].colors.contains(&color.0);
                    let add_icon = egui::Image::new(icons::plus())
                        .fit_to_exact_size(egui::vec2(13.0, 13.0))
                        .tint(if add_enabled { theme.text } else { theme.text_dim });
                    if ui
                        .scope(|ui| {
                            ui.spacing_mut().button_padding = egui::vec2(8.0, 0.0);
                            ui.spacing_mut().interact_size = egui::vec2(0.0, 0.0);
                            ui.add_enabled(
                                add_enabled,
                                egui::Button::image_and_text(
                                    add_icon,
                                    egui::RichText::new("Add current color").size(11.5),
                                )
                                .min_size(egui::vec2(row_w, 26.0))
                                .corner_radius(egui::CornerRadius::same(5))
                                .stroke(egui::Stroke::new(0.5, theme.border)),
                            )
                        })
                        .inner
                        .on_hover_text(if active_is_builtin {
                            "Duplicate this palette first to edit it"
                        } else if !add_enabled {
                            "Color already in palette"
                        } else {
                            "Add current color as a swatch"
                        })
                        .clicked()
                    {
                        palettes.0[active_idx].colors.push(color.0);
                        io::palettes::save(&palettes.0);
                    }

                    ui.add_space(8.0);
                    let mut reorder: Option<(usize, usize)> = None;
                    let mut remove_idx: Option<usize> = None;
                    let active_palette = palettes.0[active_idx].colors.clone();
                    let editable = !active_is_builtin;
                    if active_palette.is_empty() {
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(if editable {
                                    "No swatches yet — add the current color above"
                                } else {
                                    "Empty palette"
                                })
                                .color(theme.text_dim)
                                .size(11.0)
                                .italics(),
                            )
                            .wrap(),
                        );
                    } else {
                        ui.horizontal_wrapped(|ui| {
                            ui.spacing_mut().button_padding = egui::vec2(0.0, 0.0);
                            ui.spacing_mut().interact_size = egui::vec2(0.0, 0.0);
                            ui.spacing_mut().item_spacing = egui::vec2(5.0, 5.0);
                            for (si, c) in active_palette.iter().enumerate() {
                                let col = egui::Color32::from_rgba_unmultiplied(
                                    c[0], c[1], c[2], 255,
                                );
                                let is_current = color.0 == *c;
                                let stroke = if is_current {
                                    egui::Stroke::new(2.0, ACCENT)
                                } else {
                                    egui::Stroke::new(0.5, theme.border)
                                };
                                let swatch_id =
                                    egui::Id::new(("swatch", active_idx, si));
                                let mut clicked = false;
                                let resp = if editable {
                                    ui.dnd_drag_source(swatch_id, si, |ui| {
                                        let r = ui.add_sized(
                                            [22.0, 22.0],
                                            egui::Button::new("")
                                                .fill(col)
                                                .stroke(stroke)
                                                .corner_radius(
                                                    egui::CornerRadius::same(4),
                                                ),
                                        );
                                        clicked = r.clicked();
                                        r
                                    })
                                    .response
                                } else {
                                    let r = ui.add_sized(
                                        [22.0, 22.0],
                                        egui::Button::new("")
                                            .fill(col)
                                            .stroke(stroke)
                                            .corner_radius(
                                                egui::CornerRadius::same(4),
                                            ),
                                    );
                                    clicked = r.clicked();
                                    r
                                };
                                if clicked {
                                    color.0 = *c;
                                }
                                if editable {
                                    egui::Popup::context_menu(&resp).show(|ui| {
                                        if ui.button("Remove").clicked() {
                                            remove_idx = Some(si);
                                            ui.close();
                                        }
                                    });
                                }
                                resp.clone().on_hover_text(format!(
                                    "#{:02X}{:02X}{:02X}{}",
                                    c[0],
                                    c[1],
                                    c[2],
                                    if editable {
                                        "  (drag to reorder, right-click to remove)"
                                    } else {
                                        ""
                                    },
                                ));
                                if editable {
                                    if let (Some(payload), true) = (
                                        resp.dnd_release_payload::<usize>(),
                                        resp.dnd_hover_payload::<usize>().is_some()
                                            || resp
                                                .dnd_release_payload::<usize>()
                                                .is_some(),
                                    ) {
                                        let from = *payload;
                                        if from != si {
                                            reorder = Some((from, si));
                                        }
                                    }
                                }
                            }
                        });
                    }
                    if let Some(i) = remove_idx {
                        palettes.0[active_idx].colors.remove(i);
                        io::palettes::save(&palettes.0);
                    }
                    if let Some((from, to)) = reorder {
                        let colors = &mut palettes.0[active_idx].colors;
                        if from < colors.len() && to < colors.len() {
                            let c = colors.remove(from);
                            colors.insert(to, c);
                            io::palettes::save(&palettes.0);
                        }
                    }
                });

                // Recent section
                widgets::section(ui, &theme, "Recent", |ui| {
                    if recent.0.is_empty() {
                        ui.label(
                            egui::RichText::new("No recent colors")
                                .color(theme.text_dim)
                                .size(11.0)
                                .italics(),
                        );
                    } else {
                        ui.horizontal_wrapped(|ui| {
                            ui.spacing_mut().button_padding = egui::vec2(0.0, 0.0);
                            ui.spacing_mut().interact_size = egui::vec2(0.0, 0.0);
                            ui.spacing_mut().item_spacing = egui::vec2(5.0, 5.0);
                            for c in &recent.0 {
                                let col = egui::Color32::from_rgba_unmultiplied(
                                    c[0], c[1], c[2], 255,
                                );
                                let is_current = color.0 == *c;
                                let stroke = if is_current {
                                    egui::Stroke::new(2.0, ACCENT)
                                } else {
                                    egui::Stroke::new(0.5, theme.border)
                                };
                                let resp = ui.add_sized(
                                    [26.0, 26.0],
                                    egui::Button::new("")
                                        .fill(col)
                                        .stroke(stroke)
                                        .corner_radius(egui::CornerRadius::same(5)),
                                );
                                if resp.clicked() {
                                    color.0 = *c;
                                }
                                resp.on_hover_text(format!(
                                    "#{:02X}{:02X}{:02X}",
                                    c[0], c[1], c[2]
                                ));
                            }
                        });
                    }
                });

                // Shape section (only when Shape tool is active)
                if tool.current == Tool::Shape {
                    widgets::section(ui, &theme, "Shape", |ui| {
                        ui.horizontal(|ui| {
                            ui.selectable_value(
                                &mut shape_options.primitive,
                                ShapePrimitive::Rectangle,
                                "Rect",
                            );
                            ui.selectable_value(
                                &mut shape_options.primitive,
                                ShapePrimitive::Ellipse,
                                "Ellipse",
                            );
                            ui.selectable_value(
                                &mut shape_options.primitive,
                                ShapePrimitive::Line,
                                "Line",
                            );
                        });
                        ui.add_space(4.0);
                        if shape_options.primitive != ShapePrimitive::Line {
                            ui.checkbox(&mut shape_options.filled, "Filled");
                        }
                        ui.add_space(4.0);
                        ui.label(
                            egui::RichText::new(
                                "Click + drag to set the footprint, release, drag perpendicular for depth, click to commit. Esc or right-click cancels.",
                            )
                            .color(theme.text_dim)
                            .size(11.0),
                        );
                    });
                }

                // Scene section
                widgets::section(ui, &theme, "Scene", |ui| {
                    widgets::stat_row(ui, &theme, "Voxels", grid.count().to_string());
                    widgets::stat_row(
                        ui,
                        &theme,
                        "Grid",
                        format!(
                            "{g} × {g} × {g}",
                            g = grid.size
                        ),
                    );
                    widgets::stat_row(ui, &theme, "Undo", history.undo.len().to_string());
                    widgets::stat_row(ui, &theme, "Redo", history.redo.len().to_string());
                });
                });
            });
        });
    let right_rect = right_resp.response.rect;
    ctx.layer_painter(egui::LayerId::new(
        egui::Order::Middle,
        egui::Id::new("right_panel_edge"),
    ))
    .vline(
        right_rect.left(),
        right_rect.y_range(),
        egui::Stroke::new(0.5, BORDER),
    );

    // Reflect tool in cursor when pointer is over the viewport.
    if !ctx.is_pointer_over_area() {
        let alt = keys.any_pressed([KeyCode::AltLeft, KeyCode::AltRight]);
        let z = keys.pressed(KeyCode::KeyZ);
        let over_gizmo = gizmo_view.drag.active
            || gizmo_view.rect.0.zip(ctx.pointer_latest_pos()).is_some_and(
                |(r, p)| {
                    p.x >= r.min.x && p.x <= r.max.x && p.y >= r.min.y && p.y <= r.max.y
                },
            );
        let cursor = if gizmo_view.drag.active {
            egui::CursorIcon::Grabbing
        } else if over_gizmo {
            egui::CursorIcon::Grab
        } else if mouse.pressed(MouseButton::Right) {
            egui::CursorIcon::Grabbing
        } else if z {
            if alt {
                egui::CursorIcon::ZoomOut
            } else {
                egui::CursorIcon::ZoomIn
            }
        } else if keys.pressed(KeyCode::Space) {
            if mouse.pressed(MouseButton::Left) {
                egui::CursorIcon::Grabbing
            } else {
                egui::CursorIcon::Grab
            }
        } else if alt {
            egui::CursorIcon::PointingHand
        } else {
            egui::CursorIcon::Crosshair
        };
        ctx.set_cursor_icon(cursor);
    }

    if prefs_window.open {
        let before = *prefs;
        let mut open_flag = true;
        egui::Window::new("Preferences")
            .collapsible(false)
            .resizable(false)
            .open(&mut open_flag)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.set_min_width(280.0);
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new("Appearance")
                        .color(theme.text)
                        .family(egui::FontFamily::Name(NUNITO_700_FAMILY.into())),
                );
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label("Theme");
                    ui.add_space(8.0);
                    ui.radio_value(&mut prefs.theme, ThemePref::System, "System");
                    ui.radio_value(&mut prefs.theme, ThemePref::Light, "Light");
                    ui.radio_value(&mut prefs.theme, ThemePref::Dark, "Dark");
                });
                ui.add_space(12.0);

                ui.label(
                    egui::RichText::new("Canvas")
                        .color(theme.text)
                        .family(egui::FontFamily::Name(NUNITO_700_FAMILY.into())),
                );
                ui.add_space(4.0);

                let mut is_custom = matches!(prefs.canvas_bg, CanvasBgPref::Custom(_));
                ui.horizontal(|ui| {
                    ui.label("Background");
                    ui.add_space(8.0);
                    if ui
                        .radio(!is_custom, "Match theme")
                        .clicked()
                    {
                        prefs.canvas_bg = CanvasBgPref::MatchTheme;
                        is_custom = false;
                    }
                    if ui.radio(is_custom, "Custom").clicked() {
                        let seed = match prefs.canvas_bg {
                            CanvasBgPref::Custom(rgb) => rgb,
                            CanvasBgPref::MatchTheme => {
                                [theme.bg.r(), theme.bg.g(), theme.bg.b()]
                            }
                        };
                        prefs.canvas_bg = CanvasBgPref::Custom(seed);
                        is_custom = true;
                    }
                });
                if let CanvasBgPref::Custom(ref mut rgb) = prefs.canvas_bg {
                    ui.horizontal(|ui| {
                        ui.add_space(8.0);
                        ui.color_edit_button_srgb(rgb);
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(format!(
                                    "#{:02X}{:02X}{:02X}",
                                    rgb[0], rgb[1], rgb[2]
                                ))
                                .monospace(),
                            )
                            .selectable(true),
                        );
                    });
                }
                ui.add_space(4.0);

                widgets::plane_color_row(ui, theme.mode, "Floor", &mut prefs.floor_color);
                widgets::plane_color_row(ui, theme.mode, "Walls", &mut prefs.wall_color);
                ui.add_space(4.0);
                ui.checkbox(&mut prefs.show_floor, "Show bottom plane");
                ui.checkbox(&mut prefs.show_walls, "Show wall planes");
                ui.checkbox(&mut prefs.preview_outline, "Show preview outline");
                ui.add_space(8.0);
            });
        if !open_flag {
            prefs_window.open = false;
        }
        if *prefs != before {
            save_preferences(&prefs);
        }
    }

    // New-project modal. Reborrow `dialog_open` separately so the body can
    // mutate `picker_size` / `apply` on the same resource.
    if new_project.dialog_open {
        let mut open = true;
        let mut create_clicked = false;
        let mut cancel_clicked = false;
        egui::Window::new("New project")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .open(&mut open)
            .show(ctx, |ui| {
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new("Grid size")
                        .color(theme.text_dim)
                        .size(12.0),
                );
                ui.add_space(4.0);
                for &s in &ALLOWED_SIZES {
                    let label = format!("{s} × {s} × {s}");
                    if ui
                        .radio(new_project.picker_size == s, label)
                        .clicked()
                    {
                        new_project.picker_size = s;
                    }
                }
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button("Cancel").clicked() {
                        cancel_clicked = true;
                    }
                    if ui.button("Create").clicked() {
                        create_clicked = true;
                    }
                });
            });
        if create_clicked {
            new_project.apply = Some(new_project.picker_size);
            new_project.dialog_open = false;
        } else if cancel_clicked || !open {
            new_project.dialog_open = false;
        }
    }

    Ok(())
}

