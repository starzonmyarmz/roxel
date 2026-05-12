use crate::grid::VoxelGrid;
use crate::history::History;
use crate::io;
use crate::snapshot::SnapshotRequest;
use crate::theme::{
    CanvasBgPref, NUNITO_700_FAMILY, PlaneColorPref, Preferences, PreferencesWindow, Theme,
    ThemePref, apply_egui_style, plane_match_color, save_preferences,
};
use crate::shapes::ShapePrimitive;
use crate::tools::{CurrentColor, RecentColors, ShapeOptions, Tool, ToolState};
use bevy::prelude::*;
use bevy::tasks::{AsyncComputeTaskPool, Task, block_on, futures_lite::future};
use bevy::window::PrimaryWindow;
use bevy_egui::{EguiContexts, egui};
use bevy_panorbit_camera::PanOrbitCamera;
use std::path::PathBuf;

pub enum DialogResult {
    OpenProject(PathBuf),
    SaveProject(PathBuf),
    ExportVox(PathBuf),
    ExportObj(PathBuf),
    ExportFbx(PathBuf),
    ExportPng(PathBuf),
    ExportSvg(PathBuf),
    ImportAse(PathBuf),
    ExportAse(PathBuf, String, Vec<[u8; 4]>),
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
    mut palettes: ResMut<Palettes>,
    mut palette_choice: ResMut<PaletteChoice>,
    mut snapshot: ResMut<SnapshotRequest>,
    camera: Query<(&GlobalTransform, &Projection), With<PanOrbitCamera>>,
    windows: Query<&Window, With<PrimaryWindow>>,
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
        Some(DialogResult::ExportFbx(path)) => {
            if let Err(e) = io::fbx::export(&path, &grid) {
                eprintln!("Export .fbx failed: {e:?}");
            }
        }
        Some(DialogResult::ExportPng(path)) => {
            snapshot.0 = Some(path);
        }
        Some(DialogResult::ExportSvg(path)) => match (camera.single(), windows.single()) {
            (Ok((xform, projection)), Ok(window)) => {
                let viewport = Vec2::new(window.width(), window.height());
                if let Err(e) = io::svg::export(&path, &grid, xform, projection, viewport) {
                    eprintln!("Export .svg failed: {e:?}");
                }
            }
            (Err(e), _) => eprintln!("Export .svg failed: no camera found: {e:?}"),
            (_, Err(e)) => eprintln!("Export .svg failed: no window: {e:?}"),
        },
        Some(DialogResult::ImportAse(path)) => match io::ase::import(&path) {
            Ok((name, colors)) => {
                if colors.is_empty() {
                    eprintln!("Import .ase: no usable colors found");
                } else {
                    palettes.0.push(Palette { name, colors });
                    palette_choice.0 = palettes.0.len() - 1;
                }
            }
            Err(e) => eprintln!("Import .ase failed: {e:?}"),
        },
        Some(DialogResult::ExportAse(path, name, colors)) => {
            if let Err(e) = io::ase::export(&path, &name, &colors) {
                eprintln!("Export .ase failed: {e:?}");
            }
        }
        None => {}
    }
}


#[derive(Clone)]
pub struct Palette {
    pub name: String,
    pub colors: Vec<[u8; 4]>,
}

macro_rules! hex_palette {
    ($name:expr, $($r:literal $g:literal $b:literal),* $(,)?) => {
        Palette {
            name: String::from($name),
            colors: vec![$([$r, $g, $b, 255u8]),*],
        }
    };
}

#[derive(Resource)]
pub struct Palettes(pub Vec<Palette>);

