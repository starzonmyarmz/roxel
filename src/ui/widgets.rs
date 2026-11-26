use super::icons;
use crate::actions::{Chord, KeyToken, Platform};
use crate::theme::{INTER_SEMIBOLD_FAMILY, Theme};
use crate::tools::{Tool, ToolState};
use crate::ui::tokens::{font, gap, height, icon, pad, radius, shadow, size, space, stroke, width};
use bevy_egui::egui;
use roxel::color_space::ColorSpace;
use std::borrow::Cow;
use std::sync::Arc;

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
        Tool::Paint => {
            "Click or drag to recolor voxels. Double-click floods a region; F fills a selection."
        }
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
) -> egui::Response {
    let active = tool.current == kind;
    let label = kind.label();
    let button_size = size::TOOL_BUTTON;

    // Static three-state palette — no animation. The animated version stuttered
    // on hover (the visible fill alternated between two greys mid-hover), so
    // motion is reverted in favour of a snappy, predictable hover.
    let neutral_hover = theme.hover_fill();

    // Active tool tints the icon accent (coral) over a faint coral wash. The
    // wash is an *opaque* blend of accent into the pill ground (`theme.panel`):
    // a low-alpha accent fill just desaturates to grey over the dark surface,
    // so blend opaquely to keep the hue. No hover state — selected is terminal,
    // so the wash holds steady on hover (same fill across all states).
    let accent_wash = {
        let t = 0.18;
        let mix = |bg: u8, fg: u8| (bg as f32 + (fg as f32 - bg as f32) * t).round() as u8;
        let (a, p) = (theme.accent, theme.panel);
        egui::Color32::from_rgb(mix(p.r(), a.r()), mix(p.g(), a.g()), mix(p.b(), a.b()))
    };

    let (inactive_fill, hovered_fill, icon_tint) = if active {
        (accent_wash, accent_wash, theme.accent)
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
    match crate::actions::Action::SelectTool(kind).shortcut_text() {
        Some(shortcut) => resp.on_hover_text(format!("{label}  ({shortcut})")),
        None => resp.on_hover_text(label),
    }
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
    let hover_fill = theme.hover_fill();
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
    section_divider(ui, theme);
    result
}

/// Label row painted by [`section`]. Uppercase SemiBold label, no chevron,
/// no click target — sections are always expanded.
fn section_header(ui: &mut egui::Ui, theme: &Theme, title: &str) {
    let row_h = height::SECTION_HEADER;
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

/// One piece of a key-combo display: a key chip (text or a Lucide modifier
/// icon) or a dim connecting word such as "+ click" that isn't a key.
#[derive(Clone, Debug, PartialEq)]
pub enum KeySeg {
    Text(Cow<'static, str>),
    Command,
    Shift,
    Option,
    Word(&'static str),
}

impl From<KeyToken> for KeySeg {
    fn from(t: KeyToken) -> Self {
        match t {
            KeyToken::Command => KeySeg::Command,
            KeyToken::Shift => KeySeg::Shift,
            KeyToken::Control => KeySeg::Text("Ctrl".into()),
            KeyToken::Key(k) => KeySeg::Text(k),
        }
    }
}

impl KeySeg {
    fn icon_source(&self) -> Option<egui::ImageSource<'static>> {
        match self {
            KeySeg::Command => Some(icons::command()),
            KeySeg::Shift => Some(icons::arrow_big_up()),
            KeySeg::Option => Some(icons::option()),
            _ => None,
        }
    }
}

pub fn chord_segs(chord: Chord, platform: Platform) -> Vec<KeySeg> {
    chord
        .tokens(platform)
        .into_iter()
        .map(KeySeg::from)
        .collect()
}

const KEY_CHIP_H: f32 = icon::XS + space::SM;

fn key_seg_galley(ui: &egui::Ui, seg: &KeySeg, theme: &Theme) -> Option<Arc<egui::Galley>> {
    let (text, font_id, color) = match seg {
        KeySeg::Text(s) => (
            s.to_string(),
            egui::FontId::monospace(font::SMALL),
            theme.text,
        ),
        KeySeg::Word(s) => (
            s.to_string(),
            egui::FontId::proportional(font::SMALL),
            theme.text_dim,
        ),
        _ => return None,
    };
    Some(ui.painter().layout_no_wrap(text, font_id, color))
}

fn key_seg_width(galley: Option<&Arc<egui::Galley>>, seg: &KeySeg) -> f32 {
    match (seg, galley) {
        (KeySeg::Word(_), Some(g)) => g.size().x + space::XS * 2.0,
        (_, Some(g)) => g.size().x + space::SX * 2.0,
        _ => icon::XS + space::SX * 2.0,
    }
}

/// Paints `segs` right-aligned to `right`, vertically centred on `center_y`.
pub fn paint_key_segs(ui: &egui::Ui, theme: &Theme, segs: &[KeySeg], right: f32, center_y: f32) {
    let laid: Vec<_> = segs
        .iter()
        .map(|s| {
            let g = key_seg_galley(ui, s, theme);
            let w = key_seg_width(g.as_ref(), s);
            (s, g, w)
        })
        .collect();
    let total = laid.iter().map(|(_, _, w)| w).sum::<f32>()
        + space::XXS * laid.len().saturating_sub(1) as f32;
    let mut x = right - total;
    let y = center_y - KEY_CHIP_H * 0.5;
    for (seg, galley, w) in laid {
        let rect = egui::Rect::from_min_size(egui::pos2(x, y), egui::vec2(w, KEY_CHIP_H));
        if !matches!(seg, KeySeg::Word(_)) {
            ui.painter()
                .rect_filled(rect, egui::CornerRadius::same(radius::XS), theme.surface);
        }
        match galley {
            Some(g) => {
                let pos = rect.center() - g.size() * 0.5;
                ui.painter().galley(pos, g, theme.text);
            }
            None => {
                if let Some(src) = seg.icon_source() {
                    let img =
                        egui::Rect::from_center_size(rect.center(), egui::vec2(icon::XS, icon::XS));
                    egui::Image::new(src).tint(theme.text).paint_at(ui, img);
                }
            }
        }
        x += w + space::XXS;
    }
}

/// A label on the left with its key combo as chips on the right.
pub fn key_row(ui: &mut egui::Ui, theme: &Theme, label: &str, segs: &[KeySeg]) {
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), KEY_CHIP_H),
        egui::Sense::hover(),
    );
    let galley = ui.painter().layout_no_wrap(
        label.to_string(),
        egui::FontId::proportional(font::SMALL),
        theme.text_dim,
    );
    let pos = egui::pos2(rect.left(), rect.center().y - galley.size().y * 0.5);
    ui.painter().galley(pos, galley, theme.text_dim);
    paint_key_segs(ui, theme, segs, rect.right(), rect.center().y);
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
    let outline = swatch_outline(theme, state.into());
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

/// Section header row with a right-aligned action (e.g. the palette `…` menu).
/// Mirrors [`section_header`] but reserves the right edge for `action`. The
/// caller renders the section body inline after this and closes it with
/// [`section_divider`] — splitting header and body into two calls lets the
/// header action and the body each borrow shared state in turn rather than
/// both at once (which a single two-closure helper could not).
pub fn section_header_action(
    ui: &mut egui::Ui,
    theme: &Theme,
    title: &str,
    action: impl FnOnce(&mut egui::Ui),
) {
    ui.horizontal(|ui| {
        ui.set_min_height(height::SECTION_HEADER);
        ui.label(
            egui::RichText::new(section_header_text(title))
                .family(egui::FontFamily::Name(INTER_SEMIBOLD_FAMILY.into()))
                .size(font::SECTION)
                .color(theme.text_muted),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), action);
    });
    ui.add_space(space::SM);
}

