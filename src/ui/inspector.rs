//! The left inspector side panel, split out of `ui_system` (`ui.rs`). One
//! function per section; the palette section further splits into its `…`
//! menu, the rename row, the swatch grid, and applying the grid's edit.

use crate::actions::{Action, ActionQueue, ActionState, Origin};
use crate::select::{SelectState, Selection};
use crate::theme::{Preferences, Theme};
use crate::tools::{
    CurrentColor, ExtraColors, RecentColors, ShapeOptions, ShapeState, Tool, ToolState,
    apply_swatch_click,
};
use crate::ui::dialogs::{CurrentProjectPath, DocStatus};
use crate::ui::palette::{self, PaletteChoice, PaletteRenameState, Palettes, WorkingPalette};
use crate::ui::tokens::{
    font, gap, height, motion, pad, radius, size, space, stroke, swatch, width,
};
use crate::ui::{ZoomReadout, color_picker, icons, widgets};
use bevy::math::IVec3;
use bevy_egui::egui;
use roxel::color_space::ColorEditBuffer;
use roxel::grid::{Color8, VoxelGrid};
use roxel::history::History;
use roxel::io;

/// Top-left of a swatch cell at a fractional grid slot, lerping between the two
/// bracketing integer cells. Whole slots map straight to the integer cell;
/// in-between values slide horizontally within a row and diagonally across a
/// row break. Drives the palette reorder reflow animation.
fn slot_min_lerp(origin: egui::Pos2, cols: usize, step: egui::Vec2, slot_f: f32) -> egui::Pos2 {
    let cell_min = |slot: usize| {
        let r = (slot / cols) as f32;
        let c = (slot % cols) as f32;
        origin + egui::vec2(c * step.x, r * step.y)
    };
    let lo = slot_f.floor().max(0.0) as usize;
    let t = slot_f - lo as f32;
    let a = cell_min(lo);
    let b = cell_min(lo + 1);
    a + (b - a) * t
}

/// Items of the palette `…` menu; `None` is a separator.
const PALETTE_MENU: &[Option<Action>] = &[
    Some(Action::SwitchPalette),
    None,
    Some(Action::NewPalette),
    Some(Action::DuplicatePalette),
    Some(Action::RenamePalette),
    Some(Action::DeletePalette),
    None,
    Some(Action::ExportAse),
    Some(Action::ImportAse),
];

/// A palette `…` menu item with its label and enabled state resolved.
pub struct PaletteMenuRow {
    action: Action,
    label: std::borrow::Cow<'static, str>,
    enabled: bool,
}

/// Resolve [`PALETTE_MENU`] against `s` before the inspector takes its
/// mutable borrows of the palettes.
pub fn palette_menu_rows(s: &ActionState) -> Vec<Option<PaletteMenuRow>> {
    PALETTE_MENU
        .iter()
        .map(|item| {
            item.map(|action| PaletteMenuRow {
                action,
                label: action.menu_label(s),
                enabled: action.enabled(s),
            })
        })
        .collect()
}

/// Everything the inspector reads or edits, borrowed from `ui_system`.
pub struct Inspector<'a, 'w, 's> {
    pub grid: &'a VoxelGrid,
    pub bounds: Option<(IVec3, IVec3)>,
    pub history: &'a History,
    pub zoom: &'a ZoomReadout<'w, 's>,
    pub current_path: &'a CurrentProjectPath,
    pub doc: &'a DocStatus,
    pub color: &'a mut CurrentColor,
    pub color_edit: &'a mut ColorEditBuffer,
    pub prefs: &'a Preferences,
    pub recent: &'a RecentColors,
    pub extras: &'a mut ExtraColors,
    pub palettes: &'a mut Palettes,
    pub palette_choice: &'a mut PaletteChoice,
    pub palette_rename: &'a mut PaletteRenameState,
    pub working: &'a mut WorkingPalette,
    pub palette_menu: &'a [Option<PaletteMenuRow>],
    pub actions: &'a mut ActionQueue,
    pub tool: &'a ToolState,
    pub shape_options: &'a ShapeOptions,
    pub shape_state: &'a ShapeState,
    pub selection: &'a Selection,
    pub select_state: &'a SelectState,
    pub selection_voxels: usize,
}

