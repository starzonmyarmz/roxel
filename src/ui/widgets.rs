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
            egui::Label::new(
                egui::RichText::new(label).color(theme.text_dim).size(12.0),
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
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.add_space(76.0);
            ui.color_edit_button_srgb(rgb);
            ui.add(
                egui::Label::new(
                    egui::RichText::new(format!(
                        "#{:02X}{:02X}{:02X}",
                        rgb[0], rgb[1], rgb[2]
                    ))
                    .monospace()
                    .color(theme.text_dim)
                    .size(12.0),
                )
                .selectable(true),
            );
        });
    }
}

#[cfg_attr(target_os = "macos", allow(dead_code))]
pub fn vertical_rule(ui: &mut egui::Ui, theme: &Theme) {
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(1.0, 20.0), egui::Sense::hover());
    ui.painter()
        .vline(rect.center().x, rect.y_range(), egui::Stroke::new(0.5, theme.border));
}

pub fn tool_label(t: Tool) -> &'static str {
    match t {
        Tool::Brush => "Brush",
        Tool::Erase => "Erase",
        Tool::Paint => "Paint",
        Tool::Eyedropper => "Pick",
        Tool::Shape => "Shape",
        Tool::Select => "Select",
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
                .min_size(egui::vec2(28.0, 24.0))
                .corner_radius(egui::CornerRadius::same(5))
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