/// Full-width hairline divider that closes a section opened with
/// [`section_header_action`]. Paints the same separator [`section`] does.
pub fn section_divider(ui: &mut egui::Ui, theme: &Theme) {
    ui.add_space(space::MD);
    let sep_rect = ui
        .allocate_exact_size(
            egui::vec2(ui.available_width(), stroke::NORMAL),
            egui::Sense::hover(),
        )
        .0;
    hline_at(ui, theme, sep_rect.center().y);
    ui.add_space(space::MD);
}

/// Hairline divider at `y`, pixel-snapped, spanning the clip rect so it runs
/// past the frame's inner margin to both edges. Paints only; the caller
/// spaces around it.
pub fn hline_at(ui: &egui::Ui, theme: &Theme, y: f32) {
    let painter = ui.painter();
    painter.hline(
        ui.clip_rect().x_range(),
        painter.round_to_pixel_center(y),
        egui::Stroke::new(stroke::HAIR, theme.border),
    );
}

/// Border stroke for a swatch in the given selection state. Matches
/// [`swatch_button`] so painted cells and button cells read identically.
fn swatch_outline(theme: &Theme, state: SwatchSelect) -> egui::Stroke {
    match state {
        SwatchSelect::Primary | SwatchSelect::Extra => {
            egui::Stroke::new(stroke::ACCENT, theme.text)
        }
        SwatchSelect::None => egui::Stroke::NONE,
    }
}

