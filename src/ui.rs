use crate::grid::VoxelGrid;
use crate::history::History;
use crate::io;
use crate::tools::{CurrentColor, RecentColors, Tool, ToolState};
use bevy::prelude::*;
use bevy::tasks::{AsyncComputeTaskPool, Task, block_on, futures_lite::future};
use bevy_egui::{EguiContexts, egui};
use std::path::PathBuf;

pub enum DialogResult {
    OpenProject(PathBuf),
    SaveProject(PathBuf),
    ExportVox(PathBuf),
    ExportObj(PathBuf),
}

#[derive(Resource, Default)]
pub struct PendingDialog(pub Option<Task<Option<DialogResult>>>);

impl PendingDialog {
    fn is_active(&self) -> bool {
        self.0.is_some()
    }
    fn spawn<F>(&mut self, fut: F)
    where
        F: std::future::Future<Output = Option<DialogResult>> + Send + 'static,
    {
        self.0 = Some(AsyncComputeTaskPool::get().spawn(fut));
    }
}

pub fn poll_dialogs_system(
    mut pending: ResMut<PendingDialog>,
    mut grid: ResMut<VoxelGrid>,
    mut history: ResMut<History>,
) {
    let Some(task) = pending.0.as_mut() else { return; };
    let Some(result) = block_on(future::poll_once(task)) else { return; };
    pending.0 = None;
    match result {
        Some(DialogResult::OpenProject(path)) => {
            if let Err(e) = io::project::load(&path, &mut grid) {
                eprintln!("Open failed: {e:?}");
            } else {
                history.undo.clear();
                history.redo.clear();
            }
        }
        Some(DialogResult::SaveProject(path)) => {
            if let Err(e) = io::project::save(&path, &grid) {
                eprintln!("Save failed: {e:?}");
            }
        }
        Some(DialogResult::ExportVox(path)) => {
            if let Err(e) = io::vox::export(&path, &grid) {
                eprintln!("Export .vox failed: {e:?}");
            }
        }
        Some(DialogResult::ExportObj(path)) => {
            if let Err(e) = io::obj::export(&path, &grid) {
                eprintln!("Export .obj failed: {e:?}");
            }
        }
        None => {}
    }
}

const BG: egui::Color32 = egui::Color32::from_rgb(18, 20, 24);
const PANEL: egui::Color32 = egui::Color32::from_rgb(26, 28, 34);
const SURFACE: egui::Color32 = egui::Color32::from_rgb(38, 42, 50);
const SURFACE_HOVER: egui::Color32 = egui::Color32::from_rgb(54, 60, 72);
const ACCENT: egui::Color32 = egui::Color32::from_rgb(110, 165, 255);
const ACCENT_DIM: egui::Color32 = egui::Color32::from_rgb(60, 95, 155);
const TEXT: egui::Color32 = egui::Color32::from_rgb(220, 225, 235);
const TEXT_DIM: egui::Color32 = egui::Color32::from_rgb(150, 158, 172);
const BORDER: egui::Color32 = egui::Color32::from_rgb(44, 48, 58);

fn icon_brush() -> egui::ImageSource<'static> {
    egui::include_image!("../assets/icons/brush.svg")
}
fn icon_eraser() -> egui::ImageSource<'static> {
    egui::include_image!("../assets/icons/eraser.svg")
}
fn icon_paint_bucket() -> egui::ImageSource<'static> {
    egui::include_image!("../assets/icons/paint-bucket.svg")
}
fn icon_pipette() -> egui::ImageSource<'static> {
    egui::include_image!("../assets/icons/pipette.svg")
}
fn icon_file_plus() -> egui::ImageSource<'static> {
    egui::include_image!("../assets/icons/file-plus.svg")
}
fn icon_folder_open() -> egui::ImageSource<'static> {
    egui::include_image!("../assets/icons/folder-open.svg")
}
fn icon_save() -> egui::ImageSource<'static> {
    egui::include_image!("../assets/icons/save.svg")
}
fn icon_download() -> egui::ImageSource<'static> {
    egui::include_image!("../assets/icons/download.svg")
}
fn icon_undo() -> egui::ImageSource<'static> {
    egui::include_image!("../assets/icons/undo.svg")
}
fn icon_redo() -> egui::ImageSource<'static> {
    egui::include_image!("../assets/icons/redo.svg")
}

fn tool_icon(t: Tool) -> egui::ImageSource<'static> {
    match t {
        Tool::Brush => icon_brush(),
        Tool::Erase => icon_eraser(),
        Tool::Paint => icon_paint_bucket(),
        Tool::Eyedropper => icon_pipette(),
    }
}

