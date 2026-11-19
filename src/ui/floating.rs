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
use crate::camera::CameraPreset;
#[cfg(not(target_os = "macos"))]
use crate::menu::{MenuAction, recent_item_label};
#[cfg(not(target_os = "macos"))]
use crate::ui::tokens::{font, icon, width};
#[cfg(not(target_os = "macos"))]
use roxel::color_space::ColorSpace;
#[cfg(not(target_os = "macos"))]
use std::path::PathBuf;

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
fn item(
    ui: &mut egui::Ui,
    out: &mut Vec<MenuAction>,
    label: &str,
    shortcut: &str,
    enabled: bool,
    action: MenuAction,
) {
    let mut button = egui::Button::new(egui::RichText::new(label).size(font::BODY));
    if !shortcut.is_empty() {
        button = button.shortcut_text(egui::RichText::new(shortcut).size(font::SMALL));
    }
    if ui.add_enabled(enabled, button).clicked() {
        out.push(action);
        ui.close();
    }
}

/// What the menu pill needs to know to enable and check its items.
#[cfg(not(target_os = "macos"))]
pub struct PillMenuState<'a> {
    pub prefs: &'a Preferences,
    pub recent: &'a [PathBuf],
    pub dialog_busy: bool,
    pub has_undo: bool,
    pub has_redo: bool,
    pub has_selection: bool,
    pub selection_idle: bool,
    pub has_clipboard: bool,
    pub has_voxels: bool,
    pub update: Option<&'a crate::updater::Release>,
    pub checking_updates: bool,
}