/// Paint a colour square at an explicit `rect` (no allocation). The palette
/// grid lays out its own cells — so it can open a gap that shifts swatches
/// aside mid-drag — and drives interaction via `ui.interact`, so the painter
/// and the responder are separated here.
pub fn paint_swatch(
    ui: &mut egui::Ui,
    theme: &Theme,
    rect: egui::Rect,
    color: egui::Color32,
    corner_radius: u8,
    state: impl Into<SwatchSelect>,
) {
    ui.painter().rect(
        rect,
        egui::CornerRadius::same(corner_radius),
        color,
        swatch_outline(theme, state.into()),
        egui::StrokeKind::Inside,
    );
}

/// Mono color readout tooltip in the active format. Single attach point so
/// every color reference in the sidebar hovers identically (monospace, same
/// `ColorSpace::format` string the under-swatch readout uses).
pub fn color_tooltip(resp: egui::Response, space: ColorSpace, rgb: [u8; 3]) -> egui::Response {
    resp.on_hover_ui(|ui| {
        ui.label(egui::RichText::new(space.format(rgb)).monospace());
    })
}

/// Shared sidebar colour swatch: paints the square with the hover-grow every
/// sidebar swatch shares and attaches the mono color tooltip. `swatch_cell`
/// allocates its own cell (recent-color strip); [`swatch_cell_at`] paints at a
/// caller-supplied rect so the palette grid can open a reorder gap. Both return
/// the response — the palette layers drag-to-reorder + a remove context menu on
/// top, which is the only behavioural difference between the two.
#[allow(clippy::too_many_arguments)]
pub fn swatch_cell(
    ui: &mut egui::Ui,
    theme: &Theme,
    color: egui::Color32,
    rgb: [u8; 3],
    size: egui::Vec2,
    corner_radius: u8,
    state: impl Into<SwatchSelect>,
    space: ColorSpace,
) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(size, egui::Sense::click_and_drag());
    finish_swatch(
        ui,
        theme,
        rect,
        resp,
        color,
        rgb,
        corner_radius,
        state.into(),
        space,
    )
}

/// [`swatch_cell`] at an explicit `rect` + `id`, for the palette grid's
/// self-managed layout.
#[allow(clippy::too_many_arguments)]
pub fn swatch_cell_at(
    ui: &mut egui::Ui,
    theme: &Theme,
    id: egui::Id,
    rect: egui::Rect,
    color: egui::Color32,
    rgb: [u8; 3],
    corner_radius: u8,
    state: impl Into<SwatchSelect>,
    space: ColorSpace,
) -> egui::Response {
    let resp = ui.interact(rect, id, egui::Sense::click_and_drag());
    finish_swatch(
        ui,
        theme,
        rect,
        resp,
        color,
        rgb,
        corner_radius,
        state.into(),
        space,
    )
}

#[allow(clippy::too_many_arguments)]
fn finish_swatch(
    ui: &mut egui::Ui,
    theme: &Theme,
    rect: egui::Rect,
    resp: egui::Response,
    color: egui::Color32,
    rgb: [u8; 3],
    corner_radius: u8,
    state: SwatchSelect,
    space: ColorSpace,
) -> egui::Response {
    // Painted cells grow by the global hover expansion so they read the same as
    // egui Button-backed swatches did.
    let paint_rect = if resp.hovered() {
        rect.expand(ui.visuals().widgets.hovered.expansion)
    } else {
        rect
    };
    paint_swatch(ui, theme, paint_rect, color, corner_radius, state);
    color_tooltip(resp, space, rgb)
}

