use super::icons;
use crate::theme::{INTER_SEMIBOLD_FAMILY, Theme};
use crate::tools::{Tool, ToolState};
use crate::ui::tokens::{font, gap, icon, pad, radius, size, space, stroke};
use bevy_egui::egui;

#[cfg_attr(target_os = "macos", allow(dead_code))]
pub fn vertical_rule(ui: &mut egui::Ui, theme: &Theme) {
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(stroke::NORMAL, size::RULE_HEIGHT),
        egui::Sense::hover(),
    );
    ui.painter().vline(
        rect.center().x,
        rect.y_range(),
        egui::Stroke::new(stroke::HAIR, theme.border),
    );
}

#[allow(dead_code)] // referenced only by tests; live tool hint now lives in tooltips
pub fn tool_hint(t: Tool) -> &'static str {
    match t {
        Tool::Brush => "Click or drag to add voxels. Shift+click for line.",
        Tool::Erase => "Click or drag to remove voxels. Shift+click for line.",
        Tool::Paint => "Click or drag to recolor existing voxels.",
        Tool::Eyedropper => "Click a voxel to pick its color. Hold Alt to stay.",
        Tool::Shape => "Drag for footprint, drag normal for depth, click commits. Esc cancels.",
        Tool::Select => "Drag on a face to select a region.",
        Tool::Move => "Drag selection to slide. Shift locks Y. Arrows nudge by 1.",
    }
}

pub fn tool_button(
    ui: &mut egui::Ui,
    theme: &Theme,
    tool: &mut ToolState,
    kind: Tool,
    icon_src: egui::ImageSource<'static>,
    label: &str,
    shortcut: &str,
) -> egui::Response {
    let active = tool.current == kind;
    let button_size = egui::vec2(40.0, 40.0);

    // Static three-state palette — no animation. The animated version stuttered
    // on hover (the visible fill alternated between two greys mid-hover), so
    // motion is reverted in favour of a snappy, predictable hover.
    let sh = theme.surface_hover;
    let bg = theme.bg;
    let blend_u8 = |a: u8, b: u8| (((a as u16) * 3 + b as u16) / 4) as u8;
    let neutral_hover = egui::Color32::from_rgb(
        blend_u8(bg.r(), sh.r()),
        blend_u8(bg.g(), sh.g()),
        blend_u8(bg.b(), sh.b()),
    );

    // Active tool fills with the full accent — coral wash on the icon was
    // too subtle for a state this important. White icon over accent reads
    // unambiguously selected; brightened accent on hover keeps the active
    // state distinct from a hover preview.
    let a = theme.accent;
    let accent_hover = egui::Color32::from_rgb(
        a.r().saturating_add(12),
        a.g().saturating_add(12),
        a.b().saturating_add(12),
    );

    let (inactive_fill, hovered_fill, icon_tint) = if active {
        (theme.accent, accent_hover, egui::Color32::WHITE)
    } else {
        (egui::Color32::TRANSPARENT, neutral_hover, theme.text)
    };

    let icon_img = egui::Image::new(icon_src)
        .fit_to_exact_size(icon::lg_square())
        .tint(icon_tint);

    let resp = ui
        .scope(|ui| {
            ui.spacing_mut().button_padding = pad::NONE;
            ui.spacing_mut().interact_size = gap::NONE;
            let w = ui.visuals_mut();
            w.widgets.inactive.bg_fill = inactive_fill;
            w.widgets.inactive.weak_bg_fill = inactive_fill;
            w.widgets.inactive.bg_stroke = egui::Stroke::NONE;
            w.widgets.inactive.expansion = 0.0;
            w.widgets.hovered.bg_fill = hovered_fill;
            w.widgets.hovered.weak_bg_fill = hovered_fill;
            w.widgets.hovered.bg_stroke = egui::Stroke::NONE;
            w.widgets.hovered.expansion = 0.0;
            w.widgets.active.bg_fill = hovered_fill;
            w.widgets.active.weak_bg_fill = hovered_fill;
            w.widgets.active.bg_stroke = egui::Stroke::NONE;
            w.widgets.active.expansion = 0.0;
            ui.add_sized(
                button_size,
                egui::Button::image(icon_img)
                    .stroke(egui::Stroke::NONE)
                    .corner_radius(egui::CornerRadius::same(radius::INSIDE_PILL)),
            )
        })
        .inner;

    if resp.clicked() && tool.current != kind {
        tool.previous = tool.current;
        tool.current = kind;
    }
    resp.on_hover_text(format!("{label}  ({shortcut})"))
}