pub fn apply_style(mut contexts: EguiContexts) -> Result {
    let ctx = contexts.ctx_mut()?;
    egui_extras::install_image_loaders(ctx);
    let mut visuals = egui::Visuals::dark();

    visuals.override_text_color = Some(TEXT);
    visuals.panel_fill = PANEL;
    visuals.window_fill = PANEL;
    visuals.extreme_bg_color = BG;
    visuals.faint_bg_color = egui::Color32::from_rgb(32, 35, 42);

    visuals.widgets.noninteractive.bg_fill = PANEL;
    visuals.widgets.noninteractive.weak_bg_fill = PANEL;
    visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0, BORDER);
    visuals.widgets.noninteractive.fg_stroke = egui::Stroke::new(1.0, TEXT_DIM);
    visuals.widgets.noninteractive.corner_radius = egui::CornerRadius::same(6);

    visuals.widgets.inactive.bg_fill = SURFACE;
    visuals.widgets.inactive.weak_bg_fill = SURFACE;
    visuals.widgets.inactive.bg_stroke = egui::Stroke::NONE;
    visuals.widgets.inactive.fg_stroke = egui::Stroke::new(1.0, TEXT);
    visuals.widgets.inactive.corner_radius = egui::CornerRadius::same(6);

    visuals.widgets.hovered.bg_fill = SURFACE_HOVER;
    visuals.widgets.hovered.weak_bg_fill = SURFACE_HOVER;
    visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1.0, ACCENT_DIM);
    visuals.widgets.hovered.fg_stroke = egui::Stroke::new(1.0, TEXT);
    visuals.widgets.hovered.corner_radius = egui::CornerRadius::same(6);

    visuals.widgets.active.bg_fill = ACCENT;
    visuals.widgets.active.weak_bg_fill = ACCENT;
    visuals.widgets.active.bg_stroke = egui::Stroke::new(1.0, ACCENT);
    visuals.widgets.active.fg_stroke = egui::Stroke::new(1.0, egui::Color32::WHITE);
    visuals.widgets.active.corner_radius = egui::CornerRadius::same(6);

    visuals.widgets.open.bg_fill = SURFACE_HOVER;
    visuals.widgets.open.weak_bg_fill = SURFACE_HOVER;
    visuals.widgets.open.corner_radius = egui::CornerRadius::same(6);

    visuals.selection.bg_fill = ACCENT_DIM;
    visuals.selection.stroke = egui::Stroke::new(1.0, ACCENT);
    visuals.hyperlink_color = ACCENT;
    visuals.window_corner_radius = egui::CornerRadius::same(10);
    visuals.menu_corner_radius = egui::CornerRadius::same(8);
    visuals.window_stroke = egui::Stroke::new(1.0, BORDER);
    visuals.window_shadow = egui::epaint::Shadow {
        offset: [0, 4],
        blur: 12,
        spread: 0,
        color: egui::Color32::from_black_alpha(120),
    };
    visuals.popup_shadow = visuals.window_shadow;

    ctx.set_visuals(visuals);

    let mut style: egui::Style = (*ctx.style()).clone();
    style.spacing.item_spacing = egui::vec2(8.0, 8.0);
    style.spacing.button_padding = egui::vec2(12.0, 7.0);
    style.spacing.menu_margin = egui::Margin::same(8);
    style.spacing.window_margin = egui::Margin::same(12);
    style.spacing.slider_width = 160.0;
    style.spacing.interact_size.y = 26.0;

    // Heading slightly tighter and accent-tinted.
    if let Some(h) = style.text_styles.get_mut(&egui::TextStyle::Heading) {
        h.size = 15.0;
    }
    ctx.set_style(style);
    Ok(())
}