/// Paint the `+` "add current colour" affordance at an explicit `rect`.
/// Surface fill + hair border; the glyph dims when `enabled` is false and the
/// fill lifts to `surface_hover` when `hovered`.
pub fn paint_add_swatch(
    ui: &mut egui::Ui,
    theme: &Theme,
    rect: egui::Rect,
    corner_radius: u8,
    enabled: bool,
    hovered: bool,
) {
    let fill = if enabled && hovered {
        theme.surface_hover
    } else {
        theme.surface
    };
    ui.painter().rect(
        rect,
        egui::CornerRadius::same(corner_radius),
        fill,
        egui::Stroke::new(stroke::HAIR, theme.border),
        egui::StrokeKind::Inside,
    );
    let tint = if enabled {
        theme.text_dim
    } else {
        theme.text_dim.gamma_multiply(0.4)
    };
    let img_rect = egui::Rect::from_center_size(rect.center(), icon::sm_square());
    egui::Image::new(icons::plus())
        .tint(tint)
        .paint_at(ui, img_rect);
}

/// Shows `panel` against the whole window and records the space it leaves as
/// the canvas (`crate::ui::canvas_rect`).
pub fn window_panel<R>(
    ctx: &egui::Context,
    panel: egui::Panel,
    add_contents: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::InnerResponse<R> {
    let mut root = egui::Ui::new(
        ctx.clone(),
        egui::Id::new("window_panel_root"),
        egui::UiBuilder::new()
            .layer_id(egui::LayerId::background())
            .max_rect(ctx.content_rect()),
    );
    let resp = panel.show(&mut root, add_contents);
    crate::ui::set_canvas_rect(ctx, root.available_rect_before_wrap());
    resp
}

/// Alpha (0–255) of the full-screen dim painted behind an open modal.
const SCRIM_ALPHA: u8 = 96;

/// Full-screen dim backdrop drawn behind any open modal. Runs at
/// `Order::Middle`: above the canvas and inspector, below the modal surfaces
/// (which render at `Order::Foreground`, so they always sit on top). The
/// floating tool island / menu pill (egui `Foreground`) and the gizmo (a
/// separate Bevy camera) can't be covered by a Middle layer, so `ui_system`
/// hides those outright while a modal is open rather than relying on the scrim.
/// Senses clicks so the pointer can't reach the canvas behind it — no stray
/// voxels land while a modal is up.
pub fn modal_scrim(ctx: &egui::Context) {
    // Full window rect (inspector + canvas), so the inspector dims too.
    let screen = ctx.viewport_rect();
    egui::Area::new(egui::Id::new("modal_scrim"))
        .order(egui::Order::Middle)
        .fixed_pos(screen.min)
        .show(ctx, |ui| {
            ui.allocate_rect(screen, egui::Sense::click());
            ui.painter().rect_filled(
                screen,
                egui::CornerRadius::ZERO,
                egui::Color32::from_black_alpha(SCRIM_ALPHA),
            );
        });
}

/// Shared frame for every floating modal surface: modal windows, the command
/// palette, the palette switcher, and the new-project sheet. Panel fill, no
/// border, high shadow, `radius::LG` corners — only the inner margin varies
/// (`pad::MODAL` for sheets, `pad::SEARCH` for the search-style palettes). One
/// source so the four surfaces can't visually drift apart.
pub fn modal_frame(theme: &Theme, inner_margin: egui::Vec2) -> egui::Frame {
    egui::Frame::new()
        .fill(theme.panel)
        .stroke(egui::Stroke::NONE)
        .shadow(shadow::high())
        .corner_radius(egui::CornerRadius::same(radius::LG))
        .inner_margin(egui::Margin::symmetric(
            inner_margin.x as i8,
            inner_margin.y as i8,
        ))
}

/// Themed centred-modal `egui::Window` builder. SemiBold heading title,
/// non-collapsible, non-resizable, [`modal_frame`] surface. Caller adds
/// `.show(ctx, |ui| { ... })`.
pub fn modal_window<'a>(theme: &Theme, title: &str, open: &'a mut bool) -> egui::Window<'a> {
    egui::Window::new(
        egui::RichText::new(title)
            .family(egui::FontFamily::Name(INTER_SEMIBOLD_FAMILY.into()))
            .size(font::HEADING),
    )
    .collapsible(false)
    .resizable(false)
    .open(open)
    .order(egui::Order::Foreground)
    .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
    .frame(modal_frame(theme, pad::MODAL))
}