#[cfg_attr(target_os = "macos", allow(dead_code))]
pub fn icon_button(
    ui: &mut egui::Ui,
    theme: &Theme,
    icon: egui::ImageSource<'static>,
    label: &str,
) -> egui::Response {
    ui.add(egui::Button::image_and_text(
        egui::Image::new(icon)
            .fit_to_exact_size(icon::md_square())
            .tint(theme.text),
        egui::RichText::new(label).size(font::BODY),
    ))
}

pub fn icon_only_button(
    ui: &mut egui::Ui,
    theme: &Theme,
    icon_src: egui::ImageSource<'static>,
    enabled: bool,
) -> egui::Response {
    let tint = if enabled { theme.text } else { theme.text_dim };
    let img = egui::Image::new(icon_src)
        .fit_to_exact_size(icon::md_square())
        .tint(tint);
    let sh = theme.surface_hover;
    let bg = theme.bg;
    let blend = |a: u8, b: u8| (((a as u16) * 3 + b as u16) / 4) as u8;
    let hover_fill = egui::Color32::from_rgb(
        blend(bg.r(), sh.r()),
        blend(bg.g(), sh.g()),
        blend(bg.b(), sh.b()),
    );
    ui.scope(|ui| {
        ui.spacing_mut().button_padding = pad::NONE;
        ui.spacing_mut().interact_size = gap::NONE;
        let w = ui.visuals_mut();
        w.widgets.inactive.bg_fill = egui::Color32::TRANSPARENT;
        w.widgets.inactive.weak_bg_fill = egui::Color32::TRANSPARENT;
        w.widgets.inactive.bg_stroke = egui::Stroke::NONE;
        w.widgets.inactive.expansion = 0.0;
        w.widgets.hovered.bg_fill = hover_fill;
        w.widgets.hovered.weak_bg_fill = hover_fill;
        w.widgets.hovered.bg_stroke = egui::Stroke::NONE;
        w.widgets.hovered.expansion = 0.0;
        w.widgets.active.bg_fill = hover_fill;
        w.widgets.active.weak_bg_fill = hover_fill;
        w.widgets.active.bg_stroke = egui::Stroke::NONE;
        w.widgets.active.expansion = 0.0;
        ui.add_enabled(
            enabled,
            egui::Button::image(img)
                .min_size(size::ICON_BUTTON)
                .corner_radius(egui::CornerRadius::same(radius::SM))
                .stroke(egui::Stroke::NONE),
        )
    })
    .inner
}

pub fn section<R>(
    ui: &mut egui::Ui,
    theme: &Theme,
    title: &str,
    add: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    section_header(ui, theme, title);
    ui.add_space(space::SM);
    let result = add(ui);

    ui.add_space(space::MD);
    let sep_rect = ui
        .allocate_exact_size(egui::vec2(ui.available_width(), 1.0), egui::Sense::hover())
        .0;
    let painter = ui.painter();
    painter.hline(
        ui.clip_rect().x_range(),
        painter.round_to_pixel_center(sep_rect.center().y),
        egui::Stroke::new(stroke::HAIR, theme.border),
    );
    ui.add_space(space::MD);
    result
}

/// Label row painted by [`section`]. Uppercase SemiBold label, no chevron,
/// no click target — sections are always expanded.
fn section_header(ui: &mut egui::Ui, theme: &Theme, title: &str) {
    let row_h = font::SECTION + 6.0;
    let (rect, _resp) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), row_h),
        egui::Sense::hover(),
    );
    ui.painter().text(
        rect.left_center(),
        egui::Align2::LEFT_CENTER,
        section_header_text(title),
        egui::FontId::new(
            font::SECTION,
            egui::FontFamily::Name(INTER_SEMIBOLD_FAMILY.into()),
        ),
        theme.text_muted,
    );
}

/// Title-cased section label. No tracking, no uppercase — SemiBold weight
/// carries hierarchy. Each whitespace-separated word's first letter is
/// capitalised; the rest of the input is lowercased so a caller passing
/// "PALETTE" or "palette" both render as "Palette".
pub fn section_header_text(title: &str) -> String {
    title
        .split_whitespace()
        .map(|w| {
            let mut chars = w.chars();
            match chars.next() {
                Some(first) => {
                    first.to_uppercase().collect::<String>() + &chars.as_str().to_lowercase()
                }
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn stat_row(ui: &mut egui::Ui, theme: &Theme, label: &str, value: String) {
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(label)
                .color(theme.text_dim)
                .size(font::SMALL),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add(
                egui::Label::new(
                    egui::RichText::new(value)
                        .monospace()
                        .color(theme.text)
                        .size(font::SMALL),
                )
                .selectable(true),
            );
        });
    });
}

