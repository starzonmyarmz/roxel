mod color_picker;
mod command_palette;
mod dialogs;
mod floating;
pub(crate) mod icons;
mod modals;
mod palette;
mod palette_switcher;
pub mod toast;
pub mod tokens;
mod visibility;
mod widgets;

pub use command_palette::{
    CommandPalette, command_palette_shortcut_system, dispatch_command_palette_system,
};
pub use dialogs::{
    CurrentProjectPath, DialogResult, DocStatus, OpenRequest, PendingDialog, PendingImport,
    RecentFiles, new_dialog, poll_dialogs_system, spawn_open, spawn_save, spawn_save_as,
};
pub use palette::{
    DiscardConfirm, Palette, PaletteChoice, PaletteSwitcher, Palettes, WorkingPalette,
};
pub use toast::{Toasts, toast_lifetime_system};
pub use visibility::{UiVisible, tab_toggle_system};

use crate::gizmo::{GizmoDrag, GizmoRect};
use crate::grid::{NewProject, VoxelGrid};
use crate::history::History;
use crate::io;
use crate::onboarding::{Onboarding, OnboardingAnchors};
use crate::theme::{Preferences, PreferencesWindow, Theme, apply_egui_style};
use crate::tools::{
    CurrentColor, ExtraColors, RecentColors, ShapeOptions, ShapeState, Tool, ToolState,
    apply_swatch_click,
};
use crate::ui::tokens::{font, gap, height, motion, radius, space, stroke, swatch, width};
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};
use bevy_panorbit_camera::PanOrbitCamera;
use palette::PaletteParams;

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

/// `true` on any frame a modal/palette is open. Set by `ui_system`, read by
/// `gizmo::update_gizmo_viewport` to deactivate the orientation cube — its
/// camera composites after egui, so the modal scrim can't cover it otherwise.
#[derive(Resource, Default)]
pub struct ModalActive(pub bool);

#[derive(SystemParam)]
pub struct ZoomReadout<'w, 's> {
    cameras: Query<'w, 's, &'static PanOrbitCamera>,
}

#[derive(SystemParam)]
pub struct PrefsParams<'w> {
    pub prefs: ResMut<'w, Preferences>,
    pub window: ResMut<'w, PreferencesWindow>,
}

#[derive(SystemParam)]
pub struct GizmoView<'w> {
    pub rect: Res<'w, GizmoRect>,
    pub drag: Res<'w, GizmoDrag>,
}

#[derive(SystemParam)]
pub struct ColorParams<'w> {
    pub color: ResMut<'w, CurrentColor>,
    pub extras: ResMut<'w, ExtraColors>,
    pub recent: Res<'w, RecentColors>,
}

#[derive(SystemParam)]
pub struct UiInput<'w> {
    pub keys: Res<'w, ButtonInput<KeyCode>>,
    pub mouse: Res<'w, ButtonInput<MouseButton>>,
}

#[derive(SystemParam)]
pub struct UiState<'w> {
    pub new_project: ResMut<'w, NewProject>,
    pub selection: ResMut<'w, crate::select::Selection>,
    pub select_state: Res<'w, crate::select::SelectState>,
    pub shape_state: Res<'w, ShapeState>,
    pub toasts: Res<'w, Toasts>,
    pub current_path: Res<'w, CurrentProjectPath>,
    pub doc: Res<'w, DocStatus>,
    pub open_request: ResMut<'w, OpenRequest>,
    pub flyby: Res<'w, crate::camera::FlybyState>,
    pub color_edit: ResMut<'w, crate::color_space::ColorEditBuffer>,
    pub updater: ResMut<'w, crate::updater::UpdateCheck>,
    pub clipboard: Res<'w, crate::clipboard::Clipboard>,
    pub onboarding: ResMut<'w, Onboarding>,
    pub onboarding_anchors: ResMut<'w, OnboardingAnchors>,
    pub ui_visible: Res<'w, UiVisible>,
}

#[derive(SystemParam)]
pub struct UiCore<'w> {
    pub tool: ResMut<'w, ToolState>,
    pub pending: ResMut<'w, PendingDialog>,
    pub theme: Res<'w, Theme>,
    pub shape_options: ResMut<'w, ShapeOptions>,
    pub cmd_palette: ResMut<'w, CommandPalette>,
    pub modal_active: ResMut<'w, ModalActive>,
}

#[derive(SystemParam)]
pub struct UiBundles<'w, 's> {
    pub colors: ColorParams<'w>,
    pub palette_params: PaletteParams<'w, 's>,
    pub prefs_params: PrefsParams<'w>,
    pub input: UiInput<'w>,
    pub zoom: ZoomReadout<'w, 's>,
    pub gizmo_view: GizmoView<'w>,
    pub ui_state: UiState<'w>,
}