impl Default for Palettes {
    fn default() -> Self {
        Self(vec![
            hex_palette!(
                "Sweetie 16",
                0x1A 0x1C 0x2C, 0x5D 0x27 0x5D, 0xB1 0x3E 0x53, 0xEF 0x7D 0x57,
                0xFF 0xCD 0x75, 0xA7 0xF0 0x70, 0x38 0xB7 0x64, 0x25 0x71 0x79,
                0x29 0x36 0x6F, 0x3B 0x5D 0xC9, 0x41 0xA6 0xF6, 0x73 0xEF 0xF7,
                0xF4 0xF4 0xF4, 0x94 0xB0 0xC2, 0x56 0x6C 0x86, 0x33 0x3C 0x57,
            ),
            hex_palette!(
                "PICO-8",
                0x00 0x00 0x00, 0x1D 0x2B 0x53, 0x7E 0x25 0x53, 0x00 0x87 0x51,
                0xAB 0x52 0x36, 0x5F 0x57 0x4F, 0xC2 0xC3 0xC7, 0xFF 0xF1 0xE8,
                0xFF 0x00 0x4D, 0xFF 0xA3 0x00, 0xFF 0xEC 0x27, 0x00 0xE4 0x36,
                0x29 0xAD 0xFF, 0x83 0x76 0x9C, 0xFF 0x77 0xA8, 0xFF 0xCC 0xAA,
            ),
            hex_palette!(
                "DawnBringer 16",
                0x14 0x0C 0x1C, 0x44 0x24 0x34, 0x30 0x34 0x6D, 0x4E 0x4A 0x4E,
                0x85 0x4C 0x30, 0x34 0x65 0x24, 0xD0 0x46 0x48, 0x75 0x71 0x61,
                0x59 0x7D 0xCE, 0xD2 0x7D 0x2C, 0x85 0x95 0xA1, 0x6D 0xAA 0x2C,
                0xD2 0xAA 0x99, 0x6D 0xC2 0xCA, 0xDA 0xD4 0x5E, 0xDE 0xEE 0xD6,
            ),
            hex_palette!(
                "DawnBringer 32",
                0x00 0x00 0x00, 0x22 0x20 0x34, 0x45 0x28 0x3C, 0x66 0x39 0x31,
                0x8F 0x56 0x3B, 0xDF 0x71 0x26, 0xD9 0xA0 0x66, 0xEE 0xC3 0x9A,
                0xFB 0xF2 0x36, 0x99 0xE5 0x50, 0x6A 0xBE 0x30, 0x37 0x94 0x6E,
                0x4B 0x69 0x2F, 0x52 0x4B 0x24, 0x32 0x3C 0x39, 0x3F 0x3F 0x74,
                0x30 0x60 0x82, 0x5B 0x6E 0xE1, 0x63 0x9B 0xFF, 0x5F 0xCD 0xE4,
                0xCB 0xDB 0xFC, 0xFF 0xFF 0xFF, 0x9B 0xAD 0xB7, 0x84 0x7E 0x87,
                0x69 0x6A 0x6A, 0x59 0x56 0x52, 0x76 0x42 0x8A, 0xAC 0x32 0x32,
                0xD9 0x57 0x63, 0xD7 0x7B 0xBA, 0x8F 0x97 0x4A, 0x8A 0x6F 0x30,
            ),
            hex_palette!(
                "Endesga 32",
                0xBE 0x4A 0x2F, 0xD7 0x76 0x43, 0xEA 0xD4 0xAA, 0xE4 0xA6 0x72,
                0xB8 0x6F 0x50, 0x73 0x3E 0x39, 0x3E 0x27 0x31, 0xA2 0x26 0x33,
                0xE4 0x3B 0x44, 0xF7 0x76 0x22, 0xFE 0xAE 0x34, 0xFE 0xE7 0x61,
                0x63 0xC7 0x4D, 0x3E 0x89 0x48, 0x26 0x5C 0x42, 0x19 0x3C 0x3E,
                0x12 0x4E 0x89, 0x00 0x99 0xDB, 0x2C 0xE8 0xF5, 0xFF 0xFF 0xFF,
                0xC0 0xCB 0xDC, 0x8B 0x9B 0xB4, 0x5A 0x69 0x88, 0x3A 0x44 0x66,
                0x26 0x2B 0x44, 0x18 0x14 0x25, 0xFF 0x00 0x44, 0x68 0x38 0x6C,
                0xB5 0x50 0x88, 0xF6 0x75 0x7A, 0xE8 0xB7 0x96, 0xC2 0x85 0x69,
            ),
            hex_palette!(
                "NA16",
                0x8C 0x8F 0xAE, 0x58 0x45 0x63, 0x3E 0x21 0x37, 0x9A 0x63 0x48,
                0xD7 0x9B 0x7D, 0xF5 0xED 0xBA, 0xC0 0xC7 0x41, 0x64 0x7D 0x34,
                0xE4 0x94 0x3A, 0x9D 0x30 0x3B, 0xD2 0x64 0x71, 0x70 0x37 0x7F,
                0x7E 0xC4 0xC1, 0x34 0x85 0x9D, 0x17 0x43 0x4B, 0x1F 0x0E 0x1C,
            ),
            hex_palette!(
                "Basic",
                0x00 0x00 0x00, 0x80 0x80 0x80, 0xFF 0xFF 0xFF, 0xFF 0x00 0x00,
                0x00 0xFF 0x00, 0x00 0x00 0xFF, 0xFF 0xFF 0x00, 0xFF 0x00 0xFF,
                0x00 0xFF 0xFF, 0xFF 0x80 0x00, 0x80 0x00 0xFF, 0x00 0x80 0x40,
            ),
        ])
    }
}