/// The left inspector side panel: Status / Color / Palette / Shape /
/// Selection sections. Returns the panel response (`None` when the UI is
/// hidden) so the caller can anchor onboarding and fix render ordering.
pub fn inspector_panel(
    ctx: &egui::Context,
    visible: bool,
    theme: &Theme,
    mut insp: Inspector,
) -> Option<egui::InnerResponse<()>> {
    if !visible {
        crate::ui::set_canvas_rect(ctx, ctx.content_rect());
        return None;
    }
    let mac_gutter: i8 = if cfg!(target_os = "macos") {
        height::MAC_TITLEBAR_GUTTER as i8
    } else {
        0
    };
    let inset = space::PANEL_INSET as i8;
    let panel = egui::Panel::left("inspector_panel")
        .resizable(true)
        // egui's separator draws at `noninteractive.bg_stroke` (1.0px) and
        // stacks on the custom HAIR vline below — reads thick. Suppress it;
        // the hairline is the edge, the resize grab region still works.
        .show_separator_line(false)
        .default_size(width::SIDE_PANEL)
        .min_size(width::SIDE_PANEL)
        .max_size(width::SIDE_PANEL_MAX)
        .frame(
            egui::Frame::default()
                .fill(theme.panel)
                .inner_margin(egui::Margin {
                    left: inset,
                    right: 0,
                    top: inset + mac_gutter,
                    bottom: inset,
                }),
        );
    let resp = widgets::window_panel(ctx, panel, |ui| {
        egui::ScrollArea::vertical()
            .auto_shrink([false, true])
            .show(ui, |ui| {
                let inner = egui::Frame::default().inner_margin(egui::Margin {
                    right: inset,
                    ..egui::Margin::ZERO
                });
                inner.show(ui, |ui| {
                    status_section(ui, theme, &insp);
                    color_section(ui, theme, &mut insp);
                    palette_section(ui, theme, &mut insp);
                    shape_section(ui, theme, &insp);
                    selection_section(ui, theme, &insp);
                });
            });
    });
    Some(resp)
}

fn extent_label(extent: IVec3) -> String {
    format!("{} × {} × {}", extent.x, extent.y, extent.z)
}

fn zoom_label(radius: f32) -> String {
    let r = radius.round().max(0.0) as i32;
    if r == 1 {
        "1 voxel".to_string()
    } else {
        format!("{r} voxels")
    }
}

/// File name (with a `•` when modified), design size, voxel count and zoom.
fn status_section(ui: &mut egui::Ui, theme: &Theme, insp: &Inspector) {
    widgets::section(ui, theme, "Status", |ui| {
        ui.spacing_mut().item_spacing.y = space::XXS;
        let doc_name = insp
            .current_path
            .0
            .as_ref()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .unwrap_or("Untitled");
        let file_label = if insp.doc.is_modified(insp.history) {
            format!("{doc_name} •")
        } else {
            doc_name.to_string()
        };
        widgets::stat_row(ui, theme, "File", file_label);
        let size_label = match insp.bounds {
            Some((min, max)) => extent_label(max - min + IVec3::ONE),
            None => "—".to_string(),
        };
        widgets::stat_row(ui, theme, "Size", size_label);
        widgets::stat_row(ui, theme, "Voxels", insp.grid.count().to_string());
        if let Some(cam) = insp.zoom.cameras.iter().next() {
            let actual = cam.radius.unwrap_or(cam.target_radius);
            widgets::stat_row(ui, theme, "Zoom", zoom_label(actual));
        }
    });
}

fn swatch_select(color: Color8, primary: Color8, extras: &ExtraColors) -> widgets::SwatchSelect {
    if primary == color {
        widgets::SwatchSelect::Primary
    } else if extras.contains(color) {
        widgets::SwatchSelect::Extra
    } else {
        widgets::SwatchSelect::None
    }
}

fn color32(c: Color8) -> egui::Color32 {
    egui::Color32::from_rgba_unmultiplied(c[0], c[1], c[2], 255)
}