pub fn ui_system(
    mut contexts: EguiContexts,
    #[cfg_attr(target_os = "macos", allow(unused_mut))] mut grid: ResMut<VoxelGrid>,
    #[cfg_attr(target_os = "macos", allow(unused_mut))] mut history: ResMut<History>,
    core: UiCore,
    bundles: UiBundles,
) -> Result {
    let UiCore {
        mut tool,
        mut pending,
        theme,
        mut shape_options,
        mut cmd_palette,
        mut modal_active,
    } = core;
    let UiBundles {
        colors,
        palette_params,
        prefs_params,
        input,
        zoom,
        gizmo_view,
        ui_state,
    } = bundles;
    let ColorParams {
        mut color,
        mut extras,
        recent,
    } = colors;
    let UiInput { keys, mouse } = input;
    #[cfg_attr(target_os = "macos", allow(unused_variables, unused_mut))]
    let UiState {
        mut new_project,
        selection,
        select_state,
        shape_state,
        toasts,
        current_path,
        doc,
        mut open_request,
        flyby,
        mut color_edit,
        mut updater,
        clipboard,
        #[cfg_attr(target_os = "macos", allow(unused_variables))]
        mut onboarding,
        mut onboarding_anchors,
        ui_visible,
    } = ui_state;
    let ctx = contexts.ctx_mut()?;
    egui_extras::install_image_loaders(ctx);
    apply_egui_style(ctx, &theme);

    let PaletteParams {
        mut palettes,
        choice: mut palette_choice,
        rename: mut palette_rename,
        mut working,
        mut discard,
        mut switcher,
    } = palette_params;
    let PrefsParams {
        mut prefs,
        window: mut prefs_window,
    } = prefs_params;

    // Local bindings shadow the previous module-level constants so that the
    // rest of this function can stay as it was.
    #[allow(non_snake_case)]
    let PANEL = theme.panel;
    #[allow(non_snake_case, unused_variables)]
    let TEXT = theme.text;
    #[allow(non_snake_case, unused_variables)]
    let TEXT_DIM = theme.text_dim;
    #[allow(non_snake_case)]
    let BORDER = theme.border;

    // True whenever a modal/palette is open. The scrim dims the canvas +
    // inspector behind the modal; the floating tool island, menu pill, and the
    // gizmo (`ModalActive`, see `gizmo.rs`) are hidden outright rather than
    // dimmed, since they render above the Middle scrim and can't be covered by
    // it.
    let modal_open = prefs_window.open
        || new_project.dialog_open
        || open_request.confirming
        || switcher.open
        || discard.pending.is_some()
        || cmd_palette.open;
    modal_active.0 = modal_open;

    // ---------- Floating menu pill ----------
    // On macOS the native menu bar (see `menu.rs`) replaces these controls; on
    // Win/Linux the pill sits at top-center and is gated by the user pref.
    #[cfg(not(target_os = "macos"))]
    if ui_visible.0 && prefs.show_floating_menu_bar && !modal_open {
        floating::pill_menu(ctx, &theme, |ui| {
            floating::pill_menu_contents(
                ui,
                &theme,
                &mut new_project,
                &mut pending,
                &mut open_request,
                &current_path,
                &mut prefs,
                &mut history,
                &mut grid,
                &mut onboarding,
                &mut prefs_window,
                &mut updater,
            );
        });
    }

    // ---------- Floating tool island ----------
    // Hidden behind a modal (it's egui Foreground, above the Middle scrim).
    if ui_visible.0 && !modal_open {
        let island_resp = floating::tool_island(ctx, &theme, |ui| {
            floating::tool_island_contents(ui, &theme, &mut tool, &mut shape_options, &mut prefs);
        });
        onboarding_anchors.tool_rail = Some(island_resp.rect);
    }

    // ---------- Left inspector panel ----------
    let mac_gutter: i8 = if cfg!(target_os = "macos") {
        height::MAC_TITLEBAR_GUTTER as i8
    } else {
        0
    };
    let inspector_top: i8 = 12 + mac_gutter;
    let inspector_resp = if ui_visible.0 {
        Some(
            egui::SidePanel::left("inspector_panel")
                .resizable(true)
                // egui's separator draws at `noninteractive.bg_stroke` (1.0px) and
                // stacks on the custom HAIR vline below — reads thick. Suppress it;
                // the hairline is the edge, the resize grab region still works.
                .show_separator_line(false)
                .default_width(width::SIDE_PANEL)
                .min_width(width::SIDE_PANEL)
                .max_width(width::SIDE_PANEL_MAX)
                .frame(
                    egui::Frame::default()
                        .fill(PANEL)
                        .inner_margin(egui::Margin {
                            left: 12,
                            right: 0,
                            top: inspector_top,
                            bottom: 12,
                        }),
                )
                .show(ctx, |ui| {
                    egui::ScrollArea::vertical()
                        .auto_shrink([false, true])
                        .show(ui, |ui| {
                            let inner_frame = egui::Frame::default().inner_margin(egui::Margin {
                                left: 0,
                                right: 12,
                                top: 0,
                                bottom: 0,
                            });
                            inner_frame.show(ui, |ui| {
                                // Status section (design size, voxel count, zoom)
                                widgets::section(ui, &theme, "Status", |ui| {
                                    ui.spacing_mut().item_spacing.y = space::XXS;
                                    let doc_name = current_path
                                        .0
                                        .as_ref()
                                        .and_then(|p| p.file_name())
                                        .and_then(|n| n.to_str())
                                        .unwrap_or("Untitled");
                                    let file_label = if doc.is_modified(&history) {
                                        format!("{doc_name} •")
                                    } else {
                                        doc_name.to_string()
                                    };
                                    widgets::stat_row(ui, &theme, "File", file_label);
                                    let design_label = match grid.bounding_box_cached() {
                                        Some((min, max)) => {
                                            let extent = max - min + bevy::math::IVec3::ONE;
                                            format!("{} × {} × {}", extent.x, extent.y, extent.z)
                                        }
                                        None => "—".to_string(),
                                    };
                                    widgets::stat_row(ui, &theme, "Size", design_label);
                                    widgets::stat_row(
                                        ui,
                                        &theme,
                                        "Voxels",
                                        grid.count().to_string(),
                                    );
                                    if let Some(cam) = zoom.cameras.iter().next() {
                                        let actual = cam.radius.unwrap_or(cam.target_radius);
                                        let r = actual.round().max(0.0) as i32;
                                        widgets::stat_row(
                                            ui,
                                            &theme,
                                            "Zoom",
                                            format!("{r} voxel{}", if r == 1 { "" } else { "s" }),
                                        );
                                    }
                                });
                                // Color section — hero swatch opens the full
                                // picker popup (the only place to edit numerically;
                                // the color-space format lives in Preferences). Hex
                                // readout and recent colors fold in below so this is
                                // one compact block instead of three sections.
                                widgets::section(ui, &theme, "Color", |ui| {
                                    let srgba = egui::Color32::from_rgba_unmultiplied(
                                        color.0[0], color.0[1], color.0[2], color.0[3],
                                    );
                                    let swatch_w = ui.available_width();
                                    let swatch_resp = widgets::swatch_button(
                                        ui,
                                        &theme,
                                        srgba,
                                        egui::vec2(swatch_w, swatch::HERO_HEIGHT),
                                        radius::MD,
                                        false,
                                    )
                                    .on_hover_text("Click to edit color");
                                    let active_space = prefs.color_space;
                                    egui::Popup::menu(&swatch_resp)
                                        .close_behavior(
                                            egui::PopupCloseBehavior::CloseOnClickOutside,
                                        )
                                        .show(|ui| {
                                            color_picker::space_color_picker(
                                                ui,
                                                &mut color,
                                                active_space,
                                                &mut color_edit,
                                            );
                                        });

                                    ui.add_space(space::XS);
                                    // Readout for the active color in the chosen format.
                                    ui.vertical_centered(|ui| {
                                        ui.add(
                                            egui::Label::new(
                                                egui::RichText::new(
                                                    active_space.format([
                                                        color.0[0], color.0[1], color.0[2],
                                                    ]),
                                                )
                                                .monospace()
                                                .size(font::SMALL)
                                                .color(theme.text_dim),
                                            )
                                            .selectable(true),
                                        );
                                    });

                                    // Recent colors — unlabeled strip under the swatch.
                                    if !recent.0.is_empty() {
                                        ui.add_space(space::XS);
                                        widgets::swatch_grid(ui, |ui| {
                                            for c in &recent.0 {
                                                let col = egui::Color32::from_rgba_unmultiplied(
                                                    c[0], c[1], c[2], 255,
                                                );
                                                let select_state = if color.0 == *c {
                                                    widgets::SwatchSelect::Primary
                                                } else if extras.contains(*c) {
                                                    widgets::SwatchSelect::Extra
                                                } else {
                                                    widgets::SwatchSelect::None
                                                };
                                                let resp = widgets::swatch_cell(
                                                    ui,
                                                    &theme,
                                                    col,
                                                    [c[0], c[1], c[2]],
                                                    swatch::RECENT,
                                                    radius::XS,
                                                    select_state,
                                                    active_space,
                                                );
                                                if resp.clicked() {
                                                    let shift = ui.input(|i| i.modifiers.shift);
                                                    color.0 = apply_swatch_click(
                                                        shift,
                                                        *c,
                                                        color.0,
                                                        &mut extras,
                                                    );
                                                }
                                            }
                                        });
                                    }
                                });

                                if palette_choice.0 >= palettes.0.len() {
                                    palette_choice.0 = 0;
                                }

                                // Palette section — overflow … menu sits on the title row.
                                {
                                    let active_idx =
                                        palette_choice.0.min(palettes.0.len().saturating_sub(1));
                                    palette_choice.0 = active_idx;
                                    let active_is_builtin = palettes.0[active_idx].builtin;

                                    // Buffer so the rename field doesn't overflow and grow the panel.
                                    let row_w = (ui.available_width() - 2.0).max(80.0);

                                    widgets::section_header_action(ui, &theme, "Palette", |ui| {
                                        let menu_resp = widgets::icon_only_button(
                                            ui,
                                            &theme,
                                            icons::ellipsis(),
                                            true,
                                        )
                                        .on_hover_text("Palette actions");
                                        egui::Popup::menu(&menu_resp)
                                            .close_behavior(egui::PopupCloseBehavior::CloseOnClick)
                                            .show(|ui| {
                                                // Roomier menu items — wider horizontal
                                                // padding so labels breathe off both edges.
                                                ui.set_min_width(width::TOP_BAR_MENU);
                                                ui.spacing_mut().button_padding =
                                                    crate::ui::tokens::pad::MENU;
                                                if ui.button("Switch palette…").clicked() {
                                                    switcher.open_fresh();
                                                }
                                                ui.separator();
                                                if ui.button("New palette").clicked() {
                                                    working.clear();
                                                    let name =
                                                        palette::next_palette_name(&palettes.0);
                                                    palettes.0.push(Palette {
                                                        name,
                                                        colors: Vec::new(),
                                                        builtin: false,
                                                    });
                                                    let i = palettes.0.len() - 1;
                                                    palette_choice.0 = i;
                                                    palette_rename.editing = Some(i);
                                                    palette_rename.buf = palettes.0[i].name.clone();
                                                    io::palettes::save(&palettes.0);
                                                }
                                                let save_as_label = if active_is_builtin {
                                                    "Save as new palette"
                                                } else {
                                                    "Duplicate"
                                                };
                                                if ui.button(save_as_label).clicked() {
                                                    let i = palette::save_as_new(
                                                        &mut palettes,
                                                        &mut palette_choice,
                                                        &mut working,
                                                    );
                                                    palette_rename.editing = Some(i);
                                                    palette_rename.buf = palettes.0[i].name.clone();
                                                    io::palettes::save(&palettes.0);
                                                }
                                                if !active_is_builtin {
                                                    if ui.button("Rename…").clicked() {
                                                        palette_rename.editing = Some(active_idx);
                                                        palette_rename.buf =
                                                            palettes.0[active_idx].name.clone();
                                                    }
                                                    let has_user = palettes
                                                        .0
                                                        .iter()
                                                        .filter(|p| !p.builtin)
                                                        .count()
                                                        > 0;
                                                    if ui
                                                        .add_enabled(
                                                            has_user,
                                                            egui::Button::new("Delete"),
                                                        )
                                                        .clicked()
                                                    {
                                                        palettes.0.remove(active_idx);
                                                        if palette_choice.0 >= palettes.0.len() {
                                                            palette_choice.0 =
                                                                palettes.0.len().saturating_sub(1);
                                                        }
                                                        palette_rename.editing = None;
                                                        working.clear();
                                                        io::palettes::save(&palettes.0);
                                                    }
                                                }
                                                ui.separator();
                                                let dialog_busy = pending.is_active();
                                                let safe_idx = palette_choice
                                                    .0
                                                    .min(palettes.0.len().saturating_sub(1));
                                                let export_name = palettes.0[safe_idx].name.clone();
                                                let export_colors = palette::display_colors(
                                                    &palettes, &working, safe_idx,
                                                )
                                                .to_vec();
                                                let default_filename = format!(
                                                    "{}.ase",
                                                    palette::sanitize_filename(&export_name)
                                                );
                                                if ui
                                                    .add_enabled(
                                                        !dialog_busy,
                                                        egui::Button::new("Export .ase…"),
                                                    )
                                                    .clicked()
                                                {
                                                    let start_dir = prefs.last_dir.clone();
                                                    pending.spawn(async move {
                                                        dialogs::new_dialog(&start_dir)
                                                            .add_filter(
                                                                "Adobe Swatch Exchange",
                                                                &["ase"],
                                                            )
                                                            .set_file_name(&default_filename)
                                                            .save_file()
                                                            .await
                                                            .map(|f| {
                                                                DialogResult::ExportAse(
                                                                    f.path().to_path_buf(),
                                                                    export_name,
                                                                    export_colors,
                                                                )
                                                            })
                                                    });
                                                }
                                                if ui
                                                    .add_enabled(
                                                        !dialog_busy,
                                                        egui::Button::new("Import .ase…"),
                                                    )
                                                    .clicked()
                                                {
                                                    let start_dir = prefs.last_dir.clone();
                                                    pending.spawn(async move {
                                                        dialogs::new_dialog(&start_dir)
                                                            .add_filter(
                                                                "Adobe Swatch Exchange",
                                                                &["ase"],
                                                            )
                                                            .pick_file()
                                                            .await
                                                            .map(|f| {
                                                                DialogResult::ImportAse(
                                                                    f.path().to_path_buf(),
                                                                )
                                                            })
                                                    });
                                                }
                                            });
                                    });

                                    // Refresh — menu may have changed the selection.
                                    let active_idx =
                                        palette_choice.0.min(palettes.0.len().saturating_sub(1));
                                    palette_choice.0 = active_idx;
                                    let active_is_builtin = palettes.0[active_idx].builtin;

                                    if palette_rename.editing == Some(active_idx)
                                        && !active_is_builtin
                                    {
                                        ui.add_space(space::XS);
                                        ui.horizontal(|ui| {
                                            ui.spacing_mut().item_spacing.x = gap::TIGHT.x;
                                            let resp = ui.add(
                                                egui::TextEdit::singleline(&mut palette_rename.buf)
                                                    .desired_width((row_w - 76.0).max(60.0)),
                                            );
                                            if !resp.has_focus() && !resp.lost_focus() {
                                                resp.request_focus();
                                            }
                                            let commit = widgets::icon_only_button(
                                                ui,
                                                &theme,
                                                icons::check(),
                                                true,
                                            )
                                            .on_hover_text("Save name")
                                            .clicked()
                                                || (resp.lost_focus()
                                                    && ui.input(|i| {
                                                        i.key_pressed(egui::Key::Enter)
                                                    }));
                                            let cancel = widgets::icon_only_button(
                                                ui,
                                                &theme,
                                                icons::x(),
                                                true,
                                            )
                                            .on_hover_text("Cancel")
                                            .clicked()
                                                || ui.input(|i| i.key_pressed(egui::Key::Escape));
                                            if commit {
                                                let trimmed = palette_rename.buf.trim();
                                                if !trimmed.is_empty() {
                                                    palettes.0[active_idx].name =
                                                        trimmed.to_string();
                                                    io::palettes::save(&palettes.0);
                                                }
                                                palette_rename.editing = None;
                                                palette_rename.buf.clear();
                                            } else if cancel {
                                                palette_rename.editing = None;
                                                palette_rename.buf.clear();
                                            }
                                        });
                                    }

                                    let active_colors =
                                        palette::display_colors(&palettes, &working, active_idx)
                                            .to_vec();

                                    let add_enabled = !active_colors.contains(&color.0);
                                    let mut add_clicked = false;
                                    let mut reorder: Option<(usize, usize)> = None;
                                    let mut remove_idx: Option<usize> = None;

                                    // Manual grid layout: each cell is painted at a computed
                                    // rect and interacts via `ui.interact`, instead of letting
                                    // egui flow the swatches. That lets a dragged swatch leave
                                    // its slot so the rest shift to open a gap at the drop
                                    // target — the "make space" feedback. Fixed rows also dodge
                                    // the wrap bug where click-and-drag cells refuse to wrap in
                                    // `horizontal_wrapped`.
                                    let n = active_colors.len();
                                    let cell = swatch::PALETTE;
                                    let stepx = cell.x + gap::TIGHT.x;
                                    let stepy = cell.y + gap::TIGHT.y;
                                    let avail = ui.available_width();
                                    let cols =
                                        ((avail + gap::TIGHT.x) / stepx).floor().max(1.0) as usize;
                                    let total = n + 1; // colours + trailing `+` cell
                                    let rows = total.div_ceil(cols);
                                    let grid_h = (rows as f32 * stepy - gap::TIGHT.y).max(cell.y);

                                    // Drag source set on a previous frame (egui clears the dnd
                                    // payload on pointer release), clamped to a live index.
                                    let dragging = egui::DragAndDrop::payload::<usize>(ui.ctx())
                                        .map(|p| *p)
                                        .filter(|&d| d < n);

                                    let (grid_rect, _) = ui.allocate_exact_size(
                                        egui::vec2(avail, grid_h),
                                        egui::Sense::hover(),
                                    );
                                    let origin = grid_rect.min;
                                    let cell_at = |slot: usize| -> egui::Rect {
                                        let r = slot / cols;
                                        let c = slot % cols;
                                        egui::Rect::from_min_size(
                                            origin + egui::vec2(c as f32 * stepx, r as f32 * stepy),
                                            cell,
                                        )
                                    };
                                    // Fractional slot → rect, lerping between the two bracketing
                                    // integer cells so swatches slide (and wrap diagonally at row
                                    // ends) while the reorder gap moves under the cursor.
                                    let step = egui::vec2(stepx, stepy);
                                    let cell_at_f = |slot_f: f32| -> egui::Rect {
                                        egui::Rect::from_min_size(
                                            slot_min_lerp(origin, cols, step, slot_f),
                                            cell,
                                        )
                                    };

                                    // Non-dragged swatches in order; while dragging, a gap is
                                    // opened at `gap_pos` (an index into this list) under the
                                    // pointer.
                                    let others: Vec<usize> =
                                        (0..n).filter(|&i| Some(i) != dragging).collect();
                                    let pointer = ui.ctx().pointer_interact_pos();
                                    let gap_pos: Option<usize> = if dragging.is_some() {
                                        pointer.map(|p| {
                                            let rel = p - origin;
                                            let c = ((rel.x / stepx) + 0.5)
                                                .floor()
                                                .clamp(0.0, cols as f32);
                                            let r = (rel.y / stepy)
                                                .floor()
                                                .clamp(0.0, (rows as f32 - 1.0).max(0.0));
                                            ((r as usize) * cols + c as usize).min(others.len())
                                        })
                                    } else {
                                        None
                                    };

                                    enum Slot {
                                        Color(usize),
                                        Gap,
                                        Add,
                                    }
                                    let mut slots: Vec<Slot> = Vec::with_capacity(total + 1);
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

                                    let base = ui.id();
                                    let color_fmt = prefs.color_space;
                                    for (slot_idx, slot) in slots.iter().enumerate() {
                                        // Animate each cell from its old slot to its new one so
                                        // the grid reflows around the moving gap. The gap itself
                                        // paints nothing — it reads as open space the swatches
                                        // slide away from.
                                        let target = slot_idx as f32;
                                        let anim_id = match slot {
                                            Slot::Color(oi) => base.with(("swatch_slot", *oi)),
                                            Slot::Add => base.with("add_slot"),
                                            Slot::Gap => base.with("gap_slot"),
                                        };
                                        let rect = cell_at_f(ui.ctx().animate_value_with_time(
                                            anim_id,
                                            target,
                                            motion::SWATCH_REFLOW,
                                        ));
                                        match slot {
                                            Slot::Gap => {}
                                            Slot::Add => {
                                                let resp = ui.interact(
                                                    rect,
                                                    base.with("palette_add"),
                                                    egui::Sense::click(),
                                                );
                                                widgets::paint_add_swatch(
                                                    ui,
                                                    &theme,
                                                    rect,
                                                    radius::XS,
                                                    add_enabled,
                                                    resp.hovered(),
                                                );
                                                let resp = resp.on_hover_text(if add_enabled {
                                                    "Add current color"
                                                } else {
                                                    "Color already in palette"
                                                });
                                                if add_enabled {
                                                    if resp.clicked() {
                                                        add_clicked = true;
                                                    }
                                                    resp.on_hover_cursor(
                                                        egui::CursorIcon::PointingHand,
                                                    );
                                                }
                                            }
                                            Slot::Color(oi) => {
                                                let si = *oi;
                                                let c = active_colors[si];
                                                let col = egui::Color32::from_rgba_unmultiplied(
                                                    c[0], c[1], c[2], 255,
                                                );
                                                let select_state = if color.0 == c {
                                                    widgets::SwatchSelect::Primary
                                                } else if extras.contains(c) {
                                                    widgets::SwatchSelect::Extra
                                                } else {
                                                    widgets::SwatchSelect::None
                                                };
                                                let resp = widgets::swatch_cell_at(
                                                    ui,
                                                    &theme,
                                                    base.with(("palette_swatch", si)),
                                                    rect,
                                                    col,
                                                    [c[0], c[1], c[2]],
                                                    radius::XS,
                                                    select_state,
                                                    color_fmt,
                                                );
                                                if resp.clicked() {
                                                    let shift = ui.input(|i| i.modifiers.shift);
                                                    color.0 = apply_swatch_click(
                                                        shift,
                                                        c,
                                                        color.0,
                                                        &mut extras,
                                                    );
                                                }
                                                if resp.dragged() {
                                                    egui::DragAndDrop::set_payload(ui.ctx(), si);
                                                }
                                                egui::Popup::context_menu(&resp).show(|ui| {
                                                    if ui.button("Remove").clicked() {
                                                        remove_idx = Some(si);
                                                        ui.close();
                                                    }
                                                });
                                            }
                                        }
                                    }

                                    // The dragged swatch follows the cursor as a lifted ghost;
                                    // its interaction id lives here now (it left its grid slot),
                                    // so the release is detected and committed against the gap.
                                    if let Some(from) = dragging {
                                        let c = active_colors[from];
                                        let col = egui::Color32::from_rgba_unmultiplied(
                                            c[0], c[1], c[2], 255,
                                        );
                                        let ghost_center =
                                            pointer.unwrap_or_else(|| cell_at(0).center());
                                        let ghost =
                                            egui::Rect::from_center_size(ghost_center, cell);
                                        ui.painter().rect_filled(
                                            ghost.translate(egui::vec2(0.0, 2.0)),
                                            egui::CornerRadius::same(radius::XS),
                                            egui::Color32::from_black_alpha(70),
                                        );
                                        ui.painter().rect(
                                            ghost,
                                            egui::CornerRadius::same(radius::XS),
                                            col,
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
                                            reorder = Some((from, to));
                                        }
                                    }

                                    if add_clicked {
                                        let (colors, persist) = palette::edit_colors(
                                            &mut palettes,
                                            &mut working,
                                            active_idx,
                                        );
                                        colors.push(color.0);
                                        if persist {
                                            io::palettes::save(&palettes.0);
                                        }
                                    }
                                    if let Some(i) = remove_idx {
                                        let (colors, persist) = palette::edit_colors(
                                            &mut palettes,
                                            &mut working,
                                            active_idx,
                                        );
                                        if i < colors.len() {
                                            colors.remove(i);
                                        }
                                        if persist {
                                            io::palettes::save(&palettes.0);
                                        }
                                    }
                                    if let Some((from, to)) = reorder {
                                        let (colors, persist) = palette::edit_colors(
                                            &mut palettes,
                                            &mut working,
                                            active_idx,
                                        );
                                        if from < colors.len() && to < colors.len() {
                                            let c = colors.remove(from);
                                            colors.insert(to, c);
                                            if persist {
                                                io::palettes::save(&palettes.0);
                                            }
                                        }
                                    }

                                    widgets::section_divider(ui, &theme);
                                }

                                // Shape section — real-time info while drawing.
                                if tool.current == Tool::Shape
                                    && shape_state.phase.is_some()
                                    && let (Some(anchor), Some(c1), Some(c2)) = (
                                        shape_state.anchor,
                                        shape_state.corner1,
                                        shape_state.corner2,
                                    )
                                {
                                    let cells = crate::shapes::compute_shape_cells(
                                        shape_options.primitive,
                                        c1,
                                        c2,
                                        anchor.axis,
                                        shape_state.thickness,
                                        shape_state.normal_sign,
                                    );
                                    if let Some(bounds) = crate::shapes::cell_bounds(&cells) {
                                        widgets::section(ui, &theme, "Shape", |ui| {
                                            widgets::stat_row(
                                                ui,
                                                &theme,
                                                "Bounds",
                                                format!(
                                                    "{} × {} × {}",
                                                    bounds.x, bounds.y, bounds.z
                                                ),
                                            );
                                            widgets::stat_row(
                                                ui,
                                                &theme,
                                                "Voxels",
                                                cells.len().to_string(),
                                            );
                                        });
                                    }
                                }

                                // Selection section — committed or in-progress.
                                let selection_aabb = selection.aabb.or_else(|| {
                                    if select_state.phase != crate::select::SelectPhase::Idle {
                                        crate::select::in_progress_aabb(&select_state)
                                    } else {
                                        None
                                    }
                                });
                                if let Some(aabb) = selection_aabb {
                                    widgets::section(ui, &theme, "Selection", |ui| {
                                        let extents = aabb.extents();
                                        widgets::stat_row(
                                            ui,
                                            &theme,
                                            "Bounds",
                                            format!(
                                                "{} × {} × {}",
                                                extents.x, extents.y, extents.z
                                            ),
                                        );
                                        let voxel_count = if selection.aabb.is_some() {
                                            selection.voxel_count(&grid)
                                        } else {
                                            aabb.voxel_count(&grid)
                                        };
                                        widgets::stat_row(
                                            ui,
                                            &theme,
                                            "Voxels",
                                            voxel_count.to_string(),
                                        );
                                    });
                                }
                            });
                        });
                }),
        )
    } else {
        None
    };
    if let Some(resp) = inspector_resp {
        let rect = resp.response.rect;
        onboarding_anchors.color_palette = Some(rect);
        // Background order so floating windows (command palette, palette switcher
        // — both `egui::Window` at `Order::Middle`) render above the edge line
        // instead of having it slice across them.
        let painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Background,
            egui::Id::new("inspector_panel_edge"),
        ));
        painter.vline(
            painter.round_to_pixel_center(rect.right()),
            rect.y_range(),
            egui::Stroke::new(stroke::HAIR, BORDER),
        );
    }

    // Reflect tool in cursor when pointer is over the viewport.
    if !ctx.is_pointer_over_area() {
        let alt = keys.any_pressed([KeyCode::AltLeft, KeyCode::AltRight]);
        let z = keys.pressed(KeyCode::KeyZ);
        let over_gizmo = gizmo_view.drag.active
            || gizmo_view
                .rect
                .0
                .zip(ctx.pointer_latest_pos())
                .is_some_and(|(r, p)| {
                    p.x >= r.min.x && p.x <= r.max.x && p.y >= r.min.y && p.y <= r.max.y
                });
        let cursor = if gizmo_view.drag.active {
            egui::CursorIcon::Grabbing
        } else if over_gizmo {
            egui::CursorIcon::Grab
        } else if mouse.pressed(MouseButton::Right) {
            egui::CursorIcon::Grabbing
        } else if z {
            if alt {
                egui::CursorIcon::ZoomOut
            } else {
                egui::CursorIcon::ZoomIn
            }
        } else if keys.pressed(KeyCode::Space) {
            if mouse.pressed(MouseButton::Left) {
                egui::CursorIcon::Grabbing
            } else {
                egui::CursorIcon::Grab
            }
        } else if alt {
            egui::CursorIcon::PointingHand
        } else {
            egui::CursorIcon::Crosshair
        };
        ctx.set_cursor_icon(cursor);
    }

    // Scrim sits at Order::Middle — above the canvas + inspector, below the
    // Foreground modal surfaces drawn just after. `modal_open` and the hidden
    // tool island / gizmo were handled near the top of the function.
    if modal_open {
        widgets::modal_scrim(ctx);
    }

    if prefs_window.open {
        modals::draw_preferences(ctx, &theme, &mut prefs, &mut prefs_window);
    }

    // Only a modified document earns the discard confirm; a clean New applies
    // silently via `auto_apply_clean_new_project_system`. Gating the draw here
    // too means no one-frame confirm flash whatever the system order.
    if new_project.dialog_open && doc.is_modified(&history) {
        modals::draw_new_project(ctx, &theme, &mut new_project);
    }

    // Open-project guard: confirm before an unsaved document is replaced.
    if open_request.confirming {
        match modals::draw_open_confirm(ctx, &theme) {
            Some(true) => {
                open_request.confirming = false;
                dialogs::spawn_open(&mut pending, prefs.last_dir.clone());
            }
            Some(false) => open_request.confirming = false,
            None => {}
        }
    }

    // Command-palette-style palette switcher (opened from the … menu).
    if let Some(target) = palette_switcher::draw(ctx, &theme, &mut switcher, &palettes.0) {
        palette::request_select(target, &mut palette_choice, &mut working, &mut discard);
    }

    // Discard-edits confirm: switching away from a dirty built-in stages a
    // target index here so the user can keep their scratch edits first.
    if let Some(target) = discard.pending {
        modals::draw_discard(
            ctx,
            &theme,
            target,
            &mut palettes,
            &mut palette_choice,
            &mut working,
            &mut discard,
            &mut palette_rename,
        );
    }

    if cmd_palette.open {
        let state = command_palette::CatalogState {
            tool: tool.current,
            shape: &shape_options,
            has_undo: !history.undo.is_empty(),
            has_redo: !history.redo.is_empty(),
            has_selection: selection.aabb.is_some(),
            has_clipboard: clipboard.has_stamp(),
            has_voxels: grid.count() > 0,
            dialog_busy: pending.is_active(),
            palettes: &palettes.0,
            palette_choice: palette_choice.0,
            current_color: color.0,
            prefs: &prefs,
            flyby_active: flyby.active,
        };
        let catalog = command_palette::build_catalog(&state);
        command_palette::draw(ctx, &theme, &mut cmd_palette, &catalog);
    }

    toast::draw_toasts(ctx, &theme, &toasts);

    Ok(())
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
}
