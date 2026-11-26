//! Free-floating UI surfaces drawn over the canvas: the right-edge tool
//! island and (on Win/Linux) the top-center menu pill. Both follow the toast
//! pattern — a shadowed `egui::Area` styled with a pill-shaped
//! `Frame::popup` and anchored by pivot against `crate::ui::canvas_rect(ctx)` so
//! they nest cleanly beside any registered side panel.

use crate::actions::{Action, ActionQueue, Origin};
use crate::plugins::RailButton;
use crate::theme::{Preferences, Theme};
use crate::tools::{ShapeOptions, Tool, ToolState};
use crate::ui::tokens::{font, gap, icon, pad, radius, shadow, size, space, stroke, swatch};
use crate::ui::{icons, widgets};
use bevy_egui::egui;
use roxel::shapes::ShapePrimitive;

// The menu pill is Win/Linux only (macOS uses the native menu), so its
// contents helper and the deps it pulls in are gated to match.
#[cfg(not(target_os = "macos"))]
use crate::actions::ActionState;
#[cfg(not(target_os = "macos"))]
use crate::menu::{MenuEntry, NO_PLUGINS_LABEL, menu_bar, recent_item_label};
#[cfg(not(target_os = "macos"))]
use crate::ui::tokens::width;

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
    id: impl std::hash::Hash + std::fmt::Debug,
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
    let canvas = crate::ui::canvas_rect(ctx);
    let cy = (canvas.min.y + canvas.max.y) * 0.5;
    egui::pos2(canvas.max.x - space::FLOAT_GAP, cy)
}

