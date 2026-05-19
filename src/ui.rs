mod command_palette;
mod dialogs;
mod icons;
mod palette;
pub mod toast;
pub mod tokens;
mod widgets;

pub use command_palette::{
    CommandPalette, command_palette_shortcut_system, dispatch_command_palette_system,
};
pub use dialogs::{
    CurrentProjectPath, DialogResult, PendingDialog, PendingImport, RecentFiles,
    poll_dialogs_system, spawn_save, spawn_save_as,
};
pub use palette::{Palette, PaletteChoice, Palettes};
pub use toast::{Toasts, toast_lifetime_system};

use crate::gizmo::{GizmoDrag, GizmoRect};
use crate::grid::{NewProject, VoxelGrid};
use crate::history::History;
use crate::io;
use crate::shapes::ShapePrimitive;
use crate::theme::{
    CanvasBgPref, Preferences, PreferencesWindow, Theme, ThemePref, apply_egui_style,
    save_preferences,
};
use crate::tools::{CurrentColor, RecentColors, ShapeOptions, Tool, ToolState};
#[cfg(not(target_os = "macos"))]
use crate::ui::tokens::icon;
use crate::ui::tokens::{font, radius, space, stroke, swatch};
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

#[derive(SystemParam)]
pub struct UiInput<'w> {
    pub keys: Res<'w, ButtonInput<KeyCode>>,
    pub mouse: Res<'w, ButtonInput<MouseButton>>,
}