/// Dim italic small body text used for inline help under sections. Wraps.
pub fn hint_label(ui: &mut egui::Ui, theme: &Theme, text: &str) {
    ui.add(
        egui::Label::new(
            egui::RichText::new(text)
                .color(theme.text_dim)
                .size(font::SMALL),
        )
        .wrap(),
    );
}

/// Dim readout label, non-selectable. Used in former status bar; preserved
/// for reuse in future read-only readouts.
#[allow(dead_code)] // reserved
pub fn status_label(ui: &mut egui::Ui, theme: &Theme, text: &str) {
    ui.add(
        egui::Label::new(
            egui::RichText::new(text)
                .color(theme.text_dim)
                .size(font::SMALL),
        )
        .selectable(false),
    );
}

/// `#RRGGBB` uppercase. Single source of truth for hex codes shown in the UI
/// (foreground swatch readout, palette/recent hover tips, custom-colour pref
/// rows).
pub fn hex_string(rgb: [u8; 3]) -> String {
    format!("#{:02X}{:02X}{:02X}", rgb[0], rgb[1], rgb[2])
}

/// Selectable monospace hex label. `dim = true` uses `theme.text_dim` +
/// SMALL (settings-row style); `dim = false` uses `theme.text` + BODY
/// (inspector readout style).
pub fn hex_label(ui: &mut egui::Ui, theme: &Theme, rgb: [u8; 3], dim: bool) {
    let (colour, size) = if dim {
        (theme.text_dim, font::SMALL)
    } else {
        (theme.text, font::BODY)
    };
    ui.add(
        egui::Label::new(
            egui::RichText::new(hex_string(rgb))
                .monospace()
                .color(colour)
                .size(size),
        )
        .selectable(true),
    );
}

/// How a swatch participates in the active color pool. `Primary` is the
/// single `CurrentColor`; `Extra` is a shift-selected additional sample
/// color (renders the same accent ring but at the unselected stroke width so
/// the primary still dominates).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SwatchSelect {
    None,
    Primary,
    Extra,
}

impl From<bool> for SwatchSelect {
    fn from(selected: bool) -> Self {
        if selected { Self::Primary } else { Self::None }
    }
}

/// Single colour-square button. Primary swatches get a full-weight accent
/// stroke; extras (shift-selected pool members) get the same accent color at
/// the unselected stroke width so the primary stays visually dominant.
/// Unselected swatches use a subtle alpha-blended border. The caller wraps
/// for DnD, context-menus, and hover-text — keep this focused on rendering.
pub fn swatch_button(
    ui: &mut egui::Ui,
    theme: &Theme,
    color: egui::Color32,
    size: egui::Vec2,
    corner_radius: u8,
    state: impl Into<SwatchSelect>,
) -> egui::Response {
    let outline = match state.into() {
        SwatchSelect::Primary | SwatchSelect::Extra => {
            egui::Stroke::new(stroke::ACCENT, theme.text)
        }
        SwatchSelect::None => egui::Stroke::NONE,
    };
    ui.add_sized(
        [size.x, size.y],
        egui::Button::new("")
            .fill(color)
            .stroke(outline)
            .corner_radius(egui::CornerRadius::same(corner_radius)),
    )
}

/// Opens a `horizontal_wrapped` row with the spacing tweaks every swatch grid
/// in this app shares: zero button padding, zero interact size, tight gaps.
pub fn swatch_grid<R>(
    ui: &mut egui::Ui,
    add: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::InnerResponse<R> {
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().button_padding = pad::NONE;
        ui.spacing_mut().interact_size = gap::NONE;
        ui.spacing_mut().item_spacing = gap::TIGHT;
        add(ui)
    })
}

/// Themed centred-modal `egui::Window` builder. Bold 14 pt title,
/// non-collapsible, non-resizable, panel-fill frame with 0.5 border and
/// rounded corners. Caller adds `.show(ctx, |ui| { ... })`.
pub fn modal_window<'a>(
    ctx: &egui::Context,
    theme: &Theme,
    title: &str,
    open: &'a mut bool,
) -> egui::Window<'a> {
    egui::Window::new(
        egui::RichText::new(title)
            .family(egui::FontFamily::Name(INTER_SEMIBOLD_FAMILY.into()))
            .size(font::HEADING),
    )
    .collapsible(false)
    .resizable(false)
    .open(open)
    .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
    .frame(
        egui::Frame::window(&ctx.style())
            .fill(theme.panel)
            .inner_margin(egui::Margin::symmetric(20, 16))
            .stroke(egui::Stroke::NONE)
            .shadow(crate::ui::tokens::shadow::high())
            .corner_radius(egui::CornerRadius::same(radius::LG)),
    )
}