/// Top-center anchor for the Win/Linux floating menu pill.
#[cfg(not(target_os = "macos"))]
pub fn pill_menu_anchor(ctx: &egui::Context) -> egui::Pos2 {
    let canvas = crate::ui::canvas_rect(ctx);
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
    widgets::tool_button(ui, theme, tool, Tool::Brush, icons::tool(Tool::Brush));
    ui.add_space(space::SX);
    widgets::tool_button(ui, theme, tool, Tool::Erase, icons::tool(Tool::Erase));
    ui.add_space(space::SX);
    widgets::tool_button(ui, theme, tool, Tool::Paint, icons::tool(Tool::Paint));
    ui.add_space(space::SX);
    widgets::tool_button(
        ui,
        theme,
        tool,
        Tool::Eyedropper,
        icons::tool(Tool::Eyedropper),
    );
    ui.add_space(space::SX);
    let shape_resp = widgets::tool_button(
        ui,
        theme,
        tool,
        Tool::Shape,
        icons::shape_primitive(shape_options.primitive),
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
                        for prim in [
                            ShapePrimitive::Rectangle,
                            ShapePrimitive::Ellipse,
                            ShapePrimitive::Line,
                            ShapePrimitive::Sphere,
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
                                theme.on_accent
                            } else {
                                theme.text
                            };
                            egui::Image::new(icons::shape_primitive(prim))
                                .fit_to_exact_size(icon_size)
                                .tint(tint)
                                .paint_at(ui, icon_rect);
                            let r = r.on_hover_text(
                                crate::actions::Action::SelectShape(prim).spec().menu_label,
                            );
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
    widgets::tool_button(ui, theme, tool, Tool::Select, icons::tool(Tool::Select));
    ui.add_space(space::SX);
    widgets::tool_button(ui, theme, tool, Tool::Move, icons::tool(Tool::Move));
}

/// Plugin buttons shown on the rail at most, counting the overflow button.
const MAX_PLUGIN_SLOTS: usize = 5;

/// How many of `total` plugin buttons the rail shows when `room` button
/// slots fit, and whether the rest go behind an overflow button, which takes
/// the last slot. With no room at all, none show and there's no overflow
/// button either.
pub fn plugin_slots(total: usize, room: usize) -> (usize, bool) {
    let room = room.min(MAX_PLUGIN_SLOTS);
    if room == 0 {
        (0, false)
    } else if total <= room {
        (total, false)
    } else {
        (room.saturating_sub(1), true)
    }
}

/// Plugin toolbar buttons under a separator after the built-in tools. Each
/// queues the same [`Action::RunPlugin`] as its Plugins menu item.
pub fn plugin_rail(
    ui: &mut egui::Ui,
    theme: &Theme,
    buttons: &[RailButton],
    can_run: bool,
    actions: &mut ActionQueue,
) {
    if buttons.is_empty() {
        return;
    }
    let slot = size::TOOL_BUTTON.y + space::SX;
    let separator = space::SX + stroke::NORMAL;
    let chrome = 2.0 * (space::FLOAT_GAP + pad::DEFAULT.x);
    let free = crate::ui::canvas_rect(ui.ctx()).height() - chrome - ui.min_rect().height();
    let room = ((free - separator) / slot).floor().max(0.0) as usize;
    let (shown, overflow) = plugin_slots(buttons.len(), room);
    if shown == 0 && !overflow {
        return;
    }

    widgets::rail_separator(ui, theme);
    ui.add_enabled_ui(can_run, |ui| {
        for b in &buttons[..shown] {
            ui.add_space(space::SX);
            let resp = widgets::rail_button(ui, theme, b.icon.source(), false)
                .on_hover_text(&b.tooltip)
                .on_disabled_hover_text(&b.tooltip);
            if resp.clicked() {
                actions.push(Action::RunPlugin(b.command), Origin::Ui);
            }
        }
        if !overflow {
            return;
        }
        ui.add_space(space::SX);
        let more = widgets::rail_button(ui, theme, icons::ellipsis(), false)
            .on_hover_text("More plugin commands");
        egui::Popup::menu(&more)
            .align(egui::RectAlign::LEFT_END)
            .close_behavior(egui::PopupCloseBehavior::CloseOnClick)
            .show(|ui| {
                ui.spacing_mut().button_padding = pad::MENU;
                for b in &buttons[shown..] {
                    let image = egui::Image::new(b.icon.source())
                        .fit_to_exact_size(icon::md_square())
                        .tint(theme.text);
                    let label = egui::RichText::new(&b.tooltip).size(font::BODY);
                    if ui.add(egui::Button::image_and_text(image, label)).clicked() {
                        actions.push(Action::RunPlugin(b.command), Origin::Ui);
                    }
                }
            });
    });
}

#[cfg(not(target_os = "macos"))]
fn menu_item(
    ui: &mut egui::Ui,
    theme: &Theme,
    s: &ActionState,
    out: &mut ActionQueue,
    action: Action,
    label: &str,
) {
    let enabled = action.enabled(s);
    let text = egui::RichText::new(label).size(font::BODY);
    let button = if action.is_check() {
        let tint = if action.checked(s) {
            theme.accent
        } else {
            egui::Color32::TRANSPARENT
        };
        egui::Button::image_and_text(
            egui::Image::new(icons::check())
                .fit_to_exact_size(icon::sm_square())
                .tint(tint),
            text,
        )
    } else {
        egui::Button::new(text)
    };
    let button = match action.shortcut_text() {
        Some(sc) => button.shortcut_text(egui::RichText::new(sc).size(font::SMALL)),
        None => button,
    };
    if ui.add_enabled(enabled, button).clicked() {
        out.push(action, Origin::Ui);
        ui.close();
    }
}

/// Render `entries` into an open egui menu.
#[cfg(not(target_os = "macos"))]
fn menu_entries(
    ui: &mut egui::Ui,
    theme: &Theme,
    entries: &[MenuEntry],
    s: &ActionState,
    out: &mut ActionQueue,
) {
    let title = |label: &str| egui::RichText::new(label).size(font::BODY);
    for entry in entries {
        match entry {
            MenuEntry::Item(action) => {
                menu_item(ui, theme, s, out, *action, &action.menu_label(s));
            }
            MenuEntry::Separator => {
                ui.separator();
            }
            MenuEntry::Submenu(label, inner) => {
                ui.menu_button(title(label), |ui| {
                    ui.set_min_width(width::TOP_BAR_MENU);
                    menu_entries(ui, theme, inner, s, out);
                });
            }
            MenuEntry::OpenRecent => {
                ui.add_enabled_ui(!s.recent.is_empty(), |ui| {
                    ui.menu_button(title("Open Recent"), |ui| {
                        ui.set_min_width(width::TOP_BAR_MENU);
                        for (i, path) in s.recent.iter().enumerate() {
                            let label = recent_item_label(i, path);
                            menu_item(ui, theme, s, out, Action::OpenRecent(i), &label);
                        }
                        ui.separator();
                        let clear = Action::ClearRecent;
                        menu_item(ui, theme, s, out, clear, clear.spec().menu_label);
                    });
                });
            }
            MenuEntry::PluginList => plugin_list(ui, theme, s, out),
            MenuEntry::System(_) => {}
        }
    }
}

#[cfg(not(target_os = "macos"))]
fn plugin_list(ui: &mut egui::Ui, theme: &Theme, s: &ActionState, out: &mut ActionQueue) {
    if s.plugins.commands.is_empty() {
        ui.add_enabled(
            false,
            egui::Button::new(egui::RichText::new(NO_PLUGINS_LABEL).size(font::BODY)),
        );
        return;
    }
    for p in s.plugins.listed() {
        let title = egui::RichText::new(&s.plugins.list[p].manifest.name).size(font::BODY);
        ui.menu_button(title, |ui| {
            ui.set_min_width(width::TOP_BAR_MENU);
            for i in s.plugins.commands_of(p) {
                let action = Action::RunPlugin(i);
                menu_item(ui, theme, s, out, action, &action.menu_label(s));
            }
        });
    }
}

/// The menu bar as File / Edit / View / Help dropdowns plus quick Undo/Redo
/// and the update badge. Chosen items are pushed onto `out`.
#[cfg(not(target_os = "macos"))]
pub fn pill_menu_contents(
    ui: &mut egui::Ui,
    theme: &Theme,
    s: &ActionState,
    out: &mut ActionQueue,
) {
    for (label, entries) in menu_bar() {
        ui.menu_button(egui::RichText::new(label).size(font::BODY), |ui| {
            ui.set_min_width(width::TOP_BAR_MENU);
            menu_entries(ui, theme, &entries, s, out);
        });
    }

    ui.add_space(space::SM);
    widgets::vertical_rule(ui, theme);
    ui.add_space(space::XS);

    for (action, icon_src) in [(Action::Undo, icons::undo()), (Action::Redo, icons::redo())] {
        let spec = action.spec();
        let tip = match action.shortcut_text() {
            Some(sc) => format!("{} ({sc})", spec.menu_label),
            None => spec.menu_label.to_string(),
        };
        if widgets::icon_only_button(ui, theme, icon_src, action.enabled(s))
            .on_hover_text(tip)
            .clicked()
        {
            out.push(action, Origin::Ui);
        }
    }

    if let Some(tag) = s.update_tag {
        ui.add_space(space::SM);
        let resp = ui.add(egui::Button::image_and_text(
            egui::Image::new(icons::arrow_up())
                .fit_to_exact_size(icon::md_square())
                .tint(theme.accent),
            egui::RichText::new(format!("Update {tag} available"))
                .size(font::BODY)
                .color(theme.accent),
        ));
        if resp.on_hover_text("Open the release page").clicked() {
            out.push(Action::CheckForUpdates, Origin::Ui);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plugin_buttons_that_fit_all_show() {
        assert_eq!(plugin_slots(0, 4), (0, false));
        assert_eq!(plugin_slots(3, 4), (3, false));
        assert_eq!(plugin_slots(4, 4), (4, false));
    }

    #[test]
    fn the_rest_go_behind_an_overflow_button_in_the_last_slot() {
        assert_eq!(plugin_slots(5, 4), (3, true));
        assert_eq!(plugin_slots(2, 1), (0, true));
        assert_eq!(plugin_slots(2, 0), (0, false));
    }

    #[test]
    fn a_tall_rail_still_caps_plugin_buttons() {
        assert_eq!(
            plugin_slots(MAX_PLUGIN_SLOTS, 50),
            (MAX_PLUGIN_SLOTS, false)
        );
        assert_eq!(plugin_slots(20, 50), (MAX_PLUGIN_SLOTS - 1, true));
    }
}
