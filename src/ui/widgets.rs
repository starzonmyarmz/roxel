use crate::theme::{NUNITO_700_FAMILY, PlaneColorPref, Theme, plane_match_color};
use crate::tools::{Tool, ToolState};
use crate::ui::icons;
use bevy_egui::egui;

pub fn plane_color_row(
    ui: &mut egui::Ui,
    theme: &Theme,
    mode: crate::theme::ThemeMode,
    label: &str,
    pref: &mut PlaneColorPref,
) {
    let mut is_custom = matches!(pref, PlaneColorPref::Custom(_));
    ui.horizontal(|ui| {
        ui.add_sized(
            [72.0, 20.0],
            egui::Label::new(egui::RichText::new(label).color(theme.text_dim).size(12.0)),
        );
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
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.add_space(76.0);
            ui.color_edit_button_srgb(rgb);
            hex_label(ui, theme, *rgb, true);
        });
    }
}

#[cfg_attr(target_os = "macos", allow(dead_code))]
pub fn vertical_rule(ui: &mut egui::Ui, theme: &Theme) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(1.0, 20.0), egui::Sense::hover());
    ui.painter().vline(
        rect.center().x,
        rect.y_range(),
        egui::Stroke::new(0.5, theme.border),
    );
}

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
    label: &str,
    shortcut: &str,
) {
    let active = tool.current == kind;
    let (fill, fg) = if active {
        (theme.accent, egui::Color32::WHITE)
    } else {
        (theme.surface, theme.text)
    };
    let icon = egui::Image::new(icons::tool(kind))
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

#[cfg_attr(target_os = "macos", allow(dead_code))]
pub fn icon_button(
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

pub fn icon_only_button(
    ui: &mut egui::Ui,
    theme: &Theme,
    icon: egui::ImageSource<'static>,
    enabled: bool,
) -> egui::Response {
    let tint = if enabled { theme.text } else { theme.text_dim };
    let img = egui::Image::new(icon)
        .fit_to_exact_size(egui::vec2(14.0, 14.0))
        .tint(tint);
    ui.scope(|ui| {
        ui.spacing_mut().button_padding = egui::vec2(0.0, 0.0);
        ui.spacing_mut().interact_size = egui::vec2(0.0, 0.0);
        ui.add_enabled(
            enabled,
            egui::Button::image(img)
                .min_size(egui::vec2(28.0, 26.0))
                .corner_radius(egui::CornerRadius::same(6))
                .stroke(egui::Stroke::new(0.5, theme.border))
                .fill(theme.surface),
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

pub fn stat_row(ui: &mut egui::Ui, theme: &Theme, label: &str, value: String) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(label).color(theme.text_dim).size(12.0));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add(
                egui::Label::new(
                    egui::RichText::new(value)
                        .monospace()
                        .color(theme.text)
                        .size(12.0),
                )
                .selectable(true),
            );
        });
    });
}

/// Dim italic 12 pt body text used for inline help under sections. Wraps.
pub fn hint_label(ui: &mut egui::Ui, theme: &Theme, text: &str) {
    ui.add(
        egui::Label::new(
            egui::RichText::new(text)
                .color(theme.text_dim)
                .size(12.0)
                .italics(),
        )
        .wrap(),
    );
}

/// Dim 12 pt readout label, non-selectable. Used in status bar.
pub fn status_label(ui: &mut egui::Ui, theme: &Theme, text: &str) {
    ui.add(
        egui::Label::new(egui::RichText::new(text).color(theme.text_dim).size(12.0))
            .selectable(false),
    );
}

/// `#RRGGBB` uppercase. Single source of truth for hex codes shown in the UI
/// (foreground swatch readout, palette/recent hover tips, custom-colour pref
/// rows).
pub fn hex_string(rgb: [u8; 3]) -> String {
    format!("#{:02X}{:02X}{:02X}", rgb[0], rgb[1], rgb[2])
}