#[derive(Resource, Default)]
pub struct PaletteChoice(pub usize);

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
fn icon_shapes() -> egui::ImageSource<'static> {
    egui::include_image!("../assets/icons/shapes.svg")
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
        Tool::Shape => icon_shapes(),
    }
}

pub fn ui_system(
    mut contexts: EguiContexts,
    mut tool: ResMut<ToolState>,
    mut color: ResMut<CurrentColor>,
    recent: Res<RecentColors>,
    mut grid: ResMut<VoxelGrid>,
    mut history: ResMut<History>,
    mut pending: ResMut<PendingDialog>,
    mut palette_choice: ResMut<PaletteChoice>,
    palettes: Res<Palettes>,
    theme: Res<Theme>,
    mut prefs: ResMut<Preferences>,
    mut prefs_window: ResMut<PreferencesWindow>,
    mut shape_options: ResMut<ShapeOptions>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    cameras: Query<&bevy_panorbit_camera::PanOrbitCamera>,
) -> Result {
    let ctx = contexts.ctx_mut()?;
    egui_extras::install_image_loaders(ctx);
    apply_egui_style(ctx, &theme);

    // Local bindings shadow the previous module-level constants so that the
    // rest of this function can stay as it was.
    #[allow(non_snake_case)]
    let BG = theme.bg;
    #[allow(non_snake_case)]
    let PANEL = theme.panel;
    #[allow(non_snake_case)]
    let ACCENT = theme.accent;
    #[allow(non_snake_case)]
    let TEXT = theme.text;
    #[allow(non_snake_case)]
    let TEXT_DIM = theme.text_dim;
    #[allow(non_snake_case)]
    let BORDER = theme.border;

    // ---------- Top bar ----------
    egui::TopBottomPanel::top("top_bar")
        .frame(
            egui::Frame::default()
                .fill(PANEL)
                .inner_margin(egui::Margin::symmetric(12, 8))
                .stroke(egui::Stroke::new(0.5, BORDER)),
        )
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                if icon_button(ui, &theme, icon_file_plus(), "New")
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
                vertical_rule(ui, &theme);
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
                ui.add_space(12.0);
                let dist = cameras
                    .iter()
                    .next()
                    .map(|cam| cam.target_radius.round() as i32)
                    .unwrap_or(0);
                ui.label(
                    egui::RichText::new(format!("Dist {dist}"))
                        .color(TEXT_DIM)
                        .size(12.0),
                );
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
                tool_button(ui, &theme, &mut tool, Tool::Brush, "Brush", "B");
                ui.add_space(4.0);
                tool_button(ui, &theme, &mut tool, Tool::Erase, "Erase", "E");
                ui.add_space(4.0);
                tool_button(ui, &theme, &mut tool, Tool::Paint, "Paint", "P");
                ui.add_space(4.0);
                tool_button(ui, &theme, &mut tool, Tool::Shape, "Shape", "S");
                ui.add_space(4.0);
                tool_button(ui, &theme, &mut tool, Tool::Eyedropper, "Pick", "I");
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
            egui::ScrollArea::vertical().show(ui, |ui| {
                let inner_frame = egui::Frame::default()
                    .inner_margin(egui::Margin {
                        left: 0,
                        right: 12,
                        top: 0,
                        bottom: 0,
                    });
                inner_frame.show(ui, |ui| {
                // Color section
                section(ui, &theme, "Color", |ui| {
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
                        ui.label(
                            egui::RichText::new(format!(
                                "#{:02X}{:02X}{:02X}",
                                color.0[0], color.0[1], color.0[2]
                            ))
                            .monospace()
                            .size(13.0),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(
                                egui::RichText::new(format!(
                                    "{}, {}, {}",
                                    color.0[0], color.0[1], color.0[2]
                                ))
                                .color(TEXT_DIM)
                                .size(11.0),
                            );
                        });
                    });

                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("Palette").color(TEXT_DIM).size(11.0));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if palette_choice.0 >= palettes.0.len() {
                                palette_choice.0 = 0;
                            }
                            egui::ComboBox::from_id_salt("palette_combo")
                                .selected_text(palettes.0[palette_choice.0].name.as_str())
                                .width(140.0)
                                .show_ui(ui, |ui| {
                                    for (i, p) in palettes.0.iter().enumerate() {
                                        ui.selectable_value(&mut palette_choice.0, i, p.name.as_str());
                                    }
                                });
                        });
                    });
                    ui.add_space(4.0);
                    let dialog_busy = pending.is_active();
                    ui.horizontal(|ui| {
                        if ui
                            .add_enabled(!dialog_busy, egui::Button::new(
                                egui::RichText::new("Import .ase…").size(11.0),
                            ))
                            .on_hover_text("Import an Adobe Swatch Exchange palette")
                            .clicked()
                        {
                            pending.spawn(async move {
                                rfd::AsyncFileDialog::new()
                                    .add_filter("Adobe Swatch Exchange", &["ase"])
                                    .pick_file()
                                    .await
                                    .map(|f| DialogResult::ImportAse(f.path().to_path_buf()))
                            });
                        }
                        let current = &palettes.0[palette_choice.0];
                        let export_name = current.name.clone();
                        let export_colors = current.colors.clone();
                        let default_filename = format!(
                            "{}.ase",
                            sanitize_filename(&current.name)
                        );
                        if ui
                            .add_enabled(!dialog_busy, egui::Button::new(
                                egui::RichText::new("Export .ase…").size(11.0),
                            ))
                            .on_hover_text("Export current palette as Adobe Swatch Exchange")
                            .clicked()
                        {
                            pending.spawn(async move {
                                rfd::AsyncFileDialog::new()
                                    .add_filter("Adobe Swatch Exchange", &["ase"])
                                    .set_file_name(&default_filename)
                                    .save_file()
                                    .await
                                    .map(|f| DialogResult::ExportAse(
                                        f.path().to_path_buf(),
                                        export_name,
                                        export_colors,
                                    ))
                            });
                        }
                    });
                    ui.add_space(4.0);
                    let active_palette = palettes.0[palette_choice.0].colors.clone();
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().button_padding = egui::vec2(0.0, 0.0);
                        ui.spacing_mut().interact_size = egui::vec2(0.0, 0.0);
                        ui.spacing_mut().item_spacing = egui::vec2(4.0, 4.0);
                        for c in &active_palette {
                            let col = egui::Color32::from_rgba_unmultiplied(c[0], c[1], c[2], 255);
                            let is_current = color.0 == *c;
                            let stroke = if is_current {
                                egui::Stroke::new(2.0, ACCENT)
                            } else {
                                egui::Stroke::new(0.5, theme.border)
                            };
                            let resp = ui.add_sized(
                                [20.0, 20.0],
                                egui::Button::new("")
                                    .fill(col)
                                    .stroke(stroke)
                                    .corner_radius(egui::CornerRadius::same(4)),
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

                    ui.add_space(8.0);
                    ui.label(egui::RichText::new("Recent").color(TEXT_DIM).size(11.0));
                    ui.add_space(4.0);
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().button_padding = egui::vec2(0.0, 0.0);
                        ui.spacing_mut().interact_size = egui::vec2(0.0, 0.0);
                        ui.spacing_mut().item_spacing = egui::vec2(4.0, 4.0);
                        for c in &recent.0 {
                            let col = egui::Color32::from_rgba_unmultiplied(c[0], c[1], c[2], 255);
                            let is_current = color.0 == *c;
                            let stroke = if is_current {
                                egui::Stroke::new(2.0, ACCENT)
                            } else {
                                egui::Stroke::new(0.5, theme.border)
                            };
                            let resp = ui.add_sized(
                                [24.0, 24.0],
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
                        if recent.0.is_empty() {
                            ui.label(
                                egui::RichText::new("—")
                                    .color(TEXT_DIM)
                                    .size(12.0),
                            );
                        }
                    });
                });

                // Shape section (only when Shape tool is active)
                if tool.current == Tool::Shape {
                    section(ui, &theme, "Shape", |ui| {
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
                section(ui, &theme, "Scene", |ui| {
                    stat_row(ui, &theme, "Voxels", grid.count().to_string());
                    stat_row(
                        ui,
                        &theme,
                        "Grid",
                        format!(
                            "{g} × {g} × {g}",
                            g = crate::grid::GRID
                        ),
                    );
                    stat_row(ui, &theme, "Undo", history.undo.len().to_string());
                    stat_row(ui, &theme, "Redo", history.redo.len().to_string());
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
        let cursor = if mouse.pressed(MouseButton::Right) {
            egui::CursorIcon::Move
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
                        ui.label(format!("#{:02X}{:02X}{:02X}", rgb[0], rgb[1], rgb[2]));
                    });
                }
                ui.add_space(4.0);

                plane_color_row(ui, theme.mode, "Floor", &mut prefs.floor_color);
                plane_color_row(ui, theme.mode, "Walls", &mut prefs.wall_color);
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

    Ok(())
}

fn plane_color_row(
    ui: &mut egui::Ui,
    mode: crate::theme::ThemeMode,
    label: &str,
    pref: &mut PlaneColorPref,
) {
    let mut is_custom = matches!(pref, PlaneColorPref::Custom(_));
    ui.horizontal(|ui| {
        ui.label(label);
        ui.add_space(8.0);
        if ui.radio(!is_custom, "Match theme").clicked() {
            *pref = PlaneColorPref::MatchTheme;
            is_custom = false;
        }
        if ui.radio(is_custom, "Custom").clicked() {
            let seed = match *pref {
                PlaneColorPref::Custom(rgb) => rgb,
                PlaneColorPref::MatchTheme => plane_match_color(mode),
            };
            *pref = PlaneColorPref::Custom(seed);
        }
    });
    if let PlaneColorPref::Custom(ref mut rgb) = *pref {
        ui.horizontal(|ui| {
            ui.add_space(8.0);
            ui.color_edit_button_srgb(rgb);
            ui.label(format!("#{:02X}{:02X}{:02X}", rgb[0], rgb[1], rgb[2]));
        });
    }
}

fn vertical_rule(ui: &mut egui::Ui, theme: &Theme) {
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(1.0, 20.0), egui::Sense::hover());
    ui.painter()
        .vline(rect.center().x, rect.y_range(), egui::Stroke::new(0.5, theme.border));
}

fn tool_label(t: Tool) -> &'static str {
    match t {
        Tool::Brush => "Brush",
        Tool::Erase => "Erase",
        Tool::Paint => "Paint",
        Tool::Eyedropper => "Pick",
        Tool::Shape => "Shape",
    }
}

