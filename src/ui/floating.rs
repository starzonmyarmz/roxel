//! Free-floating UI surfaces drawn over the canvas: the right-edge tool
//! island and (on Win/Linux) the top-center menu pill. Both follow the toast
//! pattern — a shadowed `egui::Area` styled with a pill-shaped
//! `Frame::popup` and anchored by pivot against `ctx.available_rect()` so
//! they nest cleanly beside any registered side panel.

use crate::theme::{Preferences, Theme};
use crate::tools::{ShapeOptions, Tool, ToolState};
use crate::ui::tokens::{gap, pad, radius, shadow, space, swatch};
use crate::ui::{icons, widgets};
use bevy_egui::egui;
use roxel::shapes::ShapePrimitive;

// The menu pill is Win/Linux only (macOS uses the native menu), so its
// contents helper and the deps it pulls in are gated to match.
#[cfg(not(target_os = "macos"))]
use crate::onboarding::Onboarding;
#[cfg(not(target_os = "macos"))]
use crate::theme::PreferencesWindow;
#[cfg(not(target_os = "macos"))]
use crate::ui::dialogs::{self, CurrentProjectPath, DialogResult, OpenRequest, PendingDialog};
#[cfg(not(target_os = "macos"))]
use crate::ui::tokens::{font, icon, width};
#[cfg(not(target_os = "macos"))]
use roxel::color_space::ColorSpace;
#[cfg(not(target_os = "macos"))]
use roxel::grid::{NewProject, VoxelGrid};
#[cfg(not(target_os = "macos"))]
use roxel::history::History;

/// Frame used for every floating surface. Panel fill, rounded to
/// [`radius::PILL`] so corners read the same on every floating element. No
/// border — the soft drop shadow is what lifts the surface off the canvas.
pub fn pill_frame(theme: &Theme) -> egui::Frame {
    egui::Frame::default()
        .fill(theme.panel)
        .stroke(egui::Stroke::NONE)
        .corner_radius(egui::CornerRadius::same(radius::PILL))
        .inner_margin(egui::Margin::symmetric(
            pad::DEFAULT.x as i8,
            pad::DEFAULT.y as i8,
        ))
        .shadow(shadow::mid())
}

/// Render a floating area pinned to `anchor` with the given pivot. Used by
/// every floating helper so anchoring/order is consistent.
pub fn floating_area(
    ctx: &egui::Context,
    id: impl std::hash::Hash,
    anchor: egui::Pos2,
    pivot: egui::Align2,
    add_contents: impl FnOnce(&mut egui::Ui),
) -> egui::Response {
    egui::Area::new(egui::Id::new(id))
        .order(egui::Order::Foreground)
        .fixed_pos(anchor)
        .pivot(pivot)
        .show(ctx, add_contents)
        .response
}

/// Right-edge vertically-centered anchor for the tool island.
pub fn tool_island_anchor(ctx: &egui::Context) -> egui::Pos2 {
    let canvas = ctx.available_rect();
    let cy = (canvas.min.y + canvas.max.y) * 0.5;
    egui::pos2(canvas.max.x - space::FLOAT_GAP, cy)
}

/// Top-center anchor for the Win/Linux floating menu pill.
#[cfg(not(target_os = "macos"))]
pub fn pill_menu_anchor(ctx: &egui::Context) -> egui::Pos2 {
    let canvas = ctx.available_rect();
    let cx = (canvas.min.x + canvas.max.x) * 0.5;
    egui::pos2(cx, canvas.min.y + space::FLOAT_GAP)
}

/// Draw the floating tool island on the right edge, vertically centered. The
/// contents closure receives a vertically-laid-out [`egui::Ui`].
pub fn tool_island(
    ctx: &egui::Context,
    theme: &Theme,
    contents: impl FnOnce(&mut egui::Ui),
) -> egui::Response {
    let anchor = tool_island_anchor(ctx);
    floating_area(
        ctx,
        "tool_island",
        anchor,
        egui::Align2::RIGHT_CENTER,
        |ui| {
            pill_frame(theme)
                .inner_margin(egui::Margin::same(pad::DEFAULT.x as i8))
                .show(ui, |ui| {
                    ui.vertical_centered(|ui| contents(ui));
                });
        },
    )
}

