use crate::GridResource;
use crate::actions::{ActionQueue, ActionState, Mods, Origin};
mod color_picker;
mod command_palette;
mod dialogs;
mod floating;
pub(crate) mod icons;
mod inspector;
mod modals;
mod palette;
mod palette_switcher;
mod search_popover;
mod shortcuts;
pub mod toast;
pub mod tokens;
mod visibility;
mod widgets;

pub use command_palette::CommandPalette;
pub use dialogs::{
    CurrentProjectPath, DialogResult, DocStatus, GuardedAction, PendingDialog, PendingImport,
    RecentFiles, SavePreviewState, UnsavedGuard, poll_dialogs_system, process_save_preview_system,
    spawn_open,
};
pub use dialogs::{spawn_export, spawn_export_ase, spawn_import, spawn_save, spawn_save_as};
pub use palette::{
    DeleteConfirm, DiscardConfirm, Palette, PaletteChoice, PaletteRenameState, PaletteSwitcher,
    Palettes, WorkingPalette, display_colors, edit_colors, next_palette_name, request_select,
    save_as_new,
};
pub use shortcuts::ShortcutsWindow;
pub use toast::{Toasts, toast_lifetime_system};
pub use visibility::UiVisible;

use crate::gizmo::{GizmoDrag, GizmoRect};
use crate::onboarding::OnboardingAnchors;
use crate::theme::{Preferences, PreferencesWindow, Theme, apply_egui_style};
use crate::tools::{CurrentColor, ExtraColors, RecentColors, ShapeOptions, ShapeState, ToolState};
use crate::ui::tokens::stroke;
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};
use bevy_panorbit_camera::PanOrbitCamera;
use palette::PaletteParams;
use roxel::history::History;

/// `true` on any frame a modal/palette is open. Set by `ui_system`, read by
/// `gizmo::update_gizmo_viewport` to deactivate the orientation cube — its
/// camera composites after egui, so the modal scrim can't cover it otherwise.
#[derive(Resource, Default)]
pub struct ModalActive(pub bool);

/// The pointer belongs to egui: over a panel, or mid-drag on a widget.
pub fn egui_wants_pointer(contexts: &mut EguiContexts) -> bool {
    contexts
        .ctx_mut()
        .is_ok_and(|c| pointer_over_ui(c) || c.egui_is_using_pointer())
}

/// The pointer is over egui: a window or popup, or off the canvas (the
/// inspector). bevy_egui runs the primary pass through `run_ui` with an unused
/// root `Ui`, so `is_pointer_over_egui` alone treats background-layer panels
/// as canvas.
pub fn pointer_over_ui(ctx: &egui::Context) -> bool {
    ctx.is_pointer_over_egui()
        || ctx
            .input(|i| i.pointer.interact_pos())
            .is_some_and(|p| !canvas_rect(ctx).contains(p))
}

/// A text field has keyboard focus.
pub fn egui_wants_keyboard(contexts: &mut EguiContexts) -> bool {
    contexts
        .ctx_mut()
        .is_ok_and(|c| c.egui_wants_keyboard_input())
}

fn canvas_rect_id() -> egui::Id {
    egui::Id::new("roxel_canvas_rect")
}

/// Records the canvas beside the inspector. Stands in for egui's removed
/// `Context::available_rect`.
pub fn set_canvas_rect(ctx: &egui::Context, rect: egui::Rect) {
    ctx.data_mut(|d| d.insert_temp(canvas_rect_id(), rect));
}

/// The canvas beside the inspector, as of the latest `ui_system` pass.
pub fn canvas_rect(ctx: &egui::Context) -> egui::Rect {
    ctx.data(|d| d.get_temp(canvas_rect_id()))
        .unwrap_or_else(|| ctx.content_rect())
}

