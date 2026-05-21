//! Free-floating UI surfaces drawn over the canvas: the right-edge tool
//! island and (on Win/Linux) the top-center menu pill. Both follow the toast
//! pattern — a shadowed `egui::Area` styled with a pill-shaped
//! `Frame::popup` and anchored by pivot against `ctx.available_rect()` so
//! they nest cleanly beside any registered side panel.

use crate::theme::Theme;
use crate::ui::tokens::{pad, radius, space};
use bevy_egui::egui;

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
        .shadow(egui::epaint::Shadow {
            offset: [0, 4],
            blur: 12,
            spread: 0,
            color: egui::Color32::from_black_alpha(60),
        })
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