/// Centred confirm sheet: heading, prompt, and a right-aligned button row.
/// `buttons` are in reading order, left to right; the one whose value is
/// `enter` is the accent button and answers Enter, and Esc answers `cancel`.
/// Returns the chosen value, or `None` while still open. An anchored `Area`
/// rather than `egui::Window` so the sheet sizes tight to its content.
pub fn confirm_dialog<T: Copy + PartialEq>(
    ctx: &egui::Context,
    theme: &Theme,
    heading: &str,
    prompt: &str,
    buttons: &[(&str, T)],
    enter: T,
    cancel: T,
) -> Option<T> {
    let mut clicked = None;
    egui::Area::new(egui::Id::new("confirm_dialog").with(heading))
        .order(egui::Order::Foreground)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .show(ctx, |ui| {
            modal_frame(theme, pad::MODAL).show(ui, |ui| {
                ui.set_width(width::MODAL_CONFIRM);
                ui.vertical(|ui| {
                    ui.label(modal_heading(theme, heading));
                    ui.add_space(space::XS);
                    hint_label(ui, theme, prompt);
                    ui.add_space(space::SM);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.spacing_mut().item_spacing.x = space::SX;
                        for &(label, value) in buttons.iter().rev() {
                            if dialog_button(ui, theme, label, value == enter).clicked() {
                                clicked = Some(value);
                            }
                        }
                    });
                });
            });
        });
    clicked.or_else(|| {
        ctx.input(|i| {
            if i.key_pressed(egui::Key::Escape) {
                Some(cancel)
            } else if i.key_pressed(egui::Key::Enter) {
                Some(enter)
            } else {
                None
            }
        })
    })
}

/// SemiBold heading text in `theme.text` at [`font::HEADING`]. Used by the
/// `Area`-based confirm sheets that build their own heading row (the
/// `egui::Window` modals get theirs from [`modal_window`]).
pub fn modal_heading(theme: &Theme, title: &str) -> egui::RichText {
    egui::RichText::new(title)
        .family(egui::FontFamily::Name(INTER_SEMIBOLD_FAMILY.into()))
        .size(font::HEADING)
        .color(theme.text)
}

/// Toggle chip used by selection rows (Theme: System / Light / Dark). Selected
/// = accent fill + `on_accent` text + no stroke; unselected = surface + text + 0.5
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
        (theme.accent, theme.on_accent, egui::Stroke::NONE)
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
                    // Never break the label across lines (e.g. "3:4" → "3/:4");
                    // a too-wide chip wraps to the next row whole instead.
                    .wrap_mode(egui::TextWrapMode::Extend)
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
        ui.allocate_ui_with_layout(
            size::PREFS_LABEL,
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.set_min_width(size::PREFS_LABEL.x);
                ui.add(egui::Label::new(
                    egui::RichText::new(label)
                        .color(theme.text_dim)
                        .size(font::SMALL),
                ));
            },
        );
        add(ui);
    });
}