#[derive(SystemParam)]
pub struct ZoomReadout<'w, 's> {
    pub cameras: Query<'w, 's, &'static PanOrbitCamera>,
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
    pub guard: ResMut<'w, UnsavedGuard>,
    pub selection: ResMut<'w, crate::select::Selection>,
    pub selection_voxels: Res<'w, crate::select::SelectionVoxelCount>,
    pub select_state: Res<'w, crate::select::SelectState>,
    pub shape_state: Res<'w, ShapeState>,
    pub toasts: Res<'w, Toasts>,
    pub current_path: Res<'w, CurrentProjectPath>,
    pub doc: Res<'w, DocStatus>,
    pub flyby: Res<'w, crate::camera::FlybyState>,
    pub color_edit: ResMut<'w, roxel::color_space::ColorEditBuffer>,
    pub updater: Res<'w, crate::updater::UpdateCheck>,
    pub clipboard: Res<'w, crate::clipboard::Clipboard>,
    pub onboarding_anchors: ResMut<'w, OnboardingAnchors>,
    pub ui_visible: Res<'w, UiVisible>,
    pub shot_panel: ResMut<'w, crate::shot::ShotPanel>,
    pub actions: ResMut<'w, ActionQueue>,
    pub recent_files: Res<'w, RecentFiles>,
    pub about: ResMut<'w, crate::menu::AboutWindow>,
    pub shortcuts: ResMut<'w, ShortcutsWindow>,
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
    pub palette_params: PaletteParams<'w>,
    pub prefs_params: PrefsParams<'w>,
    pub input: UiInput<'w>,
    pub zoom: ZoomReadout<'w, 's>,
    pub gizmo_view: GizmoView<'w>,
    pub ui_state: UiState<'w>,
}

