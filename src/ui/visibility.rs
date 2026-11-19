//! Focus mode: `Action::ToggleFocusMode` (backquote) hides every chrome
//! element (inspector, floating pill, tool island) so the canvas takes the
//! whole window. Toasts and modals still render so error feedback isn't
//! trapped behind the toggle.

use bevy::prelude::*;

/// `true` when chrome is drawn. Default `true`.
#[derive(Resource)]
pub struct UiVisible(pub bool);

impl Default for UiVisible {
    fn default() -> Self {
        Self(true)
    }
}