/// Compact text button for modal action rows (Create / Cancel). `primary =
/// true` uses accent fill + `on_accent` text + no stroke; `primary = false` uses
/// surface + text + 0.5 border. Matches `chip_button` styling so a row of
/// dialog buttons reads as the same family as other modal controls.
pub fn dialog_button(
    ui: &mut egui::Ui,
    theme: &Theme,
    label: &str,
    primary: bool,
) -> egui::Response {
    let (fill, fg, btn_stroke) = if primary {
        (theme.accent, theme.on_accent, egui::Stroke::NONE)
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
    let label_pad = space::FIELD_PAD_X;

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
        .gap(space::XS)
        .align(egui::RectAlign::BOTTOM_START)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .frame(
            egui::Frame::popup(ui.style())
                .fill(theme.panel)
                .stroke(egui::Stroke::new(stroke::HAIR, theme.border))
                .corner_radius(egui::CornerRadius::same(radius::MD))
                .inner_margin(egui::Margin::same(space::XS as i8)),
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
        theme.on_accent
    } else {
        theme.text
    };
    ui.painter()
        .rect_filled(rect, egui::CornerRadius::same(radius::XS), fill);
    ui.painter().text(
        rect.left_center() + egui::vec2(space::SM, 0.0),
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
    fn key_tokens_map_to_chips() {
        assert_eq!(KeySeg::from(KeyToken::Control), KeySeg::Text("Ctrl".into()));
        assert_eq!(
            KeySeg::from(KeyToken::Key("N".into())),
            KeySeg::Text("N".into())
        );
        assert_eq!(KeySeg::from(KeyToken::Shift), KeySeg::Shift);
        assert_eq!(KeySeg::from(KeyToken::Command), KeySeg::Command);
    }

    #[test]
    fn modifier_chips_draw_icons_and_words_do_not() {
        for seg in [KeySeg::Command, KeySeg::Shift, KeySeg::Option] {
            assert!(seg.icon_source().is_some(), "{seg:?}");
        }
        assert!(KeySeg::Word("+ click").icon_source().is_none());
        assert!(KeySeg::Text("Z".into()).icon_source().is_none());
    }

    fn run_confirm(events: Vec<egui::Event>) -> Option<u8> {
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        ctx.run_ui(egui::RawInput::default(), |_| {})
            .drop_without_applying_deltas();
        let mut out = None;
        let input = egui::RawInput {
            events,
            ..Default::default()
        };
        ctx.run_ui(input, |ui| {
            out = confirm_dialog(
                ui.ctx(),
                &Theme::dark(),
                "Heading",
                "Prompt",
                &[("Discard", 0), ("Cancel", 1), ("Save", 2)],
                2,
                1,
            );
        })
        .drop_without_applying_deltas();
        out
    }

    fn key(key: egui::Key) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }
    }

    #[test]
    fn confirm_dialog_answers_enter_and_esc() {
        assert_eq!(run_confirm(vec![]), None);
        assert_eq!(run_confirm(vec![key(egui::Key::Enter)]), Some(2));
        assert_eq!(run_confirm(vec![key(egui::Key::Escape)]), Some(1));
        assert_eq!(
            run_confirm(vec![key(egui::Key::Escape), key(egui::Key::Enter)]),
            Some(1)
        );
    }

    fn pointer_over_ui_beside_window_panel(x: f32) -> bool {
        let ctx = egui::Context::default();
        let screen = egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(800.0, 600.0));
        let mut over = false;
        for _ in 0..2 {
            let input = egui::RawInput {
                screen_rect: Some(screen),
                events: vec![egui::Event::PointerMoved(egui::pos2(x, 300.0))],
                ..Default::default()
            };
            // An unused root `Ui`, the way bevy_egui runs the primary pass.
            ctx.run_ui(input, |ui| {
                let panel = egui::Panel::left("test_panel").exact_size(200.0);
                window_panel(ui.ctx(), panel, |ui| ui.label("panel"));
                over = crate::ui::pointer_over_ui(ui.ctx());
            })
            .drop_without_applying_deltas();
        }
        over
    }

    #[test]
    fn window_panel_leaves_the_rest_of_the_window_as_canvas() {
        assert!(pointer_over_ui_beside_window_panel(100.0));
        assert!(!pointer_over_ui_beside_window_panel(500.0));
    }

    #[test]
    fn hline_at_snaps_to_a_pixel_center_across_the_clip_rect() {
        let ctx = egui::Context::default();
        let mut clip = egui::Rect::NOTHING;
        let out = ctx.run_ui(egui::RawInput::default(), |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                clip = ui.clip_rect();
                hline_at(ui, &Theme::dark(), 10.3);
            });
        });
        let line = out
            .shapes
            .iter()
            .find_map(|c| match &c.shape {
                egui::Shape::LineSegment { points, .. } => Some(points),
                _ => None,
            })
            .expect("hline painted");
        assert_eq!(line[0].y, 10.5);
        assert_eq!(line[1].y, 10.5);
        assert_eq!(line[0].x, clip.left());
        assert_eq!(line[1].x, clip.right());
        out.drop_without_applying_deltas();
    }

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
