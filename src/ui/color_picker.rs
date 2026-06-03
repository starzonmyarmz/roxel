//! The space-aware color picker popup body. Pulled out of `ui.rs` so the
//! inspector module stays focused on panel layout; this is the single numeric
//! + 2D edit surface for any sRGB color.
//!
//! `space_color_picker` drives the foreground `CurrentColor`, while
//! `space_color_picker_rgb` is the `[u8; 3]` core that it and the standalone
//! canvas/shot `space_color_swatch`es funnel through — so every editable
//! picker honours `Preferences.color_space`.

use crate::theme::Theme;
use crate::ui::tokens::{gap, space, stroke};
use crate::ui::widgets;
use bevy_egui::egui;
use roxel::color_space::{ColorEditBuffer, ColorSpace};

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
///
/// [`ColorEditBuffer`]: roxel::color_space::ColorEditBuffer
pub fn space_color_picker(
    ui: &mut egui::Ui,
    color: &mut crate::tools::CurrentColor,
    space: ColorSpace,
    color_edit: &mut ColorEditBuffer,
) -> bool {
    let mut rgb = [color.0[0], color.0[1], color.0[2]];
    let changed = space_color_picker_rgb(ui, &mut rgb, space, color_edit);
    if changed {
        color.0 = [rgb[0], rgb[1], rgb[2], 255];
    }
    changed
}

/// Core picker body over a raw sRGB `[u8; 3]` (alpha-agnostic). Both the
/// foreground-color popup and the standalone canvas/shot swatches funnel
/// through here so every editable picker honours `Preferences.color_space`.
/// The `color_edit` buffer must persist across frames (resource for the
/// inspector, egui memory for the standalone swatches — see
/// [`space_color_picker_temp`]).
pub fn space_color_picker_rgb(
    ui: &mut egui::Ui,
    rgb_out: &mut [u8; 3],
    space: ColorSpace,
    color_edit: &mut ColorEditBuffer,
) -> bool {
    use roxel::color_space::{hsb_to_rgb, rgb_to_hsb};

    let mut color: [u8; 4] = [rgb_out[0], rgb_out[1], rgb_out[2], 255];

    // Cache: last rgba we produced + working hue (0..1). Per-widget id so a
    // palette swatch popup can't clobber the inspector's working state.
    type State = ([u8; 4], f32);
    let cache_id = ui.id().with("space_color_picker_state");
    let cached: Option<State> = ui.data(|d| d.get_temp(cache_id));

    let rgb3 = [color[0], color[1], color[2]];
    let mut hue_norm = match cached {
        Some((rgba, h)) if rgba == color => h,
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
    if color_edit.source != color || color_edit.space != space {
        color_edit.populate(color, space);
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
                        let step = roxel::color_space::ColorEditBuffer::field_step(space, i, shift);
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
    if stepped && let Some(rgb) = color_edit.commit() {
        color = [rgb[0], rgb[1], rgb[2], 255];
        // Keep source in sync so the gate above doesn't repopulate and
        // clobber the stepped buffer with a quantised readback.
        color_edit.source = color;
        let (h, _, _) = rgb_to_hsb([color[0], color[1], color[2]]);
        hue_norm = h / 360.0;
        changed = true;
    }
    if commit_now {
        if let Some(rgb) = color_edit.commit() {
            color = [rgb[0], rgb[1], rgb[2], 255];
            let (h, _, _) = rgb_to_hsb([color[0], color[1], color[2]]);
            hue_norm = h / 360.0;
            changed = true;
        }
        color_edit.populate(color, space);
    }

    ui.add_space(space::XS);

    // Live (S, V) read from current color (in 0..1).
    let (_h_live, s_live, v_live) = rgb_to_hsb([color[0], color[1], color[2]]);
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
        color = [rgb[0], rgb[1], rgb[2], 255];
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
        color = [rgb[0], rgb[1], rgb[2], 255];
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

    ui.data_mut(|d| d.insert_temp(cache_id, (color, hue_norm)));
    *rgb_out = [color[0], color[1], color[2]];
    changed
}

/// [`space_color_picker_rgb`] with its [`ColorEditBuffer`] parked in egui
/// temp memory (keyed by the parent `ui` id) instead of an ECS resource. Used
/// by the standalone canvas-background / shot-background swatches, which have
/// no dedicated buffer resource of their own.
pub fn space_color_picker_temp(ui: &mut egui::Ui, rgb: &mut [u8; 3], space: ColorSpace) -> bool {
    let buf_id = ui.id().with("space_picker_buf");
    let mut buf: ColorEditBuffer = ui.data(|d| d.get_temp(buf_id)).unwrap_or_default();
    let changed = space_color_picker_rgb(ui, rgb, space, &mut buf);
    ui.data_mut(|d| d.insert_temp(buf_id, buf));
    changed
}

/// A swatch button that opens the space-aware picker popup over a raw sRGB
/// `[u8; 3]`. Mirrors the inspector hero-swatch affordance so the canvas and
/// shot background pickers honour `Preferences.color_space` like the
/// foreground picker does. Returns `true` on any edit this frame.
pub fn space_color_swatch(
    ui: &mut egui::Ui,
    theme: &Theme,
    rgb: &mut [u8; 3],
    space: ColorSpace,
    size: egui::Vec2,
    corner_radius: u8,
) -> bool {
    let srgba = egui::Color32::from_rgb(rgb[0], rgb[1], rgb[2]);
    // Zero button padding + interact-size min so the swatch renders at exactly
    // `size` (egui otherwise inflates an empty Button by its default padding
    // and min interact-size). The inspector's grids do this via `swatch_grid`;
    // standalone callers must scope it themselves.
    let resp = ui
        .scope(|ui| {
            ui.spacing_mut().button_padding = crate::ui::tokens::pad::NONE;
            ui.spacing_mut().interact_size = crate::ui::tokens::gap::NONE;
            widgets::swatch_button(ui, theme, srgba, size, corner_radius, false)
        })
        .inner
        .on_hover_text("Click to edit color");
    let mut changed = false;
    egui::Popup::menu(&resp)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .show(|ui| {
            changed = space_color_picker_temp(ui, rgb, space);
        });
    changed
}
