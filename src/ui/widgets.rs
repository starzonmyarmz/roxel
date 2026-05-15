use crate::theme::{NUNITO_700_FAMILY, PlaneColorPref, Theme, plane_match_color};
use crate::tools::{Tool, ToolState};
use crate::ui::tokens::{font, gap, icon, pad, radius, space, stroke};
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
            egui::Label::new(
                egui::RichText::new(label)
                    .color(theme.text_dim)
                    .size(font::SMALL),
            ),
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
        ui.add_space(space::XS);
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
        egui::Stroke::new(stroke::HAIR, theme.border),
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
    icon_src: egui::ImageSource<'static>,
    label: &str,
    shortcut: &str,
) -> egui::Response {
    let active = tool.current == kind;
    let sh = theme.surface_hover;
    let bg = theme.bg;
    let blend = |a: u8, b: u8| (((a as u16) * 3 + b as u16) / 4) as u8;
    let hover_fill = egui::Color32::from_rgb(
        blend(bg.r(), sh.r()),
        blend(bg.g(), sh.g()),
        blend(bg.b(), sh.b()),
    );
    let resting = if active {
        theme.surface_hover
    } else {
        egui::Color32::TRANSPARENT
    };
    let hovered = if active { theme.surface_hover } else { hover_fill };
    let icon_img = egui::Image::new(icon_src)
        .fit_to_exact_size(icon::lg_square())
        .tint(theme.text);
    let resp = ui
        .scope(|ui| {
            ui.spacing_mut().button_padding = pad::NONE;
            ui.spacing_mut().interact_size = gap::NONE;
            let w = ui.visuals_mut();
            w.widgets.inactive.bg_fill = resting;
            w.widgets.inactive.weak_bg_fill = resting;
            w.widgets.inactive.bg_stroke = egui::Stroke::NONE;
            w.widgets.inactive.expansion = 0.0;
            w.widgets.hovered.bg_fill = hovered;
            w.widgets.hovered.weak_bg_fill = hovered;
            w.widgets.hovered.bg_stroke = egui::Stroke::NONE;
            w.widgets.hovered.expansion = 0.0;
            w.widgets.active.bg_fill = hovered;
            w.widgets.active.weak_bg_fill = hovered;
            w.widgets.active.bg_stroke = egui::Stroke::NONE;
            w.widgets.active.expansion = 0.0;
            ui.add_sized(
                [40.0, 40.0],
                egui::Button::image(icon_img)
                    .stroke(egui::Stroke::NONE)
                    .corner_radius(egui::CornerRadius::same(radius::SM)),
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
                .min_size(egui::vec2(28.0, 26.0))
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
    ui.label(
        egui::RichText::new(title)
            .color(theme.text)
            .size(font::BODY)
            .family(egui::FontFamily::Name(NUNITO_700_FAMILY.into())),
    );
    ui.add_space(space::SM);
    let r = add(ui);
    ui.add_space(space::MD);
    let sep_rect = ui
        .allocate_exact_size(egui::vec2(ui.available_width(), 1.0), egui::Sense::hover())
        .0;
    ui.painter().hline(
        ui.clip_rect().x_range(),
        sep_rect.center().y,
        egui::Stroke::new(stroke::HAIR, theme.border),
    );
    ui.add_space(space::MD);
    r
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
                .size(font::SMALL)
                .italics(),
        )
        .wrap(),
    );
}

/// Dim readout label, non-selectable. Used in status bar.
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

/// Single colour-square button. Selected swatches get a 2.0 px accent stroke;
/// unselected swatches a subtle alpha-blended border that reads on any fill
/// (white in dark mode, black in light mode). The caller wraps for DnD,
/// context-menus, and hover-text — keep this focused on rendering.
pub fn swatch_button(
    ui: &mut egui::Ui,
    theme: &Theme,
    color: egui::Color32,
    size: egui::Vec2,
    corner_radius: u8,
    selected: bool,
) -> egui::Response {
    let outline = if selected {
        egui::Stroke::new(stroke::ACCENT, theme.accent)
    } else {
        let border = match theme.mode {
            crate::theme::ThemeMode::Dark => egui::Color32::from_rgba_unmultiplied(255, 255, 255, 36),
            crate::theme::ThemeMode::Light => egui::Color32::from_rgba_unmultiplied(0, 0, 0, 36),
        };
        egui::Stroke::new(stroke::NORMAL, border)
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
            .size(font::BODY),
    )
    .collapsible(false)
    .resizable(false)
    .open(open)
    .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
    .frame(
        egui::Frame::window(&ctx.style())
            .fill(theme.panel)
            .inner_margin(egui::Margin::symmetric(16, 14))
            .stroke(egui::Stroke::new(stroke::HAIR, theme.border))
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
            [72.0, 20.0],
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
            egui::vec2(width, 26.0),
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
