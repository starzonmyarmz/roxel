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
    let sh = theme.surface_hover;
    let bg = theme.bg;
    let blend = |a: u8, b: u8| (((a as u16) * 3 + b as u16) / 4) as u8;
    let hover_fill = egui::Color32::from_rgb(
        blend(bg.r(), sh.r()),
        blend(bg.g(), sh.g()),
        blend(bg.b(), sh.b()),
    );
    // Selected tool reads in accent — fill + tinted icon — so the active state
    // pops as identity colour, not just "another grey button." Hover state on
    // an inactive tool stays neutral so the rail still scans evenly.
    let resting = if active {
        theme.accent
    } else {
        egui::Color32::TRANSPARENT
    };
    let hovered = if active { theme.accent } else { hover_fill };
    let icon_tint = if active {
        egui::Color32::WHITE
    } else {
        theme.text
    };
    let icon_img = egui::Image::new(icon_src)
        .fit_to_exact_size(icon::lg_square())
        .tint(icon_tint);
    let button_size = egui::vec2(40.0, 40.0);
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
                button_size,
                egui::Button::image(icon_img)
                    .stroke(egui::Stroke::NONE)
                    .corner_radius(egui::CornerRadius::same(radius::INSIDE_PILL)),
            )
        })
        .inner;
    // Translucent halo around the active tool button — three concentric rings
    // outside the rect with falling alpha approximate an outer glow, since
    // egui has no blurred-stroke primitive.
    if active {
        paint_tool_button_halo(ui, resp.rect, theme.accent);
    }
    if resp.clicked() && tool.current != kind {
        tool.previous = tool.current;
        tool.current = kind;
    }
    resp.on_hover_text(format!("{label}  ({shortcut})"))
}

/// Outer accent halo painted behind the selected tool button. Three 1-px
/// strokes outside the button rect at falling alpha approximate a soft glow
/// without a blurred-stroke primitive.
fn paint_tool_button_halo(ui: &egui::Ui, rect: egui::Rect, accent: egui::Color32) {
    let painter = ui.painter();
    let cr = egui::CornerRadius::same(radius::INSIDE_PILL);
    for (i, alpha) in [110, 60, 26].iter().enumerate() {
        let grow = (i as f32 + 1.0) * 1.5;
        let ring_rect = rect.expand(grow);
        let ring_cr = egui::CornerRadius::same((radius::INSIDE_PILL + grow as u8).min(255));
        let _ = cr;
        let mut col = accent;
        col = egui::Color32::from_rgba_unmultiplied(col.r(), col.g(), col.b(), *alpha);
        painter.rect_stroke(
            ring_rect,
            ring_cr,
            egui::Stroke::new(1.0, col),
            egui::StrokeKind::Outside,
        );
    }
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
    ui.label(section_header_richtext(theme, title));
    ui.add_space(space::SM);
    let r = add(ui);
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
    r
}

/// Uppercase, slightly-tracked section label shared by every inspector
/// section. Renders at `font::SECTION` (12 pt) in SemiBold and is dimmed
/// vs body text so headings read as control-surface furniture instead of
/// blog-post chapter titles. Tracking is faked by interleaving hair spaces
/// because egui has no per-character letter-spacing knob yet.
pub fn section_header_richtext(theme: &Theme, title: &str) -> egui::RichText {
    let spaced: String = title.chars().flat_map(|c| [c, '\u{200A}']).collect();
    egui::RichText::new(spaced.to_uppercase())
        .color(theme.text_dim)
        .size(font::SECTION)
        .family(egui::FontFamily::Name(INTER_SEMIBOLD_FAMILY.into()))
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
            crate::theme::ThemeMode::Dark => {
                egui::Color32::from_rgba_unmultiplied(255, 255, 255, 36)
            }
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
        .tint(theme.text_dim)
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