#[derive(SystemParam)]
pub struct UiState<'w> {
    pub new_project: ResMut<'w, NewProject>,
    pub selection: ResMut<'w, crate::select::Selection>,
    pub toasts: Res<'w, Toasts>,
    pub current_path: Res<'w, CurrentProjectPath>,
    pub flyby: Res<'w, crate::camera::FlybyState>,
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
    input: UiInput,
    zoom: ZoomReadout,
    gizmo_view: GizmoView,
    ui_state: UiState,
    mut cmd_palette: ResMut<CommandPalette>,
) -> Result {
    let UiInput { keys, mouse } = input;
    #[cfg_attr(target_os = "macos", allow(unused_variables))]
    let UiState {
        mut new_project,
        selection,
        toasts,
        current_path,
        flyby,
    } = ui_state;
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
    let PANEL = theme.panel;
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
                .stroke(egui::Stroke::new(stroke::HAIR, BORDER)),
        )
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                if widgets::icon_button(ui, &theme, icons::file_plus(), "New")
                    .on_hover_text("Start a new project")
                    .clicked()
                {
                    new_project.dialog_open = true;
                }
                let dialog_busy = pending.is_active();
                if ui
                    .add_enabled(
                        !dialog_busy,
                        egui::Button::image_and_text(
                            egui::Image::new(icons::folder_open())
                                .fit_to_exact_size(icon::md_square())
                                .tint(if dialog_busy { TEXT_DIM } else { TEXT }),
                            egui::RichText::new("Open…").size(font::BODY),
                        ),
                    )
                    .clicked()
                {
                    pending.spawn(async move {
                        rfd::AsyncFileDialog::new()
                            .add_filter("Roxel project", &["rox"])
                            .pick_file()
                            .await
                            .map(|f| DialogResult::OpenProject(f.path().to_path_buf()))
                    });
                }
                if ui
                    .add_enabled(
                        !dialog_busy,
                        egui::Button::image_and_text(
                            egui::Image::new(icons::save())
                                .fit_to_exact_size(icon::md_square())
                                .tint(if dialog_busy { TEXT_DIM } else { TEXT }),
                            egui::RichText::new("Save").size(font::BODY),
                        ),
                    )
                    .clicked()
                {
                    dialogs::spawn_save(&mut pending, &current_path);
                }
                if ui
                    .add_enabled(
                        !dialog_busy,
                        egui::Button::image_and_text(
                            egui::Image::new(icons::save())
                                .fit_to_exact_size(icon::md_square())
                                .tint(if dialog_busy { TEXT_DIM } else { TEXT }),
                            egui::RichText::new("Save As…").size(font::BODY),
                        ),
                    )
                    .clicked()
                {
                    dialogs::spawn_save_as(&mut pending, &current_path);
                }
                ui.menu_image_text_button(
                    egui::Image::new(icons::folder_open())
                        .fit_to_exact_size(icon::md_square())
                        .tint(TEXT),
                    egui::RichText::new("Import").size(font::BODY),
                    |ui| {
                        ui.set_min_width(180.0);
                        if ui
                            .add_enabled(!dialog_busy, egui::Button::new("MagicaVoxel .vox…"))
                            .clicked()
                        {
                            pending.spawn(async move {
                                rfd::AsyncFileDialog::new()
                                    .add_filter("MagicaVoxel", &["vox"])
                                    .pick_file()
                                    .await
                                    .map(|f| DialogResult::ImportVox(f.path().to_path_buf()))
                            });
                            ui.close();
                        }
                        if ui
                            .add_enabled(!dialog_busy, egui::Button::new("Qubicle .qb…"))
                            .clicked()
                        {
                            pending.spawn(async move {
                                rfd::AsyncFileDialog::new()
                                    .add_filter("Qubicle", &["qb"])
                                    .pick_file()
                                    .await
                                    .map(|f| DialogResult::ImportQb(f.path().to_path_buf()))
                            });
                            ui.close();
                        }
                        if ui
                            .add_enabled(!dialog_busy, egui::Button::new("Goxel .gox…"))
                            .clicked()
                        {
                            pending.spawn(async move {
                                rfd::AsyncFileDialog::new()
                                    .add_filter("Goxel", &["gox"])
                                    .pick_file()
                                    .await
                                    .map(|f| DialogResult::ImportGox(f.path().to_path_buf()))
                            });
                            ui.close();
                        }
                    },
                );
                ui.menu_image_text_button(
                    egui::Image::new(icons::download())
                        .fit_to_exact_size(icon::md_square())
                        .tint(TEXT),
                    egui::RichText::new("Export").size(font::BODY),
                    |ui| {
                        ui.set_min_width(180.0);
                        if ui
                            .add_enabled(!dialog_busy, egui::Button::new("MagicaVoxel .vox…"))
                            .clicked()
                        {
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
                        if ui
                            .add_enabled(!dialog_busy, egui::Button::new("Wavefront .obj…"))
                            .clicked()
                        {
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
                        if ui
                            .add_enabled(!dialog_busy, egui::Button::new("Autodesk .fbx…"))
                            .clicked()
                        {
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
                        if ui
                            .add_enabled(!dialog_busy, egui::Button::new("glTF .glb…"))
                            .clicked()
                        {
                            pending.spawn(async move {
                                rfd::AsyncFileDialog::new()
                                    .add_filter("glTF binary", &["glb"])
                                    .set_file_name("model.glb")
                                    .save_file()
                                    .await
                                    .map(|f| DialogResult::ExportGltf(f.path().to_path_buf()))
                            });
                            ui.close();
                        }
                        if ui
                            .add_enabled(!dialog_busy, egui::Button::new("Goxel .gox…"))
                            .clicked()
                        {
                            pending.spawn(async move {
                                rfd::AsyncFileDialog::new()
                                    .add_filter("Goxel", &["gox"])
                                    .set_file_name("model.gox")
                                    .save_file()
                                    .await
                                    .map(|f| DialogResult::ExportGox(f.path().to_path_buf()))
                            });
                            ui.close();
                        }
                        if ui
                            .add_enabled(!dialog_busy, egui::Button::new("Transparent PNG…"))
                            .clicked()
                        {
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
                        if ui
                            .add_enabled(!dialog_busy, egui::Button::new("SVG…"))
                            .clicked()
                        {
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
                    },
                );

                ui.add_space(8.0);
                widgets::vertical_rule(ui, &theme);
                ui.add_space(4.0);

                let undo_enabled = !history.undo.is_empty();
                if ui
                    .add_enabled(
                        undo_enabled,
                        egui::Button::image_and_text(
                            egui::Image::new(icons::undo())
                                .fit_to_exact_size(icon::md_square())
                                .tint(if undo_enabled { TEXT } else { TEXT_DIM }),
                            egui::RichText::new("Undo").size(font::BODY),
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
                                .fit_to_exact_size(icon::md_square())
                                .tint(if redo_enabled { TEXT } else { TEXT_DIM }),
                            egui::RichText::new("Redo").size(font::BODY),
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
                            egui::RichText::new("Preferences…").size(font::BODY),
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
                .fill(PANEL)
                .inner_margin(egui::Margin::symmetric(12, 4))
                .stroke(egui::Stroke::new(stroke::HAIR, BORDER)),
        )
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                let stats_reserve = 280.0;
                let hint_w = (ui.available_width() - stats_reserve).max(0.0);
                ui.allocate_ui_with_layout(
                    egui::vec2(hint_w, ui.available_height()),
                    egui::Layout::left_to_right(egui::Align::Center),
                    |ui| {
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(widgets::tool_hint(tool.current))
                                    .color(theme.text_dim)
                                    .size(font::SMALL),
                            )
                            .selectable(false)
                            .truncate(),
                        );
                    },
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let design_label = match grid.bounding_box() {
                        Some((min, max)) => {
                            let extent = max - min + bevy::math::IVec3::ONE;
                            format!("Size {}×{}×{}", extent.x, extent.y, extent.z)
                        }
                        None => "Size —".to_string(),
                    };
                    widgets::status_label(ui, &theme, &design_label);
                    ui.add_space(12.0);
                    widgets::status_label(
                        ui,
                        &theme,
                        &format!(
                            "{n} voxel{s}",
                            n = grid.count(),
                            s = if grid.count() == 1 { "" } else { "s" }
                        ),
                    );
                    if let Some(cam) = zoom.cameras.iter().next() {
                        let actual = cam.radius.unwrap_or(cam.target_radius);
                        let r = actual.round().max(0.0) as i32;
                        ui.add_space(12.0);
                        widgets::status_label(
                            ui,
                            &theme,
                            &format!("Zoom {r} voxel{}", if r == 1 { "" } else { "s" }),
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
                widgets::tool_button(
                    ui,
                    &theme,
                    &mut tool,
                    Tool::Brush,
                    icons::tool(Tool::Brush),
                    "Brush",
                    "B",
                );
                ui.add_space(2.0);
                widgets::tool_button(
                    ui,
                    &theme,
                    &mut tool,
                    Tool::Erase,
                    icons::tool(Tool::Erase),
                    "Erase",
                    "E",
                );
                ui.add_space(2.0);
                widgets::tool_button(
                    ui,
                    &theme,
                    &mut tool,
                    Tool::Paint,
                    icons::tool(Tool::Paint),
                    "Paint",
                    "P",
                );
                ui.add_space(2.0);
                widgets::tool_button(
                    ui,
                    &theme,
                    &mut tool,
                    Tool::Eyedropper,
                    icons::tool(Tool::Eyedropper),
                    "Pick",
                    "I",
                );
                ui.add_space(2.0);
                let shape_resp = widgets::tool_button(
                    ui,
                    &theme,
                    &mut tool,
                    Tool::Shape,
                    icons::shape_primitive(shape_options.primitive),
                    "Shape",
                    "S",
                );
                egui::Popup::menu(&shape_resp)
                    .align(egui::RectAlign::RIGHT_START)
                    .gap(6.0)
                    .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
                    .show(|ui| {
                        ui.spacing_mut().item_spacing = egui::vec2(4.0, 4.0);
                        ui.spacing_mut().button_padding = egui::vec2(6.0, 6.0);
                        ui.horizontal(|ui| {
                            for (prim, label) in [
                                (ShapePrimitive::Rectangle, "Rectangle"),
                                (ShapePrimitive::Ellipse, "Ellipse"),
                                (ShapePrimitive::Line, "Line"),
                            ] {
                                let selected = shape_options.primitive == prim;
                                let img = egui::Image::new(icons::shape_primitive(prim))
                                    .fit_to_exact_size(crate::ui::tokens::icon::md_square())
                                    .tint(if selected {
                                        egui::Color32::WHITE
                                    } else {
                                        theme.text
                                    });
                                let btn = egui::Button::image(img)
                                    .fill(if selected {
                                        theme.accent
                                    } else {
                                        theme.surface
                                    })
                                    .stroke(if selected {
                                        egui::Stroke::NONE
                                    } else {
                                        egui::Stroke::new(stroke::HAIR, theme.border)
                                    })
                                    .corner_radius(egui::CornerRadius::same(radius::SM));
                                if ui.add(btn).on_hover_text(label).clicked() {
                                    shape_options.primitive = prim;
                                    ui.close();
                                }
                            }
                        });
                    });
                ui.add_space(2.0);
                widgets::tool_button(
                    ui,
                    &theme,
                    &mut tool,
                    Tool::Select,
                    icons::tool(Tool::Select),
                    "Marquee select",
                    "M",
                );
                ui.add_space(2.0);
                widgets::tool_button(
                    ui,
                    &theme,
                    &mut tool,
                    Tool::Move,
                    icons::tool(Tool::Move),
                    "Move",
                    "V",
                );
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
        egui::Stroke::new(stroke::HAIR, BORDER),
    );

    // ---------- Right inspector ----------
    let right_resp = egui::SidePanel::right("right_panel")
        .resizable(true)
        .default_width(244.0)
        .min_width(244.0)
        .max_width(468.0)
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
            egui::ScrollArea::vertical()
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    let inner_frame = egui::Frame::default().inner_margin(egui::Margin {
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
                            let swatch_resp = widgets::swatch_button(
                                ui,
                                &theme,
                                srgba,
                                egui::vec2(swatch_w, swatch::HERO_HEIGHT),
                                radius::MD,
                                false,
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
                            ui.add_space(space::XS);
                            ui.horizontal(|ui| {
                                widgets::hex_label(
                                    ui,
                                    &theme,
                                    [color.0[0], color.0[1], color.0[2]],
                                    false,
                                );
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        ui.add(
                                            egui::Label::new(
                                                egui::RichText::new(format!(
                                                    "{}, {}, {}",
                                                    color.0[0], color.0[1], color.0[2]
                                                ))
                                                .color(TEXT_DIM)
                                                .size(font::SMALL),
                                            )
                                            .selectable(true),
                                        );
                                    },
                                );
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
                            egui::ComboBox::from_id_salt("palette_combo")
                                .selected_text(palettes.0[active_idx].name.clone())
                                .width(row_w)
                                .show_ui(ui, |ui| {
                                    for (i, p) in palettes.0.iter().enumerate() {
                                        ui.selectable_value(
                                            &mut palette_choice.0,
                                            i,
                                            p.name.clone(),
                                        );
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
                                    let commit =
                                        widgets::icon_only_button(ui, &theme, icons::check(), true)
                                            .on_hover_text("Save name")
                                            .clicked()
                                            || (resp.lost_focus()
                                                && ui.input(|i| i.key_pressed(egui::Key::Enter)));
                                    let cancel =
                                        widgets::icon_only_button(ui, &theme, icons::x(), true)
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
                                    palette_rename.buf = palettes.0[palette_choice.0].name.clone();
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
                                if widgets::icon_only_button(
                                    ui,
                                    &theme,
                                    icons::pencil(),
                                    !active_is_builtin,
                                )
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
                                let has_user = palettes.0.iter().filter(|p| !p.builtin).count() > 0;
                                let del_enabled = !active_is_builtin && has_user;
                                if widgets::icon_only_button(
                                    ui,
                                    &theme,
                                    icons::trash(),
                                    del_enabled,
                                )
                                .on_hover_text(if active_is_builtin {
                                    "Built-in palettes can't be deleted"
                                } else {
                                    "Delete palette"
                                })
                                .clicked()
                                {
                                    palettes.0.remove(active_idx);
                                    if palette_choice.0 >= palettes.0.len() {
                                        palette_choice.0 = palettes.0.len().saturating_sub(1);
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
                                                    .add_filter("Adobe Swatch Exchange", &["ase"])
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
                                                    .add_filter("Adobe Swatch Exchange", &["ase"])
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
                            active_idx = palette_choice.0.min(palettes.0.len().saturating_sub(1));
                            palette_choice.0 = active_idx;
                            active_is_builtin = palettes.0[active_idx].builtin;

                            if !active_is_builtin {
                                ui.add_space(6.0);
                                let add_enabled = !palettes.0[active_idx].colors.contains(&color.0);
                                if widgets::wide_action_button(
                                    ui,
                                    &theme,
                                    icons::plus(),
                                    "Add current color",
                                    row_w,
                                    add_enabled,
                                )
                                .on_hover_text(if !add_enabled {
                                    "Color already in palette"
                                } else {
                                    "Add current color as a swatch"
                                })
                                .clicked()
                                {
                                    palettes.0[active_idx].colors.push(color.0);
                                    io::palettes::save(&palettes.0);
                                }
                            }

                            ui.add_space(8.0);
                            let mut reorder: Option<(usize, usize)> = None;
                            let mut remove_idx: Option<usize> = None;
                            let active_palette = palettes.0[active_idx].colors.clone();
                            let editable = !active_is_builtin;
                            if active_palette.is_empty() {
                                widgets::hint_label(
                                    ui,
                                    &theme,
                                    if editable {
                                        "No swatches yet — add the current color above"
                                    } else {
                                        "Built-in palette — duplicate to add swatches"
                                    },
                                );
                            } else {
                                widgets::swatch_grid(ui, |ui| {
                                    for (si, c) in active_palette.iter().enumerate() {
                                        let col = egui::Color32::from_rgba_unmultiplied(
                                            c[0], c[1], c[2], 255,
                                        );
                                        let is_current = color.0 == *c;
                                        let swatch_id = egui::Id::new(("swatch", active_idx, si));
                                        let mut clicked = false;
                                        let resp = if editable {
                                            ui.dnd_drag_source(swatch_id, si, |ui| {
                                                let r = widgets::swatch_button(
                                                    ui,
                                                    &theme,
                                                    col,
                                                    swatch::PALETTE,
                                                    radius::XS,
                                                    is_current,
                                                );
                                                clicked = r.clicked();
                                                r
                                            })
                                            .response
                                        } else {
                                            let r = widgets::swatch_button(
                                                ui,
                                                &theme,
                                                col,
                                                swatch::PALETTE,
                                                radius::XS,
                                                is_current,
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
                                            "{}{}",
                                            widgets::hex_string([c[0], c[1], c[2]]),
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
                            if active_is_builtin && !active_palette.is_empty() {
                                ui.add_space(6.0);
                                widgets::hint_label(
                                    ui,
                                    &theme,
                                    "Built-in palette — duplicate to add swatches",
                                );
                            }
                        });

                        // Recent section
                        widgets::section(ui, &theme, "Recent", |ui| {
                            if recent.0.is_empty() {
                                widgets::hint_label(ui, &theme, "No recent colors");
                            } else {
                                widgets::swatch_grid(ui, |ui| {
                                    for c in &recent.0 {
                                        let col = egui::Color32::from_rgba_unmultiplied(
                                            c[0], c[1], c[2], 255,
                                        );
                                        let is_current = color.0 == *c;
                                        let resp = widgets::swatch_button(
                                            ui,
                                            &theme,
                                            col,
                                            swatch::RECENT,
                                            radius::XS,
                                            is_current,
                                        );
                                        if resp.clicked() {
                                            color.0 = *c;
                                        }
                                        resp.on_hover_text(widgets::hex_string([c[0], c[1], c[2]]));
                                    }
                                });
                            }
                        });

                        // Selection section — only when there's an active region.
                        if let Some(aabb) = selection.aabb {
                            widgets::section(ui, &theme, "Selection", |ui| {
                                let extents = aabb.extents();
                                widgets::stat_row(
                                    ui,
                                    &theme,
                                    "Bounds",
                                    format!("{} × {} × {}", extents.x, extents.y, extents.z),
                                );
                                widgets::stat_row(
                                    ui,
                                    &theme,
                                    "Voxels",
                                    aabb.voxel_count(&grid).to_string(),
                                );
                            });
                        }
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
        egui::Stroke::new(stroke::HAIR, BORDER),
    );

    // Reflect tool in cursor when pointer is over the viewport.
    if !ctx.is_pointer_over_area() {
        let alt = keys.any_pressed([KeyCode::AltLeft, KeyCode::AltRight]);
        let z = keys.pressed(KeyCode::KeyZ);
        let over_gizmo = gizmo_view.drag.active
            || gizmo_view
                .rect
                .0
                .zip(ctx.pointer_latest_pos())
                .is_some_and(|(r, p)| {
                    p.x >= r.min.x && p.x <= r.max.x && p.y >= r.min.y && p.y <= r.max.y
                });
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
        widgets::modal_window(ctx, &theme, "Preferences", &mut open_flag).show(ctx, |ui| {
            ui.set_min_width(340.0);
            widgets::section(ui, &theme, "Appearance", |ui| {
                widgets::prefs_row(ui, &theme, "Theme", |ui| {
                    widgets::chip_button(ui, &theme, &mut prefs.theme, ThemePref::System, "System");
                    widgets::chip_button(ui, &theme, &mut prefs.theme, ThemePref::Light, "Light");
                    widgets::chip_button(ui, &theme, &mut prefs.theme, ThemePref::Dark, "Dark");
                });
            });

            widgets::section(ui, &theme, "Canvas", |ui| {
                let mut is_custom = matches!(prefs.canvas_bg, CanvasBgPref::Custom(_));
                widgets::prefs_row(ui, &theme, "Background", |ui| {
                    if ui.radio(!is_custom, "Match theme").clicked() {
                        prefs.canvas_bg = CanvasBgPref::MatchTheme;
                        is_custom = false;
                    }
                    if ui.radio(is_custom, "Custom").clicked() {
                        let seed = match prefs.canvas_bg {
                            CanvasBgPref::Custom(rgb) => rgb,
                            CanvasBgPref::MatchTheme => [theme.bg.r(), theme.bg.g(), theme.bg.b()],
                        };
                        prefs.canvas_bg = CanvasBgPref::Custom(seed);
                    }
                });
                if let CanvasBgPref::Custom(ref mut rgb) = prefs.canvas_bg {
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        ui.add_space(76.0);
                        ui.color_edit_button_srgb(rgb);
                        widgets::hex_label(ui, &theme, *rgb, true);
                    });
                }
            });

            widgets::section(ui, &theme, "Visibility", |ui| {
                ui.checkbox(&mut prefs.show_floor_grid, "Show floor grid");
                ui.add_space(2.0);
                ui.checkbox(&mut prefs.show_origin_axes, "Show origin axes");
                ui.add_space(2.0);
                ui.checkbox(&mut prefs.show_y_axis, "Show Y axis line");
            });
        });
        if !open_flag {
            prefs_window.open = false;
        }
        if *prefs != before {
            save_preferences(&prefs);
        }
    }

    // New-project confirm modal. Open-world has no grid size to pick — this
    // is just "do you want to throw away unsaved work?".
    if new_project.dialog_open {
        let mut open = true;
        let mut create_clicked = false;
        let mut cancel_clicked = false;
        widgets::modal_window(ctx, &theme, "New project", &mut open).show(ctx, |ui| {
            ui.set_min_width(260.0);
            widgets::hint_label(ui, &theme, "Start over? This discards any unsaved work.");
            ui.add_space(8.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                if widgets::dialog_button(ui, &theme, "Create", true).clicked() {
                    create_clicked = true;
                }
                if widgets::dialog_button(ui, &theme, "Cancel", false).clicked() {
                    cancel_clicked = true;
                }
            });
        });
        if create_clicked {
            new_project.apply = true;
            new_project.dialog_open = false;
        } else if cancel_clicked || !open {
            new_project.dialog_open = false;
        }
    }

    if cmd_palette.open {
        let state = command_palette::CatalogState {
            tool: tool.current,
            shape: &shape_options,
            has_undo: !history.undo.is_empty(),
            has_redo: !history.redo.is_empty(),
            has_selection: selection.aabb.is_some(),
            dialog_busy: pending.is_active(),
            palettes: &palettes.0,
            palette_choice: palette_choice.0,
            current_color: color.0,
            prefs: &prefs,
            flyby_active: flyby.active,
        };
        let catalog = command_palette::build_catalog(&state);
        command_palette::draw(ctx, &theme, &mut cmd_palette, &catalog);
    }

    toast::draw_toasts(ctx, &theme, &toasts);

    Ok(())
}