/// Draw the floating menu pill at top-center (Win/Linux only). Sizes to its
/// content. The contents closure receives a horizontally-laid-out
/// [`egui::Ui`].
#[cfg(not(target_os = "macos"))]
pub fn pill_menu(
    ctx: &egui::Context,
    theme: &Theme,
    contents: impl FnOnce(&mut egui::Ui),
) -> egui::Response {
    let anchor = pill_menu_anchor(ctx);
    floating_area(ctx, "pill_menu", anchor, egui::Align2::CENTER_TOP, |ui| {
        pill_frame(theme).show(ui, |ui| {
            ui.horizontal(|ui| contents(ui));
        });
    })
}

/// Contents of the floating tool island: the vertical tool buttons plus the
/// shape-primitive long-press picker. Split out of `ui_system` to keep that
/// function readable; mutates `tool`/`shape_options`/`prefs` directly.
pub fn tool_island_contents(
    ui: &mut egui::Ui,
    theme: &Theme,
    tool: &mut ToolState,
    shape_options: &mut ShapeOptions,
    prefs: &mut Preferences,
) {
    ui.spacing_mut().item_spacing.y = 0.0;
    widgets::tool_button(
        ui,
        theme,
        tool,
        Tool::Brush,
        icons::tool(Tool::Brush),
        "Brush",
        "B",
    );
    ui.add_space(space::SX);
    widgets::tool_button(
        ui,
        theme,
        tool,
        Tool::Erase,
        icons::tool(Tool::Erase),
        "Erase",
        "E",
    );
    ui.add_space(space::SX);
    widgets::tool_button(
        ui,
        theme,
        tool,
        Tool::Paint,
        icons::tool(Tool::Paint),
        "Paint",
        "P",
    );
    ui.add_space(space::SX);
    widgets::tool_button(
        ui,
        theme,
        tool,
        Tool::Eyedropper,
        icons::tool(Tool::Eyedropper),
        "Pick",
        "I",
    );
    ui.add_space(space::SX);
    let shape_resp = widgets::tool_button(
        ui,
        theme,
        tool,
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
        let neutral_hover = theme.hover_fill();
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
                            (ShapePrimitive::Sphere, "Sphere"),
                        ] {
                            let selected = shape_options.primitive == prim;
                            let (rect, r) = ui.allocate_exact_size(cell, egui::Sense::click());
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
                            let icon_rect = egui::Rect::from_center_size(rect.center(), icon_size);
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
                                prefs.last_shape = prim;
                                crate::theme::save_preferences(prefs);
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
        theme,
        tool,
        Tool::Select,
        icons::tool(Tool::Select),
        "Marquee select",
        "M",
    );
    ui.add_space(space::SX);
    widgets::tool_button(
        ui,
        theme,
        tool,
        Tool::Move,
        icons::tool(Tool::Move),
        "Move",
        "V",
    );
}

