use crate::GridResource;
mod color_picker;
mod command_palette;
mod dialogs;
mod floating;
pub(crate) mod icons;
mod inspector;
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
    RecentFiles, SavePreviewState, poll_dialogs_system, process_save_preview_system, spawn_open,
};
// Re-exported only for the macOS native menu (`menu.rs`); the Win/Linux pill
// reaches these through the `dialogs` module path directly.
#[cfg(target_os = "macos")]
pub use dialogs::{spawn_export, spawn_import, spawn_save, spawn_save_as};
pub use palette::{DiscardConfirm, PaletteChoice, PaletteSwitcher, Palettes, WorkingPalette};
pub use toast::{Toasts, toast_lifetime_system};
pub use visibility::{UiVisible, tab_toggle_system};

use crate::gizmo::{GizmoDrag, GizmoRect};
use crate::onboarding::{Onboarding, OnboardingAnchors};
use crate::theme::{Preferences, PreferencesWindow, Theme, apply_egui_style};
use crate::tools::{CurrentColor, ExtraColors, RecentColors, ShapeOptions, ShapeState, ToolState};
use crate::ui::tokens::stroke;
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};
use bevy_panorbit_camera::PanOrbitCamera;
use palette::PaletteParams;
use roxel::grid::NewProject;
use roxel::history::History;

/// `true` on any frame a modal/palette is open. Set by `ui_system`, read by
/// `gizmo::update_gizmo_viewport` to deactivate the orientation cube — its
/// camera composites after egui, so the modal scrim can't cover it otherwise.
#[derive(Resource, Default)]
pub struct ModalActive(pub bool);

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
    pub new_project: ResMut<'w, NewProject>,
    pub selection: ResMut<'w, crate::select::Selection>,
    pub select_state: Res<'w, crate::select::SelectState>,
    pub shape_state: Res<'w, ShapeState>,
    pub toasts: Res<'w, Toasts>,
    pub current_path: Res<'w, CurrentProjectPath>,
    pub doc: Res<'w, DocStatus>,
    pub open_request: ResMut<'w, OpenRequest>,
    pub flyby: Res<'w, crate::camera::FlybyState>,
    pub color_edit: ResMut<'w, roxel::color_space::ColorEditBuffer>,
    pub updater: ResMut<'w, crate::updater::UpdateCheck>,
    pub clipboard: Res<'w, crate::clipboard::Clipboard>,
    pub onboarding: ResMut<'w, Onboarding>,
    pub onboarding_anchors: ResMut<'w, OnboardingAnchors>,
    pub ui_visible: Res<'w, UiVisible>,
    pub shot_panel: ResMut<'w, crate::shot::ShotPanel>,
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
    #[cfg_attr(target_os = "macos", allow(unused_mut))] mut grid: ResMut<GridResource>,
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
        mut shot_panel,
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
        || shot_panel.open
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
                &mut shot_panel,
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

    let inspector_resp = inspector::inspector_panel(
        ctx,
        ui_visible.0,
        &theme,
        &mut grid,
        &history,
        &zoom,
        &current_path,
        &doc,
        &mut color,
        &mut color_edit,
        &prefs,
        &recent,
        &mut extras,
        &mut palettes,
        &mut palette_choice,
        &mut palette_rename,
        &mut working,
        &mut switcher,
        &mut pending,
        &tool,
        &shape_options,
        &shape_state,
        &selection,
        &select_state,
    );
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
            egui::Stroke::new(stroke::HAIR, theme.border),
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
