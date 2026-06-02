//! The foreground-color picker popup body. Pulled out of `ui.rs` so the
//! inspector module stays focused on panel layout; this is the single numeric
//! + 2D edit surface for `CurrentColor`.

use crate::ui::tokens::{gap, space, stroke};
use bevy_egui::egui;
use roxel::color_space::ColorSpace;

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
    color_edit: &mut roxel::color_space::ColorEditBuffer,
) -> bool {
    use roxel::color_space::{hsb_to_rgb, rgb_to_hsb};

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
        color.0 = [rgb[0], rgb[1], rgb[2], 255];
        // Keep source in sync so the gate above doesn't repopulate and
        // clobber the stepped buffer with a quantised readback.
        color_edit.source = color.0;
        let (h, _, _) = rgb_to_hsb([color.0[0], color.0[1], color.0[2]]);
        hue_norm = h / 360.0;
        changed = true;
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