pub fn ui_system(
    mut contexts: EguiContexts,
    mut tool: ResMut<ToolState>,
    mut color: ResMut<CurrentColor>,
    recent: Res<RecentColors>,
    mut grid: ResMut<VoxelGrid>,
    mut history: ResMut<History>,
    mut pending: ResMut<PendingDialog>,
) -> Result {
    let ctx = contexts.ctx_mut()?;
    egui_extras::install_image_loaders(ctx);

    // ---------- Top bar ----------
    egui::TopBottomPanel::top("top_bar")
        .frame(
            egui::Frame::default()
                .fill(PANEL)
                .inner_margin(egui::Margin::symmetric(12, 8))
                .stroke(egui::Stroke::new(1.0, BORDER)),
        )
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                if icon_button(ui, icon_file_plus(), "New")
                    .on_hover_text("Clear the scene")
                    .clicked()
                {
                    grid.clear();
                    history.undo.clear();
                    history.redo.clear();
                }
                let dialog_busy = pending.is_active();
                if ui
                    .add_enabled(!dialog_busy, egui::Button::image_and_text(
                        egui::Image::new(icon_folder_open())
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
                        egui::Image::new(icon_save())
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
                    egui::Image::new(icon_download())
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
                });

                ui.add_space(8.0);
                vertical_rule(ui);
                ui.add_space(4.0);

                let undo_enabled = !history.undo.is_empty();
                if ui
                    .add_enabled(
                        undo_enabled,
                        egui::Button::image_and_text(
                            egui::Image::new(icon_undo())
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
                            egui::Image::new(icon_redo())
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

            });
        });

    // ---------- Bottom status bar ----------
    egui::TopBottomPanel::bottom("status_bar")
        .frame(
            egui::Frame::default()
                .fill(BG)
                .inner_margin(egui::Margin::symmetric(12, 6))
                .stroke(egui::Stroke::new(1.0, BORDER)),
        )
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(tool_label(tool.current))
                        .color(ACCENT)
                        .size(12.0),
                );
                ui.add_space(12.0);
                ui.label(
                    egui::RichText::new(format!(
                        "{n} voxel{s}",
                        n = grid.count(),
                        s = if grid.count() == 1 { "" } else { "s" }
                    ))
                    .color(TEXT_DIM)
                    .size(12.0),
                );
                ui.add_space(12.0);
                ui.label(
                    egui::RichText::new(format!(
                        "Grid {g}×{g}×{g}",
                        g = crate::grid::GRID
                    ))
                    .color(TEXT_DIM)
                    .size(12.0),
                );
            });
        });

    // ---------- Left tool rail ----------
    egui::SidePanel::left("tools")
        .resizable(false)
        .exact_width(56.0)
        .frame(
            egui::Frame::default()
                .fill(PANEL)
                .inner_margin(egui::Margin::symmetric(8, 10))
                .stroke(egui::Stroke::new(1.0, BORDER)),
        )
        .show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                tool_button(ui, &mut tool, Tool::Brush, "Brush", "B");
                ui.add_space(4.0);
                tool_button(ui, &mut tool, Tool::Erase, "Erase", "E");
                ui.add_space(4.0);
                tool_button(ui, &mut tool, Tool::Paint, "Paint", "P");
                ui.add_space(4.0);
                tool_button(ui, &mut tool, Tool::Eyedropper, "Pick", "I");
            });
        });

    // ---------- Right inspector ----------
    egui::SidePanel::right("right_panel")
        .resizable(true)
        .default_width(260.0)
        .min_width(240.0)
        .frame(
            egui::Frame::default()
                .fill(PANEL)
                .inner_margin(egui::Margin::same(12))
                .stroke(egui::Stroke::new(1.0, BORDER)),
        )
        .show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                // Color section
                section(ui, "Color", |ui| {
                    let mut srgba = egui::Color32::from_rgba_unmultiplied(
                        color.0[0], color.0[1], color.0[2], color.0[3],
                    );
                    ui.horizontal(|ui| {
                        // Large swatch preview
                        let (rect, _) = ui.allocate_exact_size(
                            egui::vec2(48.0, 48.0),
                            egui::Sense::hover(),
                        );
                        ui.painter().rect_filled(rect, 8.0, srgba);
                        ui.painter()
                            .rect_stroke(rect, 8.0, egui::Stroke::new(1.0, BORDER), egui::StrokeKind::Inside);
                        ui.vertical(|ui| {
                            ui.add_space(2.0);
                            ui.label(
                                egui::RichText::new(format!(
                                    "#{:02X}{:02X}{:02X}",
                                    color.0[0], color.0[1], color.0[2]
                                ))
                                .monospace()
                                .size(14.0),
                            );
                            ui.label(
                                egui::RichText::new(format!(
                                    "rgb({}, {}, {})",
                                    color.0[0], color.0[1], color.0[2]
                                ))
                                .color(TEXT_DIM)
                                .size(11.0),
                            );
                            ui.add_space(2.0);
                            if egui::color_picker::color_edit_button_srgba(
                                ui,
                                &mut srgba,
                                egui::color_picker::Alpha::Opaque,
                            )
                            .changed()
                            {
                                color.0 = [srgba.r(), srgba.g(), srgba.b(), 255];
                            }
                        });
                    });

                    ui.add_space(8.0);
                    ui.label(egui::RichText::new("Recent").color(TEXT_DIM).size(11.0));
                    ui.add_space(4.0);
                    ui.horizontal_wrapped(|ui| {
                        for c in &recent.0 {
                            let col = egui::Color32::from_rgba_unmultiplied(c[0], c[1], c[2], 255);
                            let is_current = color.0 == *c;
                            let (rect, resp) = ui.allocate_exact_size(
                                egui::vec2(24.0, 24.0),
                                egui::Sense::click(),
                            );
                            ui.painter().rect_filled(rect, 5.0, col);
                            let stroke = if is_current {
                                egui::Stroke::new(2.0, ACCENT)
                            } else if resp.hovered() {
                                egui::Stroke::new(1.0, TEXT)
                            } else {
                                egui::Stroke::new(1.0, BORDER)
                            };
                            ui.painter().rect_stroke(
                                rect,
                                5.0,
                                stroke,
                                egui::StrokeKind::Inside,
                            );
                            if resp.clicked() {
                                color.0 = *c;
                            }
                            resp.on_hover_text(format!(
                                "#{:02X}{:02X}{:02X}",
                                c[0], c[1], c[2]
                            ));
                        }
                        if recent.0.is_empty() {
                            ui.label(
                                egui::RichText::new("—")
                                    .color(TEXT_DIM)
                                    .size(12.0),
                            );
                        }
                    });
                });

                // Scene section
                section(ui, "Scene", |ui| {
                    stat_row(ui, "Voxels", grid.count().to_string());
                    stat_row(
                        ui,
                        "Grid",
                        format!(
                            "{g} × {g} × {g}",
                            g = crate::grid::GRID
                        ),
                    );
                    stat_row(ui, "Undo", history.undo.len().to_string());
                    stat_row(ui, "Redo", history.redo.len().to_string());
                });
            });
        });

    // Reflect tool in cursor when pointer is over the viewport.
    if !ctx.is_pointer_over_area() {
        ctx.set_cursor_icon(cursor_for_tool(tool.current));
    }

    Ok(())
}