/// Toggle chip used by selection rows (Theme: System / Light / Dark). Selected
/// = accent fill + white text + no stroke; unselected = surface + text + 0.5
/// border. Generic over the value so future selectors (shape kind, etc.) can
/// reuse it.
pub fn chip_button<T: PartialEq + Copy>(
    ui: &mut egui::Ui,
    theme: &Theme,
    current: &mut T,
    value: T,
    label: &str,
) -> egui::Response {
    let selected = *current == value;
    let (fill, fg, chip_stroke) = if selected {
        (theme.accent, egui::Color32::WHITE, egui::Stroke::NONE)
    } else {
        (
            theme.surface,
            theme.text,
            egui::Stroke::new(stroke::HAIR, theme.border),
        )
    };
    let resp = ui
        .scope(|ui| {
            ui.spacing_mut().button_padding = pad::BUTTON;
            ui.add(
                egui::Button::new(egui::RichText::new(label).color(fg).size(font::SMALL))
                    .fill(fill)
                    .stroke(chip_stroke)
                    .corner_radius(egui::CornerRadius::same(radius::SM)),
            )
        })
        .inner;
    if resp.clicked() {
        *current = value;
    }
    resp
}

/// Settings-modal row: fixed-width dim label on the left, custom content on
/// the right. Used by every row of the Preferences modal.
pub fn prefs_row(ui: &mut egui::Ui, theme: &Theme, label: &str, add: impl FnOnce(&mut egui::Ui)) {
    ui.horizontal(|ui| {
        ui.add_sized(
            size::PREFS_LABEL,
            egui::Label::new(
                egui::RichText::new(label)
                    .color(theme.text_dim)
                    .size(font::SMALL),
            ),
        );
        add(ui);
    });
}

/// Compact text button for modal action rows (Create / Cancel). `primary =
/// true` uses accent fill + white text + no stroke; `primary = false` uses
/// surface + text + 0.5 border. Matches `chip_button` styling so a row of
/// dialog buttons reads as the same family as other modal controls.
pub fn dialog_button(
    ui: &mut egui::Ui,
    theme: &Theme,
    label: &str,
    primary: bool,
) -> egui::Response {
    let (fill, fg, btn_stroke) = if primary {
        (theme.accent, egui::Color32::WHITE, egui::Stroke::NONE)
    } else {
        (
            theme.surface,
            theme.text,
            egui::Stroke::new(stroke::HAIR, theme.border),
        )
    };
    ui.scope(|ui| {
        ui.spacing_mut().button_padding = pad::DIALOG;
        ui.add(
            egui::Button::new(egui::RichText::new(label).color(fg).size(font::BODY))
                .fill(fill)
                .stroke(btn_stroke)
                .corner_radius(egui::CornerRadius::same(radius::SM)),
        )
    })
    .inner
}

/// Full-width icon + text button with the look used by inspector action rows
/// (the "Add current color" button). `width` is the explicit min width — pass
/// `ui.available_width()` for a panel-filling button.
pub fn wide_action_button(
    ui: &mut egui::Ui,
    theme: &Theme,
    icon: egui::ImageSource<'static>,
    label: &str,
    width: f32,
    enabled: bool,
) -> egui::Response {
    let tint = if enabled { theme.text } else { theme.text_dim };
    let img = egui::Image::new(icon)
        .fit_to_exact_size(icon::sm_square())
        .tint(tint);
    ui.scope(|ui| {
        ui.spacing_mut().button_padding = pad::ICON;
        ui.spacing_mut().interact_size = gap::NONE;
        ui.allocate_ui_with_layout(
            egui::vec2(width, size::ACTION_ROW_HEIGHT),
            egui::Layout::centered_and_justified(egui::Direction::LeftToRight),
            |ui| {
                ui.add_enabled(
                    enabled,
                    egui::Button::image_and_text(img, egui::RichText::new(label).size(font::SMALL))
                        .corner_radius(egui::CornerRadius::same(radius::SM))
                        .fill(theme.surface)
                        .stroke(egui::Stroke::new(stroke::HAIR, theme.border)),
                )
            },
        )
        .inner
    })
    .inner
}