/// Selectable monospace hex label. `dim = true` uses `theme.text_dim` + 12 pt
/// (settings-row style); `dim = false` uses `theme.text` + 13 pt (inspector
/// readout style).
pub fn hex_label(ui: &mut egui::Ui, theme: &Theme, rgb: [u8; 3], dim: bool) {
    let (colour, size) = if dim {
        (theme.text_dim, 12.0)
    } else {
        (theme.text, 13.0)
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

/// Single colour-square button. Selected swatches get a 2.0 px accent stroke;
/// unselected swatches a 0.5 px border. The caller wraps for DnD,
/// context-menus, and hover-text — keep this focused on rendering.
pub fn swatch_button(
    ui: &mut egui::Ui,
    theme: &Theme,
    color: egui::Color32,
    size: egui::Vec2,
    corner_radius: u8,
    selected: bool,
) -> egui::Response {
    let stroke = if selected {
        egui::Stroke::new(2.0, theme.accent)
    } else {
        egui::Stroke::new(0.5, theme.border)
    };
    ui.add_sized(
        [size.x, size.y],
        egui::Button::new("")
            .fill(color)
            .stroke(stroke)
            .corner_radius(egui::CornerRadius::same(corner_radius)),
    )
}

/// Opens a `horizontal_wrapped` row with the spacing tweaks every swatch grid
/// in this app shares: zero button padding, zero interact size, 5 px gaps.
pub fn swatch_grid<R>(
    ui: &mut egui::Ui,
    add: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::InnerResponse<R> {
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().button_padding = egui::vec2(0.0, 0.0);
        ui.spacing_mut().interact_size = egui::vec2(0.0, 0.0);
        ui.spacing_mut().item_spacing = egui::vec2(5.0, 5.0);
        add(ui)
    })
}

/// Themed centred-modal `egui::Window` builder. Nunito-700 14 pt title,
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
            .family(egui::FontFamily::Name(NUNITO_700_FAMILY.into()))
            .size(14.0),
    )
    .collapsible(false)
    .resizable(false)
    .open(open)
    .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
    .frame(
        egui::Frame::window(&ctx.style())
            .fill(theme.panel)
            .inner_margin(egui::Margin::symmetric(16, 14))
            .stroke(egui::Stroke::new(0.5, theme.border))
            .corner_radius(egui::CornerRadius::same(10)),
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
    let (fill, fg, stroke) = if selected {
        (theme.accent, egui::Color32::WHITE, egui::Stroke::NONE)
    } else {
        (
            theme.surface,
            theme.text,
            egui::Stroke::new(0.5, theme.border),
        )
    };
    let resp = ui
        .scope(|ui| {
            ui.spacing_mut().button_padding = egui::vec2(10.0, 4.0);
            ui.add(
                egui::Button::new(egui::RichText::new(label).color(fg).size(12.0))
                    .fill(fill)
                    .stroke(stroke)
                    .corner_radius(egui::CornerRadius::same(6)),
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
            [72.0, 20.0],
            egui::Label::new(egui::RichText::new(label).color(theme.text_dim).size(12.0)),
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
    let (fill, fg, stroke) = if primary {
        (theme.accent, egui::Color32::WHITE, egui::Stroke::NONE)
    } else {
        (
            theme.surface,
            theme.text,
            egui::Stroke::new(0.5, theme.border),
        )
    };
    ui.scope(|ui| {
        ui.spacing_mut().button_padding = egui::vec2(14.0, 6.0);
        ui.add(
            egui::Button::new(egui::RichText::new(label).color(fg).size(13.0))
                .fill(fill)
                .stroke(stroke)
                .corner_radius(egui::CornerRadius::same(6)),
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
        .fit_to_exact_size(egui::vec2(13.0, 13.0))
        .tint(tint);
    ui.scope(|ui| {
        ui.spacing_mut().button_padding = egui::vec2(10.0, 0.0);
        ui.spacing_mut().interact_size = egui::vec2(0.0, 0.0);
        ui.allocate_ui_with_layout(
            egui::vec2(width, 26.0),
            egui::Layout::centered_and_justified(egui::Direction::LeftToRight),
            |ui| {
                ui.add_enabled(
                    enabled,
                    egui::Button::image_and_text(img, egui::RichText::new(label).size(12.0))
                        .corner_radius(egui::CornerRadius::same(6))
                        .fill(theme.surface)
                        .stroke(egui::Stroke::new(0.5, theme.border)),
                )
            },
        )
        .inner
    })
    .inner
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