/// Hero swatch (opens the picker popup, the only place to edit numerically),
/// the readout in the active color format, and the recent-colors strip.
fn color_section(ui: &mut egui::Ui, theme: &Theme, insp: &mut Inspector) {
    widgets::section(ui, theme, "Color", |ui| {
        let c = insp.color.0;
        let srgba = egui::Color32::from_rgba_unmultiplied(c[0], c[1], c[2], c[3]);
        let swatch_w = ui.available_width();
        let swatch_resp = widgets::swatch_button(
            ui,
            theme,
            srgba,
            egui::vec2(swatch_w, swatch::HERO_HEIGHT),
            radius::MD,
            false,
        )
        .on_hover_text("Click to edit color");
        let active_space = insp.prefs.color_space;
        egui::Popup::menu(&swatch_resp)
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .show(|ui| {
                color_picker::space_color_picker(ui, insp.color, active_space, insp.color_edit);
            });

        ui.add_space(space::XS);
        let c = insp.color.0;
        ui.vertical_centered(|ui| {
            ui.add(
                egui::Label::new(
                    egui::RichText::new(active_space.format([c[0], c[1], c[2]]))
                        .monospace()
                        .size(font::SMALL)
                        .color(theme.text_dim),
                )
                .selectable(true),
            );
        });

        if !insp.recent.0.is_empty() {
            ui.add_space(space::XS);
            widgets::swatch_grid(ui, |ui| {
                for &c in &insp.recent.0 {
                    let resp = widgets::swatch_cell(
                        ui,
                        theme,
                        color32(c),
                        [c[0], c[1], c[2]],
                        swatch::RECENT,
                        radius::XS,
                        swatch_select(c, insp.color.0, insp.extras),
                        active_space,
                    );
                    if resp.clicked() {
                        let shift = ui.input(|i| i.modifiers.shift);
                        insp.color.0 = apply_swatch_click(shift, c, insp.color.0, insp.extras);
                    }
                }
            });
        }
    });
}

/// Palette title row with its `…` menu, the inline rename row, and the
/// swatch grid.
fn palette_section(ui: &mut egui::Ui, theme: &Theme, insp: &mut Inspector) {
    let last = insp.palettes.0.len().saturating_sub(1);
    if insp.palette_choice.0 > last {
        insp.palette_choice.0 = 0;
    }

    widgets::section_header_action(ui, theme, "Palette", |ui| {
        palette_menu_button(ui, theme, insp.palette_menu, insp.actions);
    });

    // Refresh — the menu may have changed the selection.
    let active_idx = insp.palette_choice.0.min(last);
    insp.palette_choice.0 = active_idx;
    if insp.palette_rename.editing == Some(active_idx) && !insp.palettes.0[active_idx].builtin {
        ui.add_space(space::XS);
        rename_row(ui, theme, insp, active_idx);
    }

    let colors = palette::display_colors(insp.palettes, insp.working, active_idx).to_vec();
    let edit = palette_grid(ui, theme, insp, &colors);
    apply_palette_edit(edit, insp, active_idx);

    widgets::section_divider(ui, theme);
}

fn palette_menu_button(
    ui: &mut egui::Ui,
    theme: &Theme,
    rows: &[Option<PaletteMenuRow>],
    actions: &mut ActionQueue,
) {
    let menu_resp = widgets::icon_only_button(ui, theme, icons::ellipsis(), true)
        .on_hover_text("Palette actions");
    egui::Popup::menu(&menu_resp)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClick)
        .show(|ui| {
            // Roomier menu items so labels breathe off both edges.
            ui.set_min_width(width::TOP_BAR_MENU);
            ui.spacing_mut().button_padding = pad::MENU;
            for row in rows {
                let Some(row) = row else {
                    ui.separator();
                    continue;
                };
                let button = egui::Button::new(&*row.label);
                if ui.add_enabled(row.enabled, button).clicked() {
                    actions.push(row.action, Origin::Ui);
                }
            }
        });
}

/// Inline rename field with Save / Cancel. Enter saves, Esc cancels.
fn rename_row(ui: &mut egui::Ui, theme: &Theme, insp: &mut Inspector, active_idx: usize) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = gap::TIGHT.x;
        let rename = &mut *insp.palette_rename;
        let resp = ui
            .add(egui::TextEdit::singleline(&mut rename.buf).desired_width(rename_field_width(ui)));
        if !resp.has_focus() && !resp.lost_focus() {
            resp.request_focus();
        }
        let commit = widgets::icon_only_button(ui, theme, icons::check(), true)
            .on_hover_text("Save name")
            .clicked()
            || (resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)));
        let cancel = widgets::icon_only_button(ui, theme, icons::x(), true)
            .on_hover_text("Cancel")
            .clicked()
            || ui.input(|i| i.key_pressed(egui::Key::Escape));
        if commit {
            let trimmed = rename.buf.trim();
            if !trimmed.is_empty() {
                insp.palettes.0[active_idx].name = trimmed.to_string();
                io::palettes::save(&insp.palettes.0);
            }
        }
        if commit || cancel {
            rename.editing = None;
            rename.buf.clear();
        }
    });
}