/// Themed select dropdown matching the design system. Trigger is a panel-wide
/// button with the current label on the left and a chevron on the right;
/// click opens a popup of selectable rows. Use in place of `egui::ComboBox`
/// so the control reads as the same family as `chip_button` and other
/// surface-fill / hair-border widgets.
pub fn select_dropdown(
    ui: &mut egui::Ui,
    theme: &Theme,
    id_salt: &str,
    width: f32,
    selected_label: &str,
    items: &[String],
    selected_idx: usize,
) -> Option<usize> {
    let height = size::DROPDOWN_HEIGHT;
    let chevron_pad = space::SM;
    let label_pad = 10.0;

    let id = ui.make_persistent_id(id_salt);
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::click());
    let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);

    let popup_open = egui::Popup::is_id_open(ui.ctx(), id);
    let hovered = resp.hovered() || popup_open;
    let fill = if hovered {
        theme.surface_hover
    } else {
        theme.surface
    };
    let painter = ui.painter();
    painter.rect(
        rect,
        egui::CornerRadius::same(radius::SM),
        fill,
        egui::Stroke::new(stroke::HAIR, theme.border),
        egui::StrokeKind::Inside,
    );

    let text_rect = egui::Rect::from_min_max(
        rect.left_top() + egui::vec2(label_pad, 0.0),
        rect.right_bottom() - egui::vec2(height, 0.0),
    );
    painter.text(
        text_rect.left_center(),
        egui::Align2::LEFT_CENTER,
        selected_label,
        egui::FontId::new(font::BODY, egui::FontFamily::Proportional),
        theme.text,
    );

    let chev_size = icon::MD;
    let chev_rect = egui::Rect::from_center_size(
        egui::pos2(
            rect.right() - chevron_pad - chev_size * 0.5,
            rect.center().y,
        ),
        egui::vec2(chev_size, chev_size),
    );
    egui::Image::new(icons::chevron_down())
        .tint(theme.text_muted)
        .paint_at(ui, chev_rect);

    let mut chosen = None;
    egui::Popup::from_toggle_button_response(&resp)
        .id(id)
        .gap(4.0)
        .align(egui::RectAlign::BOTTOM_START)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .frame(
            egui::Frame::popup(ui.style())
                .fill(theme.panel)
                .stroke(egui::Stroke::new(stroke::HAIR, theme.border))
                .corner_radius(egui::CornerRadius::same(radius::MD))
                .inner_margin(egui::Margin::same(4)),
        )
        .width(width)
        .show(|ui| {
            ui.spacing_mut().item_spacing = gap::NONE;
            let row_w = ui.available_width();
            for (i, name) in items.iter().enumerate() {
                if select_row(ui, theme, name, i == selected_idx, row_w).clicked() {
                    chosen = Some(i);
                    egui::Popup::close_id(ui.ctx(), id);
                }
            }
        });
    chosen
}

fn select_row(
    ui: &mut egui::Ui,
    theme: &Theme,
    label: &str,
    selected: bool,
    width: f32,
) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(
        egui::vec2(width, size::ACTION_ROW_HEIGHT),
        egui::Sense::click(),
    );
    let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
    let hovered = resp.hovered();
    let fill = if selected {
        theme.accent
    } else if hovered {
        theme.surface_hover
    } else {
        egui::Color32::TRANSPARENT
    };
    let fg = if selected {
        egui::Color32::WHITE
    } else {
        theme.text
    };
    ui.painter()
        .rect_filled(rect, egui::CornerRadius::same(radius::XS), fill);
    ui.painter().text(
        rect.left_center() + egui::vec2(8.0, 0.0),
        egui::Align2::LEFT_CENTER,
        label,
        egui::FontId::new(font::BODY, egui::FontFamily::Proportional),
        fg,
    );
    resp
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_string_formats_uppercase_with_hash() {
        assert_eq!(hex_string([0, 0, 0]), "#000000");
        assert_eq!(hex_string([255, 255, 255]), "#FFFFFF");
        assert_eq!(hex_string([18, 26, 200]), "#121AC8");
        assert_eq!(hex_string([171, 205, 239]), "#ABCDEF");
    }

    #[test]
    fn section_header_title_cases_input() {
        assert_eq!(section_header_text("palette"), "Palette");
        assert_eq!(section_header_text("PALETTE"), "Palette");
        assert_eq!(section_header_text("recent colors"), "Recent Colors");
        assert_eq!(section_header_text(""), "");
    }

    #[test]
    fn tool_hint_covers_every_variant() {
        for t in [
            Tool::Brush,
            Tool::Erase,
            Tool::Paint,
            Tool::Eyedropper,
            Tool::Shape,
            Tool::Select,
            Tool::Move,
        ] {
            assert!(!tool_hint(t).is_empty(), "missing hint for {t:?}");
        }
    }
}
