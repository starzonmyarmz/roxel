mod command_palette;
mod dialogs;
mod floating;
pub(crate) mod icons;
mod palette;
mod palette_switcher;
pub mod toast;
pub mod tokens;
mod visibility;
mod widgets;

pub use command_palette::{
    CommandPalette, command_palette_shortcut_system, dispatch_command_palette_system,
};
pub use dialogs::{
    CurrentProjectPath, DialogResult, PendingDialog, PendingImport, RecentFiles,
    poll_dialogs_system, spawn_save, spawn_save_as,
};
pub use palette::{
    DiscardConfirm, Palette, PaletteChoice, PaletteSwitcher, Palettes, WorkingPalette,
};
pub use toast::{Toasts, toast_lifetime_system};
pub use visibility::{UiVisible, tab_toggle_system};

use crate::color_space::ColorSpace;
use crate::gizmo::{GizmoDrag, GizmoRect};
use crate::grid::{NewProject, VoxelGrid};
use crate::history::History;
use crate::io;
use crate::onboarding::{Onboarding, OnboardingAnchors};
use crate::shapes::ShapePrimitive;
use crate::theme::{
    CanvasBgPref, Preferences, PreferencesWindow, Theme, ThemePref, apply_egui_style,
    canvas_match_color, save_preferences,
};
use crate::tools::{
    CurrentColor, ExtraColors, RecentColors, ShapeOptions, Tool, ToolState, apply_swatch_click,
};
#[cfg(not(target_os = "macos"))]
use crate::ui::tokens::icon;
use crate::ui::tokens::{font, gap, height, radius, space, stroke, swatch, width};
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
pub struct ColorParams<'w> {
    pub color: ResMut<'w, CurrentColor>,
    pub extras: ResMut<'w, ExtraColors>,
    pub recent: Res<'w, RecentColors>,
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
    pub color_edit: ResMut<'w, crate::color_space::ColorEditBuffer>,
    pub updater: ResMut<'w, crate::updater::UpdateCheck>,
    pub clipboard: Res<'w, crate::clipboard::Clipboard>,
    pub onboarding: ResMut<'w, Onboarding>,
    pub onboarding_anchors: ResMut<'w, OnboardingAnchors>,
    pub ui_visible: Res<'w, UiVisible>,
}

