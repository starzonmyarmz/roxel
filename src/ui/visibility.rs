//! "Focus mode" — Tab toggles every chrome element (panels, floating pill,
//! tool island, status chip) so the canvas takes the whole window. Toasts and
//! modals still render so error feedback isn't trapped behind the toggle.

use bevy::prelude::*;
use bevy_egui::EguiContexts;

/// `true` when chrome is drawn. Default `true`. Tab flips it.
#[derive(Resource)]
pub struct UiVisible(pub bool);

impl Default for UiVisible {
    fn default() -> Self {
        Self(true)
    }
}

/// Toggle [`UiVisible`] on backtick (`` ` ``). Backtick avoids conflict with
/// egui's Tab focus-traversal. Still gates on `wants_keyboard_input` so the
/// key types literally into a focused text field instead of toggling chrome.
pub fn tab_toggle_system(
    keys: Res<ButtonInput<KeyCode>>,
    mut contexts: EguiContexts,
    mut visible: ResMut<UiVisible>,
) -> Result {
    if !keys.just_pressed(KeyCode::Backquote) {
        return Ok(());
    }
    let ctx = contexts.ctx_mut()?;
    if ctx.wants_keyboard_input() {
        return Ok(());
    }
    visible.0 = !visible.0;
    Ok(())
}