fn cursor_for_tool(t: Tool) -> egui::CursorIcon {
    match t {
        Tool::Brush => egui::CursorIcon::Crosshair,
        Tool::Erase => egui::CursorIcon::NotAllowed,
        Tool::Paint => egui::CursorIcon::Cell,
        Tool::Eyedropper => egui::CursorIcon::Copy,
    }
}

fn vertical_rule(ui: &mut egui::Ui) {
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(1.0, 20.0), egui::Sense::hover());
    ui.painter()
        .vline(rect.center().x, rect.y_range(), egui::Stroke::new(1.0, BORDER));
}

fn tool_label(t: Tool) -> &'static str {
    match t {
        Tool::Brush => "Brush",
        Tool::Erase => "Erase",
        Tool::Paint => "Paint",
        Tool::Eyedropper => "Pick",
    }
}

fn tool_button(
    ui: &mut egui::Ui,
    tool: &mut ToolState,
    kind: Tool,
    label: &str,
    shortcut: &str,
) {
    let active = tool.current == kind;
    let size = egui::vec2(40.0, 40.0);
    let (rect, resp) = ui.allocate_exact_size(size, egui::Sense::click());

    let (fill, stroke, fg) = if active {
        (ACCENT, egui::Stroke::new(0.5, ACCENT), egui::Color32::WHITE)
    } else if resp.hovered() {
        (SURFACE_HOVER, egui::Stroke::new(0.5, ACCENT_DIM), TEXT)
    } else {
        (SURFACE, egui::Stroke::new(0.5, BORDER), TEXT)
    };

    ui.painter().rect_filled(rect, 6.0, fill);
    ui.painter()
        .rect_stroke(rect, 6.0, stroke, egui::StrokeKind::Inside);

    let icon_size = 18.0;
    let icon_rect = egui::Rect::from_center_size(
        rect.center(),
        egui::vec2(icon_size, icon_size),
    );
    egui::Image::new(tool_icon(kind))
        .fit_to_exact_size(egui::vec2(icon_size, icon_size))
        .tint(fg)
        .paint_at(ui, icon_rect);

    if resp.clicked() && tool.current != kind {
        tool.previous = tool.current;
        tool.current = kind;
    }
    resp.on_hover_text(format!("{label}  ({shortcut})"));
}

fn icon_button(
    ui: &mut egui::Ui,
    icon: egui::ImageSource<'static>,
    label: &str,
) -> egui::Response {
    ui.add(egui::Button::image_and_text(
        egui::Image::new(icon)
            .fit_to_exact_size(egui::vec2(14.0, 14.0))
            .tint(TEXT),
        egui::RichText::new(label).size(13.0),
    ))
}

fn section<R>(ui: &mut egui::Ui, title: &str, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    ui.add_space(4.0);
    ui.label(
        egui::RichText::new(title.to_uppercase())
            .color(TEXT_DIM)
            .size(11.0)
            .strong(),
    );
    ui.add_space(4.0);
    let r = egui::Frame::default()
        .fill(egui::Color32::from_rgb(32, 35, 42))
        .stroke(egui::Stroke::new(1.0, BORDER))
        .corner_radius(egui::CornerRadius::same(8))
        .inner_margin(egui::Margin::same(10))
        .show(ui, |ui| add(ui))
        .inner;
    ui.add_space(12.0);
    r
}

fn stat_row(ui: &mut egui::Ui, label: &str, value: String) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(label).color(TEXT_DIM).size(12.0));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(
                egui::RichText::new(value)
                    .monospace()
                    .color(TEXT)
                    .size(12.0),
            );
        });
    });
}