pub fn ui_system(
    mut contexts: EguiContexts,
    mut tool: ResMut<ToolState>,
    colors: ColorParams,
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
    let ColorParams {
        mut color,
        mut extras,
        recent,
    } = colors;
    let UiInput { keys, mouse } = input;
    #[cfg_attr(target_os = "macos", allow(unused_variables, unused_mut))]
    let UiState {
        mut new_project,
        selection,
        toasts,
        current_path,
        flyby,
        mut color_edit,
        mut updater,
        clipboard,
        #[cfg_attr(target_os = "macos", allow(unused_variables))]
        mut onboarding,
        mut onboarding_anchors,
        ui_visible,
    } = ui_state;
    let ctx = contexts.ctx_mut()?;
    egui_extras::install_image_loaders(ctx);
    apply_egui_style(ctx, &theme);

    let PaletteParams {
        mut palettes,
        choice: mut palette_choice,
        rename: mut palette_rename,
        mut working,
        mut discard,
        mut switcher,
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
    #[allow(non_snake_case, unused_variables)]
    let TEXT_DIM = theme.text_dim;
    #[allow(non_snake_case)]
    let BORDER = theme.border;

    // ---------- Floating menu pill ----------
    // On macOS the native menu bar (see `menu.rs`) replaces these controls; on
    // Win/Linux the pill sits at top-center and is gated by the user pref.
    #[cfg(not(target_os = "macos"))]
    if ui_visible.0 && prefs.show_floating_menu_bar {
        floating::pill_menu(ctx, &theme, |ui| {
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
                let save_resp = ui.add_enabled(
                    !dialog_busy,
                    egui::Button::image_and_text(
                        egui::Image::new(icons::save())
                            .fit_to_exact_size(icon::md_square())
                            .tint(if dialog_busy { TEXT_DIM } else { TEXT }),
                        egui::RichText::new("Save").size(font::BODY),
                    ),
                );
                if save_resp.clicked() {
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
                        ui.set_min_width(width::TOP_BAR_MENU);
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
                        ui.set_min_width(width::TOP_BAR_MENU);
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

                ui.add_space(space::SM);
                widgets::vertical_rule(ui, &theme);
                ui.add_space(space::XS);

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
                        .add(
                            egui::Button::new(egui::RichText::new("?").size(font::BODY))
                                .min_size(egui::vec2(24.0, 24.0)),
                        )
                        .on_hover_text("Show onboarding tour")
                        .clicked()
                    {
                        onboarding.start();
                    }
                    if ui
                        .add(egui::Button::new(
                            egui::RichText::new("Preferences…").size(font::BODY),
                        ))
                        .on_hover_text("Appearance and other settings")
                        .clicked()
                    {
                        prefs_window.open = !prefs_window.open;
                    }
                    if let Some(rel) = updater.available() {
                        let url = rel.html_url.clone();
                        let label = format!("Update {} available", rel.tag);
                        let resp = ui.add(egui::Button::image_and_text(
                            egui::Image::new(icons::arrow_up())
                                .fit_to_exact_size(icon::md_square())
                                .tint(theme.accent),
                            egui::RichText::new(label)
                                .size(font::BODY)
                                .color(theme.accent),
                        ));
                        if resp.on_hover_text("Open the release page").clicked() {
                            crate::updater::open_url(&url);
                        }
                    } else {
                        let busy = updater.is_checking();
                        if ui
                            .add_enabled(
                                !busy,
                                egui::Button::new(
                                    egui::RichText::new("Check for Updates…").size(font::BODY),
                                ),
                            )
                            .on_hover_text("Look for a newer Roxel release on GitHub")
                            .clicked()
                        {
                            crate::updater::start_check(&mut updater, true);
                        }
                    }
                });
            });
        });
    }

    // ---------- Floating tool island ----------
    if ui_visible.0 {
        let island_resp = floating::tool_island(ctx, &theme, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            widgets::tool_button(
                ui,
                &theme,
                &mut tool,
                Tool::Brush,
                icons::tool(Tool::Brush),
                "Brush",
                "B",
            );
            ui.add_space(space::SX);
            widgets::tool_button(
                ui,
                &theme,
                &mut tool,
                Tool::Erase,
                icons::tool(Tool::Erase),
                "Erase",
                "E",
            );
            ui.add_space(space::SX);
            widgets::tool_button(
                ui,
                &theme,
                &mut tool,
                Tool::Paint,
                icons::tool(Tool::Paint),
                "Paint",
                "P",
            );
            ui.add_space(space::SX);
            widgets::tool_button(
                ui,
                &theme,
                &mut tool,
                Tool::Eyedropper,
                icons::tool(Tool::Eyedropper),
                "Pick",
                "I",
            );
            ui.add_space(space::SX);
            let shape_resp = widgets::tool_button(
                ui,
                &theme,
                &mut tool,
                Tool::Shape,
                icons::shape_primitive(shape_options.primitive),
                "Shape",
                "S",
            );
            // Click toggles the picker. Clicking the same rail button again
            // closes it; clicking outside closes via the released-off check
            // below. State lives in egui memory.
            let mem_id = shape_resp.id.with("picker_open");
            let mut popup_open = ui
                .ctx()
                .memory(|m| m.data.get_temp::<bool>(mem_id))
                .unwrap_or(false);
            if shape_resp.clicked() {
                popup_open = !popup_open;
            }
            if popup_open {
                // Use bare `Area` (not `Popup`) so we can disable the default
                // fade-in. Buttons are painted manually so the hover fill
                // tracks `contains_pointer()` even while LMB is held — egui's
                // standard hover styling only fires when the button itself
                // was the press target.
                let area_id = shape_resp.id.with("shape_picker_area");
                let anchor = shape_resp.rect.left_center() - egui::vec2(space::SM, 0.0);
                // Tool-rail neutral hover blend so picker options match the
                // hover style of main tool buttons (bg ⊕ surface_hover ratio).
                let blend_u8 = |a: u8, b: u8| (((a as u16) * 3 + b as u16) / 4) as u8;
                let neutral_hover = egui::Color32::from_rgb(
                    blend_u8(theme.bg.r(), theme.surface_hover.r()),
                    blend_u8(theme.bg.g(), theme.surface_hover.g()),
                    blend_u8(theme.bg.b(), theme.surface_hover.b()),
                );
                egui::Area::new(area_id)
                    .order(egui::Order::Foreground)
                    .fade_in(false)
                    .fixed_pos(anchor)
                    .pivot(egui::Align2::RIGHT_CENTER)
                    .show(ui.ctx(), |ui| {
                        egui::Frame::popup(ui.style()).show(ui, |ui| {
                            ui.spacing_mut().item_spacing = gap::TIGHT;
                            ui.horizontal(|ui| {
                                let cell = swatch::TOOL;
                                for (prim, label) in [
                                    (ShapePrimitive::Rectangle, "Rectangle"),
                                    (ShapePrimitive::Ellipse, "Ellipse"),
                                    (ShapePrimitive::Line, "Line"),
                                ] {
                                    let selected = shape_options.primitive == prim;
                                    let (rect, r) =
                                        ui.allocate_exact_size(cell, egui::Sense::click());
                                    let over = r.contains_pointer();
                                    let fill = if selected {
                                        theme.accent
                                    } else if over {
                                        neutral_hover
                                    } else {
                                        egui::Color32::TRANSPARENT
                                    };
                                    ui.painter().rect_filled(
                                        rect,
                                        egui::CornerRadius::same(radius::SM),
                                        fill,
                                    );
                                    let icon_size = crate::ui::tokens::icon::md_square();
                                    let icon_rect =
                                        egui::Rect::from_center_size(rect.center(), icon_size);
                                    let tint = if selected {
                                        egui::Color32::WHITE
                                    } else {
                                        theme.text
                                    };
                                    egui::Image::new(icons::shape_primitive(prim))
                                        .fit_to_exact_size(icon_size)
                                        .tint(tint)
                                        .paint_at(ui, icon_rect);
                                    let r = r.on_hover_text(label);
                                    if r.clicked() {
                                        shape_options.primitive = prim;
                                        if tool.current != Tool::Shape {
                                            tool.previous = tool.current;
                                            tool.current = Tool::Shape;
                                        }
                                        popup_open = false;
                                    }
                                }
                            });
                        });
                    });
            }
            // Close picker when the pointer presses anywhere outside the
            // shape rail button and the picker area itself.
            if popup_open && ui.input(|i| i.pointer.any_pressed()) {
                let pos = ui.input(|i| i.pointer.interact_pos());
                let over_button = pos.map(|p| shape_resp.rect.contains(p)).unwrap_or(false);
                let picker_id = shape_resp.id.with("shape_picker_area");
                let over_picker = pos
                    .and_then(|p| {
                        ui.ctx()
                            .memory(|m| m.area_rect(picker_id).map(|r| r.contains(p)))
                    })
                    .unwrap_or(false);
                if !over_button && !over_picker {
                    popup_open = false;
                }
            }
            ui.ctx()
                .memory_mut(|m| m.data.insert_temp(mem_id, popup_open));
            ui.add_space(space::SX);
            widgets::tool_button(
                ui,
                &theme,
                &mut tool,
                Tool::Select,
                icons::tool(Tool::Select),
                "Marquee select",
                "M",
            );
            ui.add_space(space::SX);
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
        onboarding_anchors.tool_rail = Some(island_resp.rect);
    }

    // ---------- Left inspector panel ----------
    let mac_gutter: i8 = if cfg!(target_os = "macos") {
        height::MAC_TITLEBAR_GUTTER as i8
    } else {
        0
    };
    let inspector_top: i8 = 12 + mac_gutter;
    let inspector_resp = if ui_visible.0 {
        Some(
            egui::SidePanel::left("inspector_panel")
                .resizable(true)
                // egui's separator draws at `noninteractive.bg_stroke` (1.0px) and
                // stacks on the custom HAIR vline below — reads thick. Suppress it;
                // the hairline is the edge, the resize grab region still works.
                .show_separator_line(false)
                .default_width(width::SIDE_PANEL)
                .min_width(width::SIDE_PANEL)
                .max_width(width::SIDE_PANEL_MAX)
                .frame(
                    egui::Frame::default()
                        .fill(PANEL)
                        .inner_margin(egui::Margin {
                            left: 12,
                            right: 0,
                            top: inspector_top,
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
                                // Status section (design size, voxel count, zoom)
                                widgets::section(ui, &theme, "Status", |ui| {
                                    ui.spacing_mut().item_spacing.y = space::XXS;
                                    let design_label = match grid.bounding_box() {
                                        Some((min, max)) => {
                                            let extent = max - min + bevy::math::IVec3::ONE;
                                            format!("{}×{}×{}", extent.x, extent.y, extent.z)
                                        }
                                        None => "—".to_string(),
                                    };
                                    widgets::stat_row(ui, &theme, "Size", design_label);
                                    widgets::stat_row(
                                        ui,
                                        &theme,
                                        "Voxels",
                                        grid.count().to_string(),
                                    );
                                    if let Some(cam) = zoom.cameras.iter().next() {
                                        let actual = cam.radius.unwrap_or(cam.target_radius);
                                        let r = actual.round().max(0.0) as i32;
                                        widgets::stat_row(
                                            ui,
                                            &theme,
                                            "Zoom",
                                            format!("{r} voxel{}", if r == 1 { "" } else { "s" }),
                                        );
                                    }
                                });
                                // Color section — hero swatch opens the full
                                // picker popup (the only place to edit numerically;
                                // the color-space format lives in Preferences). Hex
                                // readout and recent colors fold in below so this is
                                // one compact block instead of three sections.
                                widgets::section(ui, &theme, "Color", |ui| {
                                    let srgba = egui::Color32::from_rgba_unmultiplied(
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
                                    let active_space = prefs.color_space;
                                    egui::Popup::menu(&swatch_resp)
                                        .close_behavior(
                                            egui::PopupCloseBehavior::CloseOnClickOutside,
                                        )
                                        .show(|ui| {
                                            space_color_picker(
                                                ui,
                                                &mut color,
                                                active_space,
                                                &mut color_edit,
                                            );
                                        });

                                    ui.add_space(space::XS);
                                    // Hex readout for the active color.
                                    ui.vertical_centered(|ui| {
                                        ui.label(
                                            egui::RichText::new(widgets::hex_string([
                                                color.0[0], color.0[1], color.0[2],
                                            ]))
                                            .monospace()
                                            .size(font::SMALL)
                                            .color(theme.text_dim),
                                        );
                                    });

                                    // Recent colors — unlabeled strip under the swatch.
                                    if !recent.0.is_empty() {
                                        ui.add_space(space::XS);
                                        widgets::swatch_grid(ui, |ui| {
                                            for c in &recent.0 {
                                                let col = egui::Color32::from_rgba_unmultiplied(
                                                    c[0], c[1], c[2], 255,
                                                );
                                                let select_state = if color.0 == *c {
                                                    widgets::SwatchSelect::Primary
                                                } else if extras.contains(*c) {
                                                    widgets::SwatchSelect::Extra
                                                } else {
                                                    widgets::SwatchSelect::None
                                                };
                                                let resp = widgets::swatch_button(
                                                    ui,
                                                    &theme,
                                                    col,
                                                    swatch::RECENT,
                                                    radius::XS,
                                                    select_state,
                                                );
                                                if resp.clicked() {
                                                    let shift = ui.input(|i| i.modifiers.shift);
                                                    color.0 = apply_swatch_click(
                                                        shift,
                                                        *c,
                                                        color.0,
                                                        &mut extras,
                                                    );
                                                }
                                                resp.on_hover_text(widgets::hex_string([
                                                    c[0], c[1], c[2],
                                                ]));
                                            }
                                        });
                                    }
                                });

                                if palette_choice.0 >= palettes.0.len() {
                                    palette_choice.0 = 0;
                                }

                                // Palette section — overflow … menu sits on the title row.
                                {
                                    let active_idx =
                                        palette_choice.0.min(palettes.0.len().saturating_sub(1));
                                    palette_choice.0 = active_idx;
                                    let active_is_builtin = palettes.0[active_idx].builtin;

                                    // Buffer so the rename field doesn't overflow and grow the panel.
                                    let row_w = (ui.available_width() - 2.0).max(80.0);

                                    widgets::section_header_action(ui, &theme, "Palette", |ui| {
                                            let menu_resp = widgets::icon_only_button(
                                                ui,
                                                &theme,
                                                icons::ellipsis(),
                                                true,
                                            )
                                            .on_hover_text("Palette actions");
                                            egui::Popup::menu(&menu_resp)
                                                .close_behavior(
                                                    egui::PopupCloseBehavior::CloseOnClick,
                                                )
                                                .show(|ui| {
                                                    // Roomier menu items — wider horizontal
                                                    // padding so labels breathe off both edges.
                                                    ui.set_min_width(width::TOP_BAR_MENU);
                                                    ui.spacing_mut().button_padding =
                                                        crate::ui::tokens::pad::MENU;
                                                    if ui.button("Switch palette…").clicked() {
                                                        switcher.open_fresh();
                                                    }
                                                    ui.separator();
                                                    if ui.button("New palette").clicked() {
                                                        working.clear();
                                                        let name =
                                                            palette::next_palette_name(&palettes.0);
                                                        palettes.0.push(Palette {
                                                            name,
                                                            colors: Vec::new(),
                                                            builtin: false,
                                                        });
                                                        let i = palettes.0.len() - 1;
                                                        palette_choice.0 = i;
                                                        palette_rename.editing = Some(i);
                                                        palette_rename.buf =
                                                            palettes.0[i].name.clone();
                                                        io::palettes::save(&palettes.0);
                                                    }
                                                    let save_as_label = if active_is_builtin {
                                                        "Save as new palette"
                                                    } else {
                                                        "Duplicate"
                                                    };
                                                    if ui.button(save_as_label).clicked() {
                                                        let i = palette::save_as_new(
                                                            &mut palettes,
                                                            &mut palette_choice,
                                                            &mut working,
                                                        );
                                                        palette_rename.editing = Some(i);
                                                        palette_rename.buf =
                                                            palettes.0[i].name.clone();
                                                        io::palettes::save(&palettes.0);
                                                    }
                                                    if !active_is_builtin {
                                                        if ui.button("Rename…").clicked() {
                                                            palette_rename.editing =
                                                                Some(active_idx);
                                                            palette_rename.buf =
                                                                palettes.0[active_idx].name.clone();
                                                        }
                                                        let has_user = palettes
                                                            .0
                                                            .iter()
                                                            .filter(|p| !p.builtin)
                                                            .count()
                                                            > 0;
                                                        if ui
                                                            .add_enabled(
                                                                has_user,
                                                                egui::Button::new("Delete"),
                                                            )
                                                            .clicked()
                                                        {
                                                            palettes.0.remove(active_idx);
                                                            if palette_choice.0 >= palettes.0.len()
                                                            {
                                                                palette_choice.0 = palettes
                                                                    .0
                                                                    .len()
                                                                    .saturating_sub(1);
                                                            }
                                                            palette_rename.editing = None;
                                                            working.clear();
                                                            io::palettes::save(&palettes.0);
                                                        }
                                                    }
                                                    ui.separator();
                                                    let dialog_busy = pending.is_active();
                                                    let safe_idx = palette_choice
                                                        .0
                                                        .min(palettes.0.len().saturating_sub(1));
                                                    let export_name =
                                                        palettes.0[safe_idx].name.clone();
                                                    let export_colors = palette::display_colors(
                                                        &palettes, &working, safe_idx,
                                                    )
                                                    .to_vec();
                                                    let default_filename = format!(
                                                        "{}.ase",
                                                        palette::sanitize_filename(&export_name)
                                                    );
                                                    if ui
                                                        .add_enabled(
                                                            !dialog_busy,
                                                            egui::Button::new("Export .ase…"),
                                                        )
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
                                                    if ui
                                                        .add_enabled(
                                                            !dialog_busy,
                                                            egui::Button::new("Import .ase…"),
                                                        )
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
                                                });
                                    });

                                    // Refresh — menu may have changed the selection.
                                    let active_idx =
                                        palette_choice.0.min(palettes.0.len().saturating_sub(1));
                                    palette_choice.0 = active_idx;
                                    let active_is_builtin = palettes.0[active_idx].builtin;

                                    if palette_rename.editing == Some(active_idx)
                                        && !active_is_builtin
                                    {
                                        ui.add_space(space::XS);
                                        ui.horizontal(|ui| {
                                            ui.spacing_mut().item_spacing.x = gap::TIGHT.x;
                                            let resp = ui.add(
                                                egui::TextEdit::singleline(&mut palette_rename.buf)
                                                    .desired_width((row_w - 76.0).max(60.0)),
                                            );
                                            if !resp.has_focus() && !resp.lost_focus() {
                                                resp.request_focus();
                                            }
                                            let commit = widgets::icon_only_button(
                                                ui,
                                                &theme,
                                                icons::check(),
                                                true,
                                            )
                                            .on_hover_text("Save name")
                                            .clicked()
                                                || (resp.lost_focus()
                                                    && ui.input(|i| {
                                                        i.key_pressed(egui::Key::Enter)
                                                    }));
                                            let cancel = widgets::icon_only_button(
                                                ui,
                                                &theme,
                                                icons::x(),
                                                true,
                                            )
                                            .on_hover_text("Cancel")
                                            .clicked()
                                                || ui.input(|i| i.key_pressed(egui::Key::Escape));
                                            if commit {
                                                let trimmed = palette_rename.buf.trim();
                                                if !trimmed.is_empty() {
                                                    palettes.0[active_idx].name =
                                                        trimmed.to_string();
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

                                    // Built-in with unsaved scratch edits: nudge toward the
                                    // "Save as new palette" action in the … menu.
                                    if active_is_builtin && working.is_dirty_for(active_idx) {
                                        ui.add_space(space::SX);
                                        widgets::hint_label(
                                            ui,
                                            &theme,
                                            "Modified — “Save as new palette” in the … menu to keep these edits",
                                        );
                                    }

                                    let active_colors =
                                        palette::display_colors(&palettes, &working, active_idx)
                                            .to_vec();

                                    let add_enabled = !active_colors.contains(&color.0);
                                    let mut add_clicked = false;
                                    let mut reorder: Option<(usize, usize)> = None;
                                    let mut remove_idx: Option<usize> = None;

                                    // Manual grid layout: each cell is painted at a computed
                                    // rect and interacts via `ui.interact`, instead of letting
                                    // egui flow the swatches. That lets a dragged swatch leave
                                    // its slot so the rest shift to open a gap at the drop
                                    // target — the "make space" feedback. Fixed rows also dodge
                                    // the wrap bug where click-and-drag cells refuse to wrap in
                                    // `horizontal_wrapped`.
                                    let n = active_colors.len();
                                    let cell = swatch::PALETTE;
                                    let stepx = cell.x + gap::TIGHT.x;
                                    let stepy = cell.y + gap::TIGHT.y;
                                    let avail = ui.available_width();
                                    let cols = ((avail + gap::TIGHT.x) / stepx).floor().max(1.0)
                                        as usize;
                                    let total = n + 1; // colours + trailing `+` cell
                                    let rows = total.div_ceil(cols);
                                    let grid_h = (rows as f32 * stepy - gap::TIGHT.y).max(cell.y);

                                    // Drag source set on a previous frame (egui clears the dnd
                                    // payload on pointer release), clamped to a live index.
                                    let dragging = egui::DragAndDrop::payload::<usize>(ui.ctx())
                                        .map(|p| *p)
                                        .filter(|&d| d < n);

                                    let (grid_rect, _) = ui.allocate_exact_size(
                                        egui::vec2(avail, grid_h),
                                        egui::Sense::hover(),
                                    );
                                    let origin = grid_rect.min;
                                    let cell_at = |slot: usize| -> egui::Rect {
                                        let r = slot / cols;
                                        let c = slot % cols;
                                        egui::Rect::from_min_size(
                                            origin
                                                + egui::vec2(c as f32 * stepx, r as f32 * stepy),
                                            cell,
                                        )
                                    };

                                    // Non-dragged swatches in order; while dragging, a gap is
                                    // opened at `gap_pos` (an index into this list) under the
                                    // pointer.
                                    let others: Vec<usize> =
                                        (0..n).filter(|&i| Some(i) != dragging).collect();
                                    let pointer = ui.ctx().pointer_interact_pos();
                                    let gap_pos: Option<usize> = if dragging.is_some() {
                                        pointer.map(|p| {
                                            let rel = p - origin;
                                            let c = ((rel.x / stepx) + 0.5)
                                                .floor()
                                                .clamp(0.0, cols as f32);
                                            let r = (rel.y / stepy)
                                                .floor()
                                                .clamp(0.0, (rows as f32 - 1.0).max(0.0));
                                            ((r as usize) * cols + c as usize).min(others.len())
                                        })
                                    } else {
                                        None
                                    };

                                    enum Slot {
                                        Color(usize),
                                        Gap,
                                        Add,
                                    }
                                    let mut slots: Vec<Slot> = Vec::with_capacity(total + 1);
                                    for (pos, &oi) in others.iter().enumerate() {
                                        if gap_pos == Some(pos) {
                                            slots.push(Slot::Gap);
                                        }
                                        slots.push(Slot::Color(oi));
                                    }
                                    if gap_pos == Some(others.len()) {
                                        slots.push(Slot::Gap);
                                    }
                                    slots.push(Slot::Add);

                                    let base = ui.id();
                                    for (slot_idx, slot) in slots.iter().enumerate() {
                                        let rect = cell_at(slot_idx);
                                        match slot {
                                            Slot::Gap => widgets::paint_swatch_gap(
                                                ui,
                                                &theme,
                                                rect,
                                                radius::XS,
                                            ),
                                            Slot::Add => {
                                                let resp = ui.interact(
                                                    rect,
                                                    base.with("palette_add"),
                                                    egui::Sense::click(),
                                                );
                                                widgets::paint_add_swatch(
                                                    ui,
                                                    &theme,
                                                    rect,
                                                    radius::XS,
                                                    add_enabled,
                                                    resp.hovered(),
                                                );
                                                let resp = resp.on_hover_text(if add_enabled {
                                                    "Add current color"
                                                } else {
                                                    "Color already in palette"
                                                });
                                                if add_enabled {
                                                    if resp.clicked() {
                                                        add_clicked = true;
                                                    }
                                                    resp.on_hover_cursor(
                                                        egui::CursorIcon::PointingHand,
                                                    );
                                                }
                                            }
                                            Slot::Color(oi) => {
                                                let si = *oi;
                                                let c = active_colors[si];
                                                let col = egui::Color32::from_rgba_unmultiplied(
                                                    c[0], c[1], c[2], 255,
                                                );
                                                let select_state = if color.0 == c {
                                                    widgets::SwatchSelect::Primary
                                                } else if extras.contains(c) {
                                                    widgets::SwatchSelect::Extra
                                                } else {
                                                    widgets::SwatchSelect::None
                                                };
                                                let resp = ui.interact(
                                                    rect,
                                                    base.with(("palette_swatch", si)),
                                                    egui::Sense::click_and_drag(),
                                                );
                                                widgets::paint_swatch(
                                                    ui,
                                                    &theme,
                                                    rect,
                                                    col,
                                                    radius::XS,
                                                    select_state,
                                                );
                                                if resp.clicked() {
                                                    let shift =
                                                        ui.input(|i| i.modifiers.shift);
                                                    color.0 = apply_swatch_click(
                                                        shift,
                                                        c,
                                                        color.0,
                                                        &mut extras,
                                                    );
                                                }
                                                if resp.dragged() {
                                                    egui::DragAndDrop::set_payload(
                                                        ui.ctx(),
                                                        si,
                                                    );
                                                }
                                                egui::Popup::context_menu(&resp).show(|ui| {
                                                    if ui.button("Remove").clicked() {
                                                        remove_idx = Some(si);
                                                        ui.close();
                                                    }
                                                });
                                                resp.on_hover_text(format!(
                                                    "{}  (drag to reorder, right-click to remove)",
                                                    widgets::hex_string([c[0], c[1], c[2]]),
                                                ));
                                            }
                                        }
                                    }

                                    // The dragged swatch follows the cursor as a lifted ghost;
                                    // its interaction id lives here now (it left its grid slot),
                                    // so the release is detected and committed against the gap.
                                    if let Some(from) = dragging {
                                        let c = active_colors[from];
                                        let col = egui::Color32::from_rgba_unmultiplied(
                                            c[0], c[1], c[2], 255,
                                        );
                                        let ghost_center =
                                            pointer.unwrap_or_else(|| cell_at(0).center());
                                        let ghost =
                                            egui::Rect::from_center_size(ghost_center, cell);
                                        ui.painter().rect_filled(
                                            ghost.translate(egui::vec2(0.0, 2.0)),
                                            egui::CornerRadius::same(radius::XS),
                                            egui::Color32::from_black_alpha(70),
                                        );
                                        ui.painter().rect(
                                            ghost,
                                            egui::CornerRadius::same(radius::XS),
                                            col,
                                            egui::Stroke::new(stroke::ACCENT, theme.text),
                                            egui::StrokeKind::Inside,
                                        );
                                        ui.ctx()
                                            .set_cursor_icon(egui::CursorIcon::Grabbing);
                                        let resp = ui.interact(
                                            ghost,
                                            base.with(("palette_swatch", from)),
                                            egui::Sense::click_and_drag(),
                                        );
                                        if resp.dragged() {
                                            egui::DragAndDrop::set_payload(ui.ctx(), from);
                                        }
                                        if resp.drag_stopped() {
                                            if let Some(to) = gap_pos {
                                                if to != from {
                                                    reorder = Some((from, to));
                                                }
                                            }
                                        }
                                    }

                                    if add_clicked {
                                        let (colors, persist) = palette::edit_colors(
                                            &mut palettes,
                                            &mut working,
                                            active_idx,
                                        );
                                        colors.push(color.0);
                                        if persist {
                                            io::palettes::save(&palettes.0);
                                        }
                                    }
                                    if let Some(i) = remove_idx {
                                        let (colors, persist) = palette::edit_colors(
                                            &mut palettes,
                                            &mut working,
                                            active_idx,
                                        );
                                        if i < colors.len() {
                                            colors.remove(i);
                                        }
                                        if persist {
                                            io::palettes::save(&palettes.0);
                                        }
                                    }
                                    if let Some((from, to)) = reorder {
                                        let (colors, persist) = palette::edit_colors(
                                            &mut palettes,
                                            &mut working,
                                            active_idx,
                                        );
                                        if from < colors.len() && to < colors.len() {
                                            let c = colors.remove(from);
                                            colors.insert(to, c);
                                            if persist {
                                                io::palettes::save(&palettes.0);
                                            }
                                        }
                                    }

                                    widgets::section_divider(ui, &theme);
                                }

                                // Selection section — only when there's an active region.
                                if let Some(aabb) = selection.aabb {
                                    widgets::section(ui, &theme, "Selection", |ui| {
                                        let extents = aabb.extents();
                                        widgets::stat_row(
                                            ui,
                                            &theme,
                                            "Bounds",
                                            format!(
                                                "{} × {} × {}",
                                                extents.x, extents.y, extents.z
                                            ),
                                        );
                                        widgets::stat_row(
                                            ui,
                                            &theme,
                                            "Voxels",
                                            selection.voxel_count(&grid).to_string(),
                                        );
                                    });
                                }
                            });
                        });
                }),
        )
    } else {
        None
    };
    if let Some(resp) = inspector_resp {
        let rect = resp.response.rect;
        onboarding_anchors.color_palette = Some(rect);
        // Background order so floating windows (command palette, palette switcher
        // — both `egui::Window` at `Order::Middle`) render above the edge line
        // instead of having it slice across them.
        let painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Background,
            egui::Id::new("inspector_panel_edge"),
        ));
        painter.vline(
            painter.round_to_pixel_center(rect.right()),
            rect.y_range(),
            egui::Stroke::new(stroke::HAIR, BORDER),
        );
    }

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
            ui.set_min_width(width::MODAL_PREFS);
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
                        widgets::hex_label(ui, &theme, *rgb, true);
                    });
                }
            });

            widgets::section(ui, &theme, "Visibility", |ui| {
                ui.checkbox(&mut prefs.show_floor_grid, "Show floor grid");
                ui.add_space(space::XXS);
                ui.checkbox(&mut prefs.show_origin_axes, "Show origin axes");
                #[cfg(not(target_os = "macos"))]
                {
                    ui.add_space(space::XXS);
                    ui.checkbox(&mut prefs.show_floating_menu_bar, "Show floating menu bar");
                }
            });

            // Color-space format for the picker's numeric fields. Demoted here
            // from the inspector: most users never leave Hex, so it doesn't earn
            // panel space in a beginner-friendly editor.
            widgets::section(ui, &theme, "Color", |ui| {
                widgets::prefs_row(ui, &theme, "Format", |ui| {
                    let labels: Vec<String> = ColorSpace::ALL
                        .iter()
                        .map(|s| s.label().to_string())
                        .collect();
                    let idx = ColorSpace::ALL
                        .iter()
                        .position(|s| *s == prefs.color_space)
                        .unwrap_or(0);
                    if let Some(i) = widgets::select_dropdown(
                        ui,
                        &theme,
                        "prefs_color_space",
                        width::PREFS_DROPDOWN,
                        prefs.color_space.label(),
                        &labels,
                        idx,
                    ) {
                        prefs.color_space = ColorSpace::ALL[i];
                    }
                });
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
    // is just "do you want to throw away unsaved work?". Built with an
    // anchored `Area` instead of `egui::Window` so the modal sizes tight to
    // its content (egui::Window kept ballooning vertically here).
    if new_project.dialog_open {
        let mut create_clicked = false;
        let mut cancel_clicked = false;
        egui::Area::new("new_project_modal".into())
            .order(egui::Order::Foreground)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ctx, |ui| {
                egui::Frame::NONE
                    .fill(theme.panel)
                    .inner_margin(egui::Margin::symmetric(20, 16))
                    .shadow(crate::ui::tokens::shadow::high())
                    .corner_radius(egui::CornerRadius::same(radius::LG))
                    .show(ui, |ui| {
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
                            widgets::hint_label(ui, &theme, "Discard unsaved work and start over?");
                            ui.add_space(space::SM);
                            ui.horizontal(|ui| {
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        ui.spacing_mut().item_spacing.x = space::SX;
                                        if widgets::dialog_button(ui, &theme, "Create", true)
                                            .clicked()
                                        {
                                            create_clicked = true;
                                        }
                                        if widgets::dialog_button(ui, &theme, "Cancel", false)
                                            .clicked()
                                        {
                                            cancel_clicked = true;
                                        }
                                    },
                                );
                            });
                        });
                    });
            });
        let esc = ctx.input(|i| i.key_pressed(egui::Key::Escape));
        if create_clicked {
            new_project.apply = true;
            new_project.dialog_open = false;
        } else if cancel_clicked || esc {
            new_project.dialog_open = false;
        }
    }

    // Command-palette-style palette switcher (opened from the … menu).
    if let Some(target) = palette_switcher::draw(ctx, &theme, &mut switcher, &palettes.0) {
        palette::request_select(target, &mut palette_choice, &mut working, &mut discard);
    }

    // Discard-edits confirm: switching away from a dirty built-in stages a
    // target index here so the user can keep their scratch edits first.
    if let Some(target) = discard.pending {
        let name = palettes.0[palette_choice.0.min(palettes.0.len().saturating_sub(1))]
            .name
            .clone();
        let mut open = true;
        let mut save_clicked = false;
        let mut discard_clicked = false;
        let mut cancel_clicked = false;
        widgets::modal_window(ctx, &theme, "Discard edits?", &mut open).show(ctx, |ui| {
            ui.set_width(width::MODAL_DISCARD);
            widgets::hint_label(
                ui,
                &theme,
                &format!("“{name}” has unsaved swatches. Save them as a new palette, or discard?"),
            );
            ui.add_space(space::SM);
            ui.horizontal(|ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = space::SX;
                    if widgets::dialog_button(ui, &theme, "Save as new", true).clicked() {
                        save_clicked = true;
                    }
                    if widgets::dialog_button(ui, &theme, "Discard", false).clicked() {
                        discard_clicked = true;
                    }
                    if widgets::dialog_button(ui, &theme, "Cancel", false).clicked() {
                        cancel_clicked = true;
                    }
                });
            });
        });
        if save_clicked {
            let i = palette::save_as_new(&mut palettes, &mut palette_choice, &mut working);
            palette_rename.editing = Some(i);
            palette_rename.buf = palettes.0[i].name.clone();
            io::palettes::save(&palettes.0);
            discard.pending = None;
        } else if discard_clicked {
            working.clear();
            palette_choice.0 = target.min(palettes.0.len().saturating_sub(1));
            discard.pending = None;
        } else if cancel_clicked || !open {
            discard.pending = None;
        }
    }

    if cmd_palette.open {
        let state = command_palette::CatalogState {
            tool: tool.current,
            shape: &shape_options,
            has_undo: !history.undo.is_empty(),
            has_redo: !history.redo.is_empty(),
            has_selection: selection.aabb.is_some(),
            has_clipboard: clipboard.has_stamp(),
            has_voxels: grid.count() > 0,
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

/// Custom color picker popup body — the single numeric edit surface for the
/// foreground color. Composed of:
///
/// 1. **Field row** — buffer-backed text inputs in the active [`ColorSpace`]
///    (`Hex` = one field, others = three). Backed by [`ColorEditBuffer`] so
///    keystrokes don't roundtrip through `Color8` mid-edit (greys lose hue,
///    OKLCH chroma quantises). Commit on `lost_focus`; invalid reverts.
///    Arrow keys step the focused field (Shift = ×10).
/// 2. **2D area** — saturation (x) × value (y), painted as a vertex-gradient
///    mesh. Click/drag sets S and V; the hue used to render the area lives
///    in egui memory so dragging through grey (S=0) doesn't lose the hue.
/// 3. **Hue bar** — full hue gradient strip, click/drag to set hue.
///
/// The active space comes from `Preferences.color_space` (chosen in the
/// Preferences window, not here). State cache `(last_rgba, h_norm_0_1)` is
/// keyed by widget id; we re-derive the hue from RGB only when the live color
/// changed elsewhere (palette click, eyedropper, field edit). All writes
/// funnel through `color.0`.
fn space_color_picker(
    ui: &mut egui::Ui,
    color: &mut crate::tools::CurrentColor,
    space: ColorSpace,
    color_edit: &mut crate::color_space::ColorEditBuffer,
) -> bool {
    use crate::color_space::{hsb_to_rgb, rgb_to_hsb};

    // Cache: last rgba we produced + working hue (0..1). Per-widget id so a
    // palette swatch popup can't clobber the inspector's working state.
    type State = ([u8; 4], f32);
    let cache_id = ui.id().with("space_color_picker_state");
    let cached: Option<State> = ui.data(|d| d.get_temp(cache_id));

    let rgb3 = [color.0[0], color.0[1], color.0[2]];
    let mut hue_norm = match cached {
        Some((rgba, h)) if rgba == color.0 => h,
        _ => {
            let (h, _, _) = rgb_to_hsb(rgb3);
            h / 360.0
        }
    };

    let mut changed = false;
    let area_size = 220.0;

    // ---------- Editable field row ----------
    // Repopulate when the live color or active space drifts from the buffer so
    // typing isn't clobbered by a quantised readback (see `ColorEditBuffer`).
    if color_edit.source != color.0 || color_edit.space != space {
        color_edit.populate(color.0, space);
    }
    // Commit on lost_focus (Enter / Tab / click-away); invalid input reverts
    // silently. Arrow-key stepping keeps the buffer authoritative so small
    // HSL/HSB/OKLCH steps survive the 8-bit RGB roundtrip.
    let mut commit_now = false;
    let mut stepped = false;
    match space {
        ColorSpace::Hex => {
            let resp = ui.add(
                egui::TextEdit::singleline(&mut color_edit.fields[0])
                    .font(egui::TextStyle::Monospace)
                    .desired_width(area_size),
            );
            if resp.lost_focus() {
                commit_now = true;
            }
        }
        _ => {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = gap::TIGHT.x;
                let w = ((area_size - 8.0) / 3.0).max(40.0);
                for i in 0..3 {
                    let resp = ui.add(
                        egui::TextEdit::singleline(&mut color_edit.fields[i])
                            .font(egui::TextStyle::Monospace)
                            .desired_width(w),
                    );
                    if resp.has_focus() {
                        let (up, down, shift) = ui.input(|inp| {
                            (
                                inp.key_pressed(egui::Key::ArrowUp),
                                inp.key_pressed(egui::Key::ArrowDown),
                                inp.modifiers.shift,
                            )
                        });
                        let step = crate::color_space::ColorEditBuffer::field_step(space, i, shift);
                        if up && color_edit.step_field(i, step) {
                            stepped = true;
                        }
                        if down && color_edit.step_field(i, -step) {
                            stepped = true;
                        }
                    }
                    if resp.lost_focus() {
                        commit_now = true;
                    }
                }
            });
        }
    }
    if stepped {
        if let Some(rgb) = color_edit.commit() {
            color.0 = [rgb[0], rgb[1], rgb[2], 255];
            // Keep source in sync so the gate above doesn't repopulate and
            // clobber the stepped buffer with a quantised readback.
            color_edit.source = color.0;
            let (h, _, _) = rgb_to_hsb([color.0[0], color.0[1], color.0[2]]);
            hue_norm = h / 360.0;
            changed = true;
        }
    }
    if commit_now {
        if let Some(rgb) = color_edit.commit() {
            color.0 = [rgb[0], rgb[1], rgb[2], 255];
            let (h, _, _) = rgb_to_hsb([color.0[0], color.0[1], color.0[2]]);
            hue_norm = h / 360.0;
            changed = true;
        }
        color_edit.populate(color.0, space);
    }

    ui.add_space(space::XS);

    // Live (S, V) read from current color (in 0..1).
    let (_h_live, s_live, v_live) = rgb_to_hsb([color.0[0], color.0[1], color.0[2]]);
    let mut s_norm = s_live / 100.0;
    let mut v_norm = v_live / 100.0;

    // ---------- 2D S × V area ----------
    let (rect, resp) = ui.allocate_at_least(
        egui::vec2(area_size, area_size),
        egui::Sense::click_and_drag(),
    );
    if let Some(mpos) = resp.interact_pointer_pos() {
        s_norm = egui::emath::remap_clamp(mpos.x, rect.left()..=rect.right(), 0.0..=1.0);
        v_norm = egui::emath::remap_clamp(mpos.y, rect.bottom()..=rect.top(), 0.0..=1.0);
        let rgb = hsb_to_rgb(hue_norm * 360.0, s_norm * 100.0, v_norm * 100.0);
        color.0 = [rgb[0], rgb[1], rgb[2], 255];
        changed = true;
    }
    if ui.is_rect_visible(rect) {
        const N: usize = 16;
        let mut mesh = egui::epaint::Mesh::default();
        for yi in 0..=N {
            for xi in 0..=N {
                let xt = xi as f32 / N as f32;
                let yt = yi as f32 / N as f32;
                let rgb = hsb_to_rgb(hue_norm * 360.0, xt * 100.0, (1.0 - yt) * 100.0);
                let cell = egui::Color32::from_rgb(rgb[0], rgb[1], rgb[2]);
                let x = egui::emath::lerp(rect.left()..=rect.right(), xt);
                let y = egui::emath::lerp(rect.top()..=rect.bottom(), yt);
                mesh.colored_vertex(egui::pos2(x, y), cell);
                if xi < N && yi < N {
                    let row = (N + 1) as u32;
                    let tl = (yi * (N + 1) + xi) as u32;
                    mesh.add_triangle(tl, tl + 1, tl + row);
                    mesh.add_triangle(tl + 1, tl + row, tl + row + 1);
                }
            }
        }
        ui.painter().add(egui::Shape::mesh(mesh));
        ui.painter().rect_stroke(
            rect,
            0.0,
            egui::Stroke::new(
                stroke::HAIR,
                ui.visuals().widgets.noninteractive.bg_stroke.color,
            ),
            egui::StrokeKind::Inside,
        );
        let mx = egui::emath::lerp(rect.left()..=rect.right(), s_norm);
        let my = egui::emath::lerp(rect.top()..=rect.bottom(), 1.0 - v_norm);
        let dot = hsb_to_rgb(hue_norm * 360.0, s_norm * 100.0, v_norm * 100.0);
        ui.painter().circle(
            egui::pos2(mx, my),
            5.5,
            egui::Color32::from_rgb(dot[0], dot[1], dot[2]),
            egui::Stroke::new(2.0, egui::Color32::WHITE),
        );
    }

    ui.add_space(space::XS);

    // ---------- Hue bar ----------
    let bar_h = 16.0;
    let (hue_rect, hue_resp) =
        ui.allocate_at_least(egui::vec2(area_size, bar_h), egui::Sense::click_and_drag());
    if let Some(mpos) = hue_resp.interact_pointer_pos() {
        hue_norm = egui::emath::remap_clamp(mpos.x, hue_rect.left()..=hue_rect.right(), 0.0..=1.0);
        let rgb = hsb_to_rgb(hue_norm * 360.0, s_norm * 100.0, v_norm * 100.0);
        color.0 = [rgb[0], rgb[1], rgb[2], 255];
        changed = true;
    }
    if ui.is_rect_visible(hue_rect) {
        const N: usize = 24;
        let mut mesh = egui::epaint::Mesh::default();
        for i in 0..=N {
            let t = i as f32 / N as f32;
            let rgb = hsb_to_rgb(t * 360.0, 100.0, 100.0);
            let c = egui::Color32::from_rgb(rgb[0], rgb[1], rgb[2]);
            let x = egui::emath::lerp(hue_rect.left()..=hue_rect.right(), t);
            mesh.colored_vertex(egui::pos2(x, hue_rect.top()), c);
            mesh.colored_vertex(egui::pos2(x, hue_rect.bottom()), c);
            if i < N {
                let base = (i * 2) as u32;
                mesh.add_triangle(base, base + 1, base + 2);
                mesh.add_triangle(base + 1, base + 2, base + 3);
            }
        }
        ui.painter().add(egui::Shape::mesh(mesh));
        ui.painter().rect_stroke(
            hue_rect,
            0.0,
            egui::Stroke::new(
                stroke::HAIR,
                ui.visuals().widgets.noninteractive.bg_stroke.color,
            ),
            egui::StrokeKind::Inside,
        );
        let mx = egui::emath::lerp(hue_rect.left()..=hue_rect.right(), hue_norm);
        let r = bar_h * 0.4;
        ui.painter().add(egui::Shape::convex_polygon(
            vec![
                egui::pos2(mx, hue_rect.center().y),
                egui::pos2(mx + r, hue_rect.bottom()),
                egui::pos2(mx - r, hue_rect.bottom()),
            ],
            egui::Color32::WHITE,
            egui::Stroke::new(1.0, egui::Color32::BLACK),
        ));
    }

    ui.data_mut(|d| d.insert_temp(cache_id, (color.0, hue_norm)));
    changed
}
