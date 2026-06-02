use super::icons;
use crate::theme::{INTER_SEMIBOLD_FAMILY, Theme};
use crate::tools::{Tool, ToolState};
use crate::ui::tokens::{font, gap, icon, pad, radius, shadow, size, space, stroke};
use bevy_egui::egui;
use roxel::color_space::ColorSpace;

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
    label: &str,
    shortcut: &str,
) -> egui::Response {
    let active = tool.current == kind;
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
        ui.set_min_height(font::SECTION + 6.0);
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
        .allocate_exact_size(egui::vec2(ui.available_width(), 1.0), egui::Sense::hover())
        .0;
    let painter = ui.painter();
    painter.hline(
        ui.clip_rect().x_range(),
        painter.round_to_pixel_center(sep_rect.center().y),
        egui::Stroke::new(stroke::HAIR, theme.border),
    );
    ui.add_space(space::MD);
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
    // Full window rect (panels + canvas). `viewport_rect`/`content_rect` both
    // exclude the side panel, which would leave the inspector undimmed — for a
    // backdrop we explicitly want the whole window, so `screen_rect` is right
    // despite its deprecation toward the layout-oriented accessors.
    #[allow(deprecated)]
    let screen = ctx.screen_rect();
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

/// Themed select dropdown matching the design system. Trigger is a panel-wide
/// button with the current label on the left and a chevron on the right;
/// click opens a popup of selectable rows. Use in place of `egui::ComboBox`
/// so the control reads as the same family as `chip_button` and other
/// surface-fill / hair-border widgets.
#[allow(dead_code)] // reserved — last caller (prefs color format) moved to the View menu
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

#[allow(dead_code)] // reserved — used by select_dropdown
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