pub fn ui_system(
    mut contexts: EguiContexts,
    mut grid: ResMut<GridResource>,
    history: Res<History>,
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
        mut guard,
        selection,
        selection_voxels,
        select_state,
        shape_state,
        toasts,
        current_path,
        doc,
        flyby,
        mut color_edit,
        updater,
        clipboard,
        mut onboarding_anchors,
        ui_visible,
        mut shot_panel,
        mut actions,
        recent_files,
        mut about,
        mut shortcuts,
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
        mut delete,
        mut switcher,
    } = palette_params;
    let PrefsParams {
        mut prefs,
        window: mut prefs_window,
    } = prefs_params;
    crate::theme::apply_motion_style(ctx, &prefs);

    // True whenever a modal/palette is open. The scrim dims the canvas +
    // inspector behind the modal; the floating tool island, menu pill, and the
    // gizmo (`ModalActive`, see `gizmo.rs`) are hidden outright rather than
    // dimmed, since they render above the Middle scrim and can't be covered by
    // it.
    let modal_open = prefs_window.open
        || guard.confirming.is_some()
        || switcher.open
        || discard.pending.is_some()
        || delete.pending.is_some()
        || shot_panel.open
        || about.0
        || shortcuts.0
        || cmd_palette.open;
    modal_active.0 = modal_open;

    // Snapshot for the menus and the command palette, rebuilt where it's used
    // because the surfaces in between mutate prefs and palettes.
    macro_rules! action_state {
        () => {
            ActionState {
                tool: tool.current,
                shape: shape_options.primitive,
                has_undo: !history.undo.is_empty(),
                has_redo: !history.redo.is_empty(),
                stroke_open: history.is_open(),
                has_selection: selection.aabb.is_some(),
                selection_idle: select_state.drag.is_idle(),
                has_clipboard: clipboard.has_stamp(),
                has_voxels: grid.count() > 0,
                dialog_busy: pending.is_active(),
                flyby_active: flyby.active,
                focus_mode: !ui_visible.0,
                checking_updates: updater.is_checking(),
                update_tag: updater.available().map(|r| r.tag.as_str()),
                palettes: &palettes.0,
                palette_choice: palette_choice.0.min(palettes.0.len().saturating_sub(1)),
                active_colors: palette::display_colors(
                    &palettes,
                    &working,
                    palette_choice.0.min(palettes.0.len().saturating_sub(1)),
                ),
                current_color: color.0,
                prefs: &prefs,
                recent: &recent_files.0,
            }
        };
    }

    // ---------- Floating menu pill ----------
    // On macOS the native menu bar (see `menu.rs`) replaces these controls; on
    // Win/Linux the pill sits at top-center and is gated by the user pref.
    #[cfg(not(target_os = "macos"))]
    if ui_visible.0 && prefs.show_floating_menu_bar && !modal_open {
        let state = action_state!();
        floating::pill_menu(ctx, &theme, |ui| {
            floating::pill_menu_contents(ui, &theme, &state, &mut actions);
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

    let bounds = grid.bypass_change_detection().bounding_box_cached();
    let palette_menu = inspector::palette_menu_rows(&action_state!());
    let inspector_resp = inspector::inspector_panel(
        ctx,
        ui_visible.0,
        &theme,
        inspector::Inspector {
            grid: &grid,
            bounds,
            history: &history,
            zoom: &zoom,
            current_path: &current_path,
            doc: &doc,
            color: &mut color,
            color_edit: &mut color_edit,
            prefs: &prefs,
            recent: &recent,
            extras: &mut extras,
            palettes: &mut palettes,
            palette_choice: &mut palette_choice,
            palette_rename: &mut palette_rename,
            working: &mut working,
            palette_menu: &palette_menu,
            actions: &mut actions,
            tool: &tool,
            shape_options: &shape_options,
            shape_state: &shape_state,
            selection: &selection,
            select_state: &select_state,
            selection_voxels: selection_voxels.0,
        },
    );
    if let Some(resp) = inspector_resp {
        let rect = resp.response.rect;
        onboarding_anchors.color_palette = Some(rect);
        // On the panel's own background layer, so floating windows (command
        // palette, palette switcher — `Order::Middle`) render above the edge
        // line instead of having it slice across them.
        let painter = ctx.layer_painter(resp.response.layer_id);
        painter.vline(
            painter.round_to_pixel_center(rect.right()),
            rect.y_range(),
            egui::Stroke::new(stroke::HAIR, theme.border),
        );
    }

    // Reflect tool in cursor when pointer is over the viewport.
    if !pointer_over_ui(ctx) {
        let alt = Mods::from_keys(&keys).alt;
        let z = keys.pressed(KeyCode::KeyZ);
        let over_gizmo = gizmo_view.drag.active
            || gizmo_view
                .rect
                .0
                .zip(ctx.pointer_latest_pos())
                .is_some_and(|(r, p)| {
                    let p = p * prefs.ui_zoom();
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

    if let Some(action) = guard.confirming.clone() {
        let doc_name = current_path
            .0
            .as_ref()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .unwrap_or("Untitled");
        if let Some(choice) = modals::draw_unsaved_confirm(ctx, &theme, &action, doc_name)
            && guard.resolve_confirm(choice)
        {
            dialogs::spawn_save(&mut pending, &current_path, prefs.last_dir.clone());
        }
    }

    // Command-palette-style palette switcher (opened from the … menu).
    if let Some(target) = palette_switcher::draw(ctx, &theme, &mut switcher, &palettes.0) {
        palette::request_select(target, &mut palette_choice, &mut working, &mut discard);
    }

    if let Some(idx) = delete.pending {
        let name = palettes
            .0
            .get(idx)
            .map(|p| p.name.clone())
            .unwrap_or_default();
        match modals::draw_delete_palette(ctx, &theme, &name) {
            Some(true) => {
                delete.pending = None;
                if palette::delete_palette(
                    &mut palettes,
                    &mut palette_choice,
                    &mut working,
                    &mut palette_rename,
                    idx,
                ) {
                    roxel::io::palettes::save(&palettes.0);
                }
            }
            Some(false) => delete.pending = None,
            None => {}
        }
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

    // Export-Shot tweak panel: live preview + art-direction knobs. Opened from
    // the command palette / File → Export; "Export…" spawns the save dialog.
    if shot_panel.open {
        modals::draw_shot_panel(
            ctx,
            &theme,
            &mut shot_panel,
            &mut pending,
            &prefs.last_dir,
            prefs.color_space,
        );
    }

    if about.0 && modals::draw_about(ctx, &theme) {
        about.0 = false;
    }

    if shortcuts.0 && shortcuts::draw(ctx, &theme) {
        shortcuts.0 = false;
    }

    if cmd_palette.open {
        let state = action_state!();
        let catalog = command_palette::build_catalog(&state);
        if let Some(action) = command_palette::draw(ctx, &theme, &mut cmd_palette, &catalog) {
            actions.push(action, Origin::Ui);
        }
    }

    toast::draw_toasts(ctx, &theme, &toasts);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canvas_rect_falls_back_to_the_content_rect_until_recorded() {
        let ctx = egui::Context::default();
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(800.0, 600.0),
            )),
            ..Default::default()
        };
        ctx.run_ui(input, |ui| {
            let ctx = ui.ctx();
            assert_eq!(canvas_rect(ctx), ctx.content_rect());
            let canvas = egui::Rect::from_min_max(egui::pos2(240.0, 0.0), egui::pos2(800.0, 600.0));
            set_canvas_rect(ctx, canvas);
            assert_eq!(canvas_rect(ctx), canvas);
        })
        .drop_without_applying_deltas();
    }
}