#[cfg(not(target_os = "macos"))]
#[allow(clippy::too_many_arguments)]
pub fn pill_menu_contents(
    ui: &mut egui::Ui,
    theme: &Theme,
    new_project: &mut NewProject,
    pending: &mut PendingDialog,
    open_request: &mut OpenRequest,
    current_path: &CurrentProjectPath,
    prefs: &mut Preferences,
    history: &mut History,
    grid: &mut VoxelGrid,
    onboarding: &mut Onboarding,
    prefs_window: &mut PreferencesWindow,
    updater: &mut crate::updater::UpdateCheck,
) {
    #[allow(non_snake_case)]
    let TEXT = theme.text;
    #[allow(non_snake_case)]
    let TEXT_DIM = theme.text_dim;
    ui.horizontal(|ui| {
        if widgets::icon_button(ui, theme, icons::file_plus(), "New")
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
            // Route through the guard so an unsaved document prompts
            // before the open dialog replaces it.
            open_request.requested = true;
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
            dialogs::spawn_save(pending, current_path, prefs.last_dir.clone());
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
            dialogs::spawn_save_as(pending, current_path, prefs.last_dir.clone());
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
                    let start_dir = prefs.last_dir.clone();
                    pending.spawn(async move {
                        dialogs::new_dialog(&start_dir)
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
                    let start_dir = prefs.last_dir.clone();
                    pending.spawn(async move {
                        dialogs::new_dialog(&start_dir)
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
                    let start_dir = prefs.last_dir.clone();
                    pending.spawn(async move {
                        dialogs::new_dialog(&start_dir)
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
                    let start_dir = prefs.last_dir.clone();
                    pending.spawn(async move {
                        dialogs::new_dialog(&start_dir)
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
                    let start_dir = prefs.last_dir.clone();
                    pending.spawn(async move {
                        dialogs::new_dialog(&start_dir)
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
                    let start_dir = prefs.last_dir.clone();
                    pending.spawn(async move {
                        dialogs::new_dialog(&start_dir)
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
                    let start_dir = prefs.last_dir.clone();
                    pending.spawn(async move {
                        dialogs::new_dialog(&start_dir)
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
                    let start_dir = prefs.last_dir.clone();
                    pending.spawn(async move {
                        dialogs::new_dialog(&start_dir)
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
                    let start_dir = prefs.last_dir.clone();
                    pending.spawn(async move {
                        dialogs::new_dialog(&start_dir)
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

        ui.add_space(space::XS);
        ui.menu_image_text_button(
            egui::Image::new(icons::eye())
                .fit_to_exact_size(icon::md_square())
                .tint(TEXT),
            egui::RichText::new("View").size(font::BODY),
            |ui| {
                ui.set_min_width(width::TOP_BAR_MENU);
                let toggle = |ui: &mut egui::Ui, on: &mut bool, label: &str| -> bool {
                    let tint = if *on {
                        theme.accent
                    } else {
                        egui::Color32::TRANSPARENT
                    };
                    if ui
                        .add(egui::Button::image_and_text(
                            egui::Image::new(icons::check())
                                .fit_to_exact_size(icon::sm_square())
                                .tint(tint),
                            egui::RichText::new(label).size(font::BODY),
                        ))
                        .clicked()
                    {
                        *on = !*on;
                        ui.close();
                        return true;
                    }
                    false
                };
                let mut changed = toggle(ui, &mut prefs.show_floor_grid, "Floor Grid");
                changed |= toggle(ui, &mut prefs.show_origin_axes, "Origin Axes");
                if changed {
                    crate::theme::save_preferences(prefs);
                }
            },
        );
        ui.add_space(space::XS);
        ui.menu_image_text_button(
            egui::Image::new(icons::paint_bucket())
                .fit_to_exact_size(icon::md_square())
                .tint(TEXT),
            egui::RichText::new("Color Format").size(font::BODY),
            |ui| {
                ui.set_min_width(width::TOP_BAR_MENU);
                let mut changed = false;
                for space in ColorSpace::ALL {
                    let on = space == prefs.color_space;
                    let tint = if on {
                        theme.accent
                    } else {
                        egui::Color32::TRANSPARENT
                    };
                    if ui
                        .add(egui::Button::image_and_text(
                            egui::Image::new(icons::check())
                                .fit_to_exact_size(icon::sm_square())
                                .tint(tint),
                            egui::RichText::new(space.label()).size(font::BODY),
                        ))
                        .clicked()
                    {
                        prefs.color_space = space;
                        changed = true;
                        ui.close();
                    }
                }
                if changed {
                    crate::theme::save_preferences(prefs);
                }
            },
        );

        ui.add_space(space::SM);
        widgets::vertical_rule(ui, theme);
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
            history.undo(grid);
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
            history.redo(grid);
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
                    crate::updater::start_check(updater, true);
                }
            }
        });
    });
}