/// File / Edit / View / Help menus plus quick Undo/Redo, mirroring the macOS
/// menu bar. Chosen items are pushed onto `out` for
/// `apply_menu_actions_system`.
#[cfg(not(target_os = "macos"))]
pub fn pill_menu_contents(
    ui: &mut egui::Ui,
    theme: &Theme,
    s: &PillMenuState,
    out: &mut Vec<MenuAction>,
) {
    let check = |ui: &mut egui::Ui, on: bool, label: &str| -> bool {
        let tint = if on {
            theme.accent
        } else {
            egui::Color32::TRANSPARENT
        };
        let clicked = ui
            .add(egui::Button::image_and_text(
                egui::Image::new(icons::check())
                    .fit_to_exact_size(icon::sm_square())
                    .tint(tint),
                egui::RichText::new(label).size(font::BODY),
            ))
            .clicked();
        if clicked {
            ui.close();
        }
        clicked
    };
    let title = |label: &str| egui::RichText::new(label).size(font::BODY);
    let idle = !s.dialog_busy;

    ui.menu_button(title("File"), |ui| {
        ui.set_min_width(width::TOP_BAR_MENU);
        item(ui, out, "New…", "Ctrl+N", true, MenuAction::NewProject);
        item(ui, out, "Open…", "Ctrl+O", idle, MenuAction::OpenProject);
        ui.add_enabled_ui(!s.recent.is_empty(), |ui| {
            ui.menu_button(title("Open Recent"), |ui| {
                ui.set_min_width(width::TOP_BAR_MENU);
                for (i, path) in s.recent.iter().enumerate() {
                    item(
                        ui,
                        out,
                        &recent_item_label(i, path),
                        "",
                        true,
                        MenuAction::OpenRecent(i),
                    );
                }
                ui.separator();
                item(ui, out, "Clear Menu", "", true, MenuAction::ClearRecent);
            });
        });
        item(ui, out, "Save", "Ctrl+S", idle, MenuAction::SaveProject);
        item(
            ui,
            out,
            "Save As…",
            "Ctrl+Shift+S",
            idle,
            MenuAction::SaveProjectAs,
        );
        ui.separator();
        ui.menu_button(title("Import"), |ui| {
            ui.set_min_width(width::TOP_BAR_MENU);
            item(
                ui,
                out,
                "MagicaVoxel (.vox)…",
                "",
                idle,
                MenuAction::ImportVox,
            );
            item(ui, out, "Qubicle (.qb)…", "", idle, MenuAction::ImportQb);
            item(ui, out, "Goxel (.gox)…", "", idle, MenuAction::ImportGox);
        });
        ui.menu_button(title("Export"), |ui| {
            ui.set_min_width(width::TOP_BAR_MENU);
            item(
                ui,
                out,
                "MagicaVoxel (.vox)…",
                "",
                idle,
                MenuAction::ExportVox,
            );
            item(ui, out, "Goxel (.gox)…", "", idle, MenuAction::ExportGox);
            item(
                ui,
                out,
                "Wavefront (.obj)…",
                "",
                idle,
                MenuAction::ExportObj,
            );
            item(ui, out, "glTF (.glb)…", "", idle, MenuAction::ExportGltf);
            item(ui, out, "Transparent PNG…", "", idle, MenuAction::ExportPng);
            item(
                ui,
                out,
                "Roxel Shot (PNG)…",
                "",
                s.has_voxels,
                MenuAction::ExportShot,
            );
            item(ui, out, "SVG…", "", idle, MenuAction::ExportSvg);
        });
        ui.separator();
        item(
            ui,
            out,
            "Command Palette…",
            "Ctrl+K",
            true,
            MenuAction::ShowCommandPalette,
        );
        item(
            ui,
            out,
            "Preferences…",
            "Ctrl+,",
            true,
            MenuAction::Preferences,
        );
        ui.separator();
        let quit = if cfg!(windows) { "Exit" } else { "Quit" };
        item(ui, out, quit, "", true, MenuAction::Quit);
    });

    ui.menu_button(title("Edit"), |ui| {
        ui.set_min_width(width::TOP_BAR_MENU);
        item(ui, out, "Undo", "Ctrl+Z", s.has_undo, MenuAction::Undo);
        item(
            ui,
            out,
            "Redo",
            "Ctrl+Shift+Z",
            s.has_redo,
            MenuAction::Redo,
        );
        ui.separator();
        let sel = s.has_selection;
        item(
            ui,
            out,
            "Fill Selection",
            "F",
            sel,
            MenuAction::FillSelection,
        );
        item(
            ui,
            out,
            "Delete Selection Contents",
            "Backspace",
            sel && s.selection_idle,
            MenuAction::DeleteSelectionContents,
        );
        item(
            ui,
            out,
            "Clear Selection",
            "",
            sel,
            MenuAction::ClearSelection,
        );
        ui.separator();
        item(ui, out, "Cut", "Ctrl+X", sel, MenuAction::Cut);
        item(ui, out, "Copy", "Ctrl+C", sel, MenuAction::Copy);
        item(
            ui,
            out,
            "Paste",
            "Ctrl+V",
            s.has_clipboard,
            MenuAction::Paste,
        );
        ui.separator();
        item(
            ui,
            out,
            "Double Density",
            "",
            s.has_voxels,
            MenuAction::DoubleDensity,
        );
        item(
            ui,
            out,
            "Halve Density",
            "",
            s.has_voxels,
            MenuAction::HalveDensity,
        );
    });

    ui.menu_button(title("View"), |ui| {
        ui.set_min_width(width::TOP_BAR_MENU);
        ui.menu_button(title("Camera"), |ui| {
            ui.set_min_width(width::TOP_BAR_MENU);
            item(ui, out, "Frame View", "Ctrl+0", true, MenuAction::FrameView);
            ui.separator();
            for (preset, shortcut) in [
                (CameraPreset::Front, "Ctrl+1"),
                (CameraPreset::Back, "Ctrl+Shift+1"),
                (CameraPreset::Right, "Ctrl+3"),
                (CameraPreset::Left, "Ctrl+Shift+3"),
                (CameraPreset::Top, "Ctrl+7"),
            ] {
                item(
                    ui,
                    out,
                    preset.label(),
                    shortcut,
                    true,
                    MenuAction::ViewPreset(preset),
                );
            }
            ui.separator();
            item(
                ui,
                out,
                CameraPreset::Iso.label(),
                "Ctrl+5",
                true,
                MenuAction::ViewPreset(CameraPreset::Iso),
            );
        });
        ui.separator();
        if check(ui, s.prefs.show_floor_grid, "Floor Grid") {
            out.push(MenuAction::ToggleFloorGrid);
        }
        if check(ui, s.prefs.show_origin_axes, "Origin Axes") {
            out.push(MenuAction::ToggleOriginAxes);
        }
        ui.separator();
        ui.menu_button(title("Color Format"), |ui| {
            ui.set_min_width(width::TOP_BAR_MENU);
            for space in ColorSpace::ALL {
                if check(ui, space == s.prefs.color_space, space.label()) {
                    out.push(MenuAction::SetColorSpace(space));
                }
            }
        });
    });

    ui.menu_button(title("Help"), |ui| {
        ui.set_min_width(width::TOP_BAR_MENU);
        item(
            ui,
            out,
            "Show Onboarding Tour…",
            "",
            true,
            MenuAction::ShowOnboarding,
        );
        item(ui, out, "Changelog", "", true, MenuAction::Changelog);
        ui.separator();
        item(
            ui,
            out,
            "Check for Updates…",
            "",
            !s.checking_updates,
            MenuAction::CheckForUpdates,
        );
        item(ui, out, "About Roxel", "", true, MenuAction::About);
    });

    ui.add_space(space::SM);
    widgets::vertical_rule(ui, theme);
    ui.add_space(space::XS);

    if widgets::icon_only_button(ui, theme, icons::undo(), s.has_undo)
        .on_hover_text("Undo (Ctrl+Z)")
        .clicked()
    {
        out.push(MenuAction::Undo);
    }
    if widgets::icon_only_button(ui, theme, icons::redo(), s.has_redo)
        .on_hover_text("Redo (Ctrl+Shift+Z)")
        .clicked()
    {
        out.push(MenuAction::Redo);
    }

    if let Some(rel) = s.update {
        ui.add_space(space::SM);
        let resp = ui.add(egui::Button::image_and_text(
            egui::Image::new(icons::arrow_up())
                .fit_to_exact_size(icon::md_square())
                .tint(theme.accent),
            egui::RichText::new(format!("Update {} available", rel.tag))
                .size(font::BODY)
                .color(theme.accent),
        ));
        if resp.on_hover_text("Open the release page").clicked() {
            crate::updater::open_url(&rel.html_url);
        }
    }
}