/// Rename field width: what's left of the row after the Save and Cancel icon
/// buttons, less a hair so the panel never grows to fit it.
fn rename_field_width(ui: &egui::Ui) -> f32 {
    let buttons = 2.0 * (size::ICON_BUTTON.x + gap::TIGHT.x);
    (ui.available_width() - buttons - space::XXS).max(width::RENAME_FIELD_MIN)
}

/// What the swatch grid asked for this frame.
#[derive(Default, Debug, PartialEq)]
struct PaletteEdit {
    add: bool,
    reorder: Option<(usize, usize)>,
    remove: Option<usize>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Slot {
    Color(usize),
    Gap,
    Add,
}

/// Grid slot order: the non-dragged swatches (`others`), an empty gap at
/// `gap_pos` while dragging, then the trailing `+` cell.
fn slot_layout(others: &[usize], gap_pos: Option<usize>) -> Vec<Slot> {
    let mut slots = Vec::with_capacity(others.len() + 2);
    for (pos, &oi) in others.iter().enumerate() {
        if gap_pos == Some(pos) {
            slots.push(Slot::Gap);
        }
        slots.push(Slot::Color(oi));
    }
    if gap_pos == Some(others.len()) {
        slots.push(Slot::Gap);
    }
    slots.push(Slot::Add);
    slots
}

/// Index into the non-dragged swatches where a drop at `rel` (pointer offset
/// from the grid origin) would land. Columns round to the nearest gap between
/// cells; rows floor.
fn drop_index(rel: egui::Vec2, step: egui::Vec2, cols: usize, rows: usize, len: usize) -> usize {
    let c = ((rel.x / step.x) + 0.5).floor().clamp(0.0, cols as f32);
    let r = (rel.y / step.y)
        .floor()
        .clamp(0.0, (rows as f32 - 1.0).max(0.0));
    ((r as usize) * cols + c as usize).min(len)
}

/// The palette swatch grid. Each cell is painted at a computed rect and
/// interacts via `ui.interact` instead of letting egui flow the swatches, so a
/// dragged swatch can leave its slot and the rest shift to open a gap at the
/// drop target. Fixed rows also dodge the wrap bug where click-and-drag cells
/// refuse to wrap in `horizontal_wrapped`.
fn palette_grid(
    ui: &mut egui::Ui,
    theme: &Theme,
    insp: &mut Inspector,
    colors: &[Color8],
) -> PaletteEdit {
    let mut edit = PaletteEdit::default();
    let add_enabled = !colors.contains(&insp.color.0);
    let n = colors.len();
    let cell = swatch::PALETTE;
    let step = cell + gap::TIGHT;
    let avail = ui.available_width();
    let cols = ((avail + gap::TIGHT.x) / step.x).floor().max(1.0) as usize;
    let rows = (n + 1).div_ceil(cols);
    let grid_h = (rows as f32 * step.y - gap::TIGHT.y).max(cell.y);

    // Drag source set on a previous frame (egui clears the dnd payload on
    // pointer release), clamped to a live index.
    let dragging = egui::DragAndDrop::payload::<usize>(ui.ctx())
        .map(|p| *p)
        .filter(|&d| d < n);

    let (grid_rect, _) = ui.allocate_exact_size(egui::vec2(avail, grid_h), egui::Sense::hover());
    let origin = grid_rect.min;
    let cell_at_f =
        |slot_f: f32| egui::Rect::from_min_size(slot_min_lerp(origin, cols, step, slot_f), cell);

    let others: Vec<usize> = (0..n).filter(|&i| Some(i) != dragging).collect();
    let pointer = ui.ctx().pointer_interact_pos();
    let gap_pos = dragging
        .and(pointer)
        .map(|p| drop_index(p - origin, step, cols, rows, others.len()));

    let base = ui.id();
    let color_fmt = insp.prefs.color_space;
    for (slot_idx, slot) in slot_layout(&others, gap_pos).into_iter().enumerate() {
        // Animate each cell from its old slot to its new one so the grid
        // reflows around the moving gap. The gap paints nothing — it reads as
        // open space the swatches slide away from.
        let anim_id = match slot {
            Slot::Color(oi) => base.with(("swatch_slot", oi)),
            Slot::Add => base.with("add_slot"),
            Slot::Gap => base.with("gap_slot"),
        };
        let rect = cell_at_f(ui.ctx().animate_value_with_time(
            anim_id,
            slot_idx as f32,
            insp.prefs.motion_secs(motion::SWATCH_REFLOW),
        ));
        match slot {
            Slot::Gap => {}
            Slot::Add => {
                let resp = ui.interact(rect, base.with("palette_add"), egui::Sense::click());
                widgets::paint_add_swatch(ui, theme, rect, radius::XS, add_enabled, resp.hovered());
                let resp = resp.on_hover_text(if add_enabled {
                    "Add current color"
                } else {
                    "Color already in palette"
                });
                if add_enabled {
                    edit.add = resp.clicked();
                    resp.on_hover_cursor(egui::CursorIcon::PointingHand);
                }
            }
            Slot::Color(si) => {
                let c = colors[si];
                let resp = widgets::swatch_cell_at(
                    ui,
                    theme,
                    base.with(("palette_swatch", si)),
                    rect,
                    color32(c),
                    [c[0], c[1], c[2]],
                    radius::XS,
                    swatch_select(c, insp.color.0, insp.extras),
                    color_fmt,
                );
                if resp.clicked() {
                    let shift = ui.input(|i| i.modifiers.shift);
                    insp.color.0 = apply_swatch_click(shift, c, insp.color.0, insp.extras);
                }
                if resp.dragged() {
                    egui::DragAndDrop::set_payload(ui.ctx(), si);
                }
                egui::Popup::context_menu(&resp).show(|ui| {
                    if ui.button("Remove").clicked() {
                        edit.remove = Some(si);
                        ui.close();
                    }
                });
            }
        }
    }

    // The dragged swatch follows the cursor as a lifted ghost; its interaction
    // id lives here now (it left its grid slot), so the release is detected
    // and committed against the gap.
    if let Some(from) = dragging {
        let ghost_center = pointer.unwrap_or_else(|| cell_at_f(0.0).center());
        let ghost = egui::Rect::from_center_size(ghost_center, cell);
        ui.painter().rect_filled(
            ghost.translate(egui::vec2(0.0, space::XXS)),
            egui::CornerRadius::same(radius::XS),
            egui::Color32::from_black_alpha(70),
        );
        ui.painter().rect(
            ghost,
            egui::CornerRadius::same(radius::XS),
            color32(colors[from]),
            egui::Stroke::new(stroke::ACCENT, theme.text),
            egui::StrokeKind::Inside,
        );
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
        let resp = ui.interact(
            ghost,
            base.with(("palette_swatch", from)),
            egui::Sense::click_and_drag(),
        );
        if resp.dragged() {
            egui::DragAndDrop::set_payload(ui.ctx(), from);
        }
        if resp.drag_stopped()
            && let Some(to) = gap_pos
            && to != from
        {
            edit.reorder = Some((from, to));
        }
    }
    edit
}

fn apply_palette_edit(edit: PaletteEdit, insp: &mut Inspector, active_idx: usize) {
    if edit.add {
        insp.actions.push(Action::AddColorToPalette, Origin::Ui);
    }
    if edit.remove.is_none() && edit.reorder.is_none() {
        return;
    }
    let (colors, persist) = palette::edit_colors(insp.palettes, insp.working, active_idx);
    let mut changed = false;
    if let Some(i) = edit.remove
        && i < colors.len()
    {
        colors.remove(i);
        changed = true;
    }
    if let Some((from, to)) = edit.reorder
        && from < colors.len()
        && to < colors.len()
    {
        let c = colors.remove(from);
        colors.insert(to, c);
        changed = true;
    }
    if changed && persist {
        io::palettes::save(&insp.palettes.0);
    }
}

/// Live bounds and voxel count while a shape is being drawn.
fn shape_section(ui: &mut egui::Ui, theme: &Theme, insp: &Inspector) {
    let state = &insp.shape_state.drag;
    if insp.tool.current != Tool::Shape || state.is_idle() {
        return;
    }
    let (Some(anchor), Some(c1), Some(c2)) = (state.anchor, state.corner1, state.corner2) else {
        return;
    };
    let cells = roxel::shapes::compute_shape_cells(
        insp.shape_options.primitive,
        c1,
        c2,
        anchor.axis,
        state.thickness,
        state.normal_sign,
    );
    let Some(bounds) = roxel::shapes::cell_bounds(&cells) else {
        return;
    };
    widgets::section(ui, theme, "Shape", |ui| {
        widgets::stat_row(ui, theme, "Bounds", extent_label(bounds));
        widgets::stat_row(ui, theme, "Voxels", cells.len().to_string());
    });
}

/// Bounds and voxel count of the committed or in-progress selection.
fn selection_section(ui: &mut egui::Ui, theme: &Theme, insp: &Inspector) {
    let Some(aabb) = crate::select::displayed_aabb(insp.selection, insp.select_state) else {
        return;
    };
    widgets::section(ui, theme, "Selection", |ui| {
        widgets::stat_row(ui, theme, "Bounds", extent_label(aabb.extents()));
        widgets::stat_row(ui, theme, "Voxels", insp.selection_voxels.to_string());
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    const ORIGIN: egui::Pos2 = egui::Pos2::new(10.0, 20.0);
    const STEP: egui::Vec2 = egui::Vec2::new(30.0, 40.0);
    const COLS: usize = 4;

    #[test]
    fn slot_min_lerp_whole_slots_match_integer_cells() {
        // Slot 0 sits at the origin; slot `cols` wraps to the next row.
        assert_eq!(slot_min_lerp(ORIGIN, COLS, STEP, 0.0), ORIGIN);
        assert_eq!(
            slot_min_lerp(ORIGIN, COLS, STEP, 2.0),
            ORIGIN + egui::vec2(2.0 * STEP.x, 0.0)
        );
        assert_eq!(
            slot_min_lerp(ORIGIN, COLS, STEP, COLS as f32),
            ORIGIN + egui::vec2(0.0, STEP.y)
        );
    }

    #[test]
    fn slot_min_lerp_halfway_within_row_slides_horizontally() {
        // 0 → 1 is a pure horizontal slide; midpoint is half a step in x only.
        let mid = slot_min_lerp(ORIGIN, COLS, STEP, 0.5);
        assert_eq!(mid, ORIGIN + egui::vec2(STEP.x * 0.5, 0.0));
    }

    #[test]
    fn slot_min_lerp_across_row_break_slides_diagonally() {
        // Last cell of row 0 (slot 3) → first of row 1 (slot 4): the lerp moves
        // left across the row and down a row at once, so x drops and y rises.
        let a = slot_min_lerp(ORIGIN, COLS, STEP, 3.0);
        let mid = slot_min_lerp(ORIGIN, COLS, STEP, 3.5);
        let b = slot_min_lerp(ORIGIN, COLS, STEP, 4.0);
        assert!(mid.x < a.x && mid.x > b.x);
        assert!(mid.y > a.y && mid.y < b.y);
    }
    #[test]
    fn slot_layout_opens_the_gap_and_ends_with_add() {
        assert_eq!(
            slot_layout(&[0, 1], None),
            vec![Slot::Color(0), Slot::Color(1), Slot::Add]
        );
        assert_eq!(
            slot_layout(&[0, 2], Some(1)),
            vec![Slot::Color(0), Slot::Gap, Slot::Color(2), Slot::Add]
        );
        assert_eq!(
            slot_layout(&[0, 2], Some(2)),
            vec![Slot::Color(0), Slot::Color(2), Slot::Gap, Slot::Add]
        );
    }

    #[test]
    fn drop_index_rounds_columns_and_clamps_to_the_list() {
        let step = egui::Vec2::new(28.0, 28.0);
        assert_eq!(drop_index(egui::Vec2::new(0.0, 0.0), step, 4, 2, 6), 0);
        assert_eq!(drop_index(egui::Vec2::new(20.0, 0.0), step, 4, 2, 6), 1);
        assert_eq!(drop_index(egui::Vec2::new(10.0, 30.0), step, 4, 2, 6), 4);
        assert_eq!(drop_index(egui::Vec2::new(999.0, 999.0), step, 4, 2, 6), 6);
        assert_eq!(drop_index(egui::Vec2::new(-50.0, -50.0), step, 4, 2, 6), 0);
    }

    #[test]
    fn status_labels_read_naturally() {
        assert_eq!(zoom_label(1.2), "1 voxel");
        assert_eq!(zoom_label(31.6), "32 voxels");
        assert_eq!(extent_label(IVec3::new(3, 4, 5)), "3 × 4 × 5");
    }
}