fn tool_button(
    ui: &mut egui::Ui,
    theme: &Theme,
    tool: &mut ToolState,
    kind: Tool,
    label: &str,
    shortcut: &str,
) {
    let active = tool.current == kind;
    let (fill, fg) = if active {
        (theme.accent, egui::Color32::WHITE)
    } else {
        (theme.surface, theme.text)
    };
    let icon = egui::Image::new(tool_icon(kind))
        .fit_to_exact_size(egui::vec2(18.0, 18.0))
        .tint(fg);
    let resp = ui
        .scope(|ui| {
            ui.spacing_mut().button_padding = egui::vec2(0.0, 0.0);
            ui.spacing_mut().interact_size = egui::vec2(0.0, 0.0);
            ui.add_sized(
                [40.0, 40.0],
                egui::Button::image(icon)
                    .fill(fill)
                    .stroke(egui::Stroke::NONE)
                    .corner_radius(egui::CornerRadius::same(6)),
            )
        })
        .inner;
    if resp.clicked() && tool.current != kind {
        tool.previous = tool.current;
        tool.current = kind;
    }
    resp.on_hover_text(format!("{label}  ({shortcut})"));
}

fn icon_button(
    ui: &mut egui::Ui,
    theme: &Theme,
    icon: egui::ImageSource<'static>,
    label: &str,
) -> egui::Response {
    ui.add(egui::Button::image_and_text(
        egui::Image::new(icon)
            .fit_to_exact_size(egui::vec2(14.0, 14.0))
            .tint(theme.text),
        egui::RichText::new(label).size(13.0),
    ))
}

fn section<R>(
    ui: &mut egui::Ui,
    theme: &Theme,
    title: &str,
    add: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    ui.label(
        egui::RichText::new(title)
            .color(theme.text)
            .size(13.0)
            .family(egui::FontFamily::Name(NUNITO_700_FAMILY.into())),
    );
    ui.add_space(8.0);
    let r = add(ui);
    ui.add_space(12.0);
    let sep_rect = ui
        .allocate_exact_size(egui::vec2(ui.available_width(), 1.0), egui::Sense::hover())
        .0;
    ui.painter().hline(
        ui.clip_rect().x_range(),
        sep_rect.center().y,
        egui::Stroke::new(0.5, theme.border),
    );
    ui.add_space(12.0);
    r
}

fn sanitize_filename(s: &str) -> String {
    let cleaned: String = s
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect();
    if cleaned.is_empty() { "palette".into() } else { cleaned }
}

fn stat_row(ui: &mut egui::Ui, theme: &Theme, label: &str, value: String) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(label).color(theme.text_dim).size(12.0));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(
                egui::RichText::new(value)
                    .monospace()
                    .color(theme.text)
                    .size(12.0),
            );
        });
    });
}
