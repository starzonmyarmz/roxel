//! Design tokens — every spacing, padding, font, radius, and icon size in the
//! UI resolves to a value here. Scales are 4-px grid, all even, no fonts
//! below 12 pt.
//!
//! Pick by semantic name, not raw value. If a new use site doesn't fit any
//! existing token, add a token rather than inlining a number.

use bevy_egui::egui::Vec2;

/// Font sizes (proportional + monospace). `BODY` doubles as the mono size so
/// hex strings and stat values match adjacent body text.
pub mod font {
    pub const SMALL: f32 = 12.0;
    pub const BODY: f32 = 14.0;
    pub const HEADING: f32 = 16.0;
}

/// Corner radii. `u8` to feed `egui::CornerRadius::same(...)` directly.
pub mod radius {
    pub const XS: u8 = 4; // small inner bits (swatch corners, toast accent bar)
    pub const SM: u8 = 6; // buttons, tool buttons, command palette rows
    pub const MD: u8 = 8; // menus, popups, hero swatch
    pub const LG: u8 = 12; // modal windows
}

/// Scalar gaps for `ui.add_space(...)`. 4-px grid.
pub mod space {
    pub const XS: f32 = 4.0;
    pub const SM: f32 = 8.0;
    pub const MD: f32 = 12.0;
    #[allow(dead_code)] // reserved for layout tuning
    pub const LG: f32 = 16.0;
}

/// `item_spacing` Vec2s. Used inside `ui.scope` blocks that need to override
/// the global `style.spacing.item_spacing`.
pub mod gap {
    use super::Vec2;
    pub const NONE: Vec2 = Vec2::new(0.0, 0.0);
    pub const TIGHT: Vec2 = Vec2::new(4.0, 4.0); // swatch grid
    pub const DEFAULT: Vec2 = Vec2::new(8.0, 8.0); // matches global default
}

/// `button_padding` Vec2s. `DEFAULT` matches the global style; the others are
/// scoped overrides for buttons that need a different inner padding.
pub mod pad {
    use super::Vec2;
    pub const NONE: Vec2 = Vec2::new(0.0, 0.0); // icon-only buttons
    pub const ICON: Vec2 = Vec2::new(8.0, 0.0); // wide_action_button row
    pub const BUTTON: Vec2 = Vec2::new(12.0, 4.0); // chip_button
    pub const DEFAULT: Vec2 = Vec2::new(12.0, 8.0); // global default
    pub const DIALOG: Vec2 = Vec2::new(16.0, 8.0); // modal action buttons
}

/// Icon `fit_to_exact_size` values. Square — width == height. 4-px grid.
pub mod icon {
    use super::Vec2;
    pub const SM: f32 = 14.0; // command-palette footer + wide-action icon
    pub const MD: f32 = 16.0; // top-bar, dialog icons
    pub const LG: f32 = 20.0; // left tool rail

    pub const fn sm_square() -> Vec2 {
        Vec2::new(SM, SM)
    }
    pub const fn md_square() -> Vec2 {
        Vec2::new(MD, MD)
    }
    pub const fn lg_square() -> Vec2 {
        Vec2::new(LG, LG)
    }
}

/// Swatch sizes for the inspector. 4-px grid.
pub mod swatch {
    use super::Vec2;
    pub const RECENT: Vec2 = Vec2::new(24.0, 24.0);
    pub const PALETTE: Vec2 = Vec2::new(24.0, 24.0);
    #[allow(dead_code)] // tool rail uses a Button cell, not a swatch — reserved if that changes
    pub const TOOL: Vec2 = Vec2::new(28.0, 28.0);
    pub const HERO_HEIGHT: f32 = 56.0; // foreground colour swatch height
}

/// Stroke widths. Hairline borders, accent rings, etc.
pub mod stroke {
    pub const HAIR: f32 = 0.5; // panel edges, swatch outlines, dividers
    pub const NORMAL: f32 = 1.0; // widget borders inside egui Visuals
    pub const ACCENT: f32 = 2.0; // selected swatch ring
}

/// Fixed widget sizes (Vec2 or scalar). 4-px grid, even.
pub mod size {
    use super::Vec2;
    pub const RULE_HEIGHT: f32 = 20.0; // vertical_rule
    pub const ICON_BUTTON: Vec2 = Vec2::new(28.0, 26.0); // icon_only_button min
    pub const ACTION_ROW_HEIGHT: f32 = 26.0; // wide_action_button, select_row
    pub const DROPDOWN_HEIGHT: f32 = 28.0;
    pub const PREFS_LABEL: Vec2 = Vec2::new(72.0, 20.0);
    pub const TOAST_ACCENT: Vec2 = Vec2::new(4.0, 20.0); // toast left accent bar
    pub const CMD_PALETTE_ROW: f32 = 30.0;
}

/// Container widths. Modals, panels, menus.
pub mod width {
    #[cfg_attr(target_os = "macos", allow(dead_code))] // native muda menu on mac
    pub const TOP_BAR_MENU: f32 = 180.0; // Import / Export submenus
    pub const SIDE_PANEL: f32 = 244.0; // right inspector
    pub const MODAL_PREFS: f32 = 340.0;
    pub const MODAL_NEW: f32 = 260.0;
    pub const COMMAND_PALETTE: f32 = 520.0;
    pub const TOAST: f32 = 360.0;
}

/// Container heights / max-heights.
pub mod height {
    pub const COMMAND_PALETTE_MAX: f32 = 360.0;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_fonts_at_least_12() {
        assert!(font::SMALL >= 12.0);
        assert!(font::BODY >= 12.0);
        assert!(font::HEADING >= 12.0);
    }

    #[test]
    fn all_radii_even() {
        for r in [radius::XS, radius::SM, radius::MD, radius::LG] {
            assert_eq!(r % 2, 0, "radius {r} is odd");
        }
    }

    #[test]
    fn all_scalars_even() {
        for v in [space::XS, space::SM, space::MD, space::LG] {
            assert_eq!(v as u32 % 2, 0, "space {v} is odd");
            assert!(v >= 0.0);
        }
        for v in [font::SMALL, font::BODY, font::HEADING] {
            assert_eq!(v as u32 % 2, 0, "font {v} is odd");
        }
        for v in [icon::SM, icon::MD, icon::LG] {
            assert_eq!(v as u32 % 2, 0, "icon {v} is odd");
        }
    }

    #[test]
    fn pad_and_gap_vecs_even() {
        let vs = [
            pad::NONE,
            pad::ICON,
            pad::BUTTON,
            pad::DEFAULT,
            pad::DIALOG,
            gap::NONE,
            gap::TIGHT,
            gap::DEFAULT,
        ];
        for v in vs {
            assert_eq!(v.x as u32 % 2, 0, "pad/gap x={} is odd", v.x);
            assert_eq!(v.y as u32 % 2, 0, "pad/gap y={} is odd", v.y);
        }
    }

    #[test]
    fn size_width_height_all_even() {
        let scalars = [
            size::RULE_HEIGHT,
            size::ACTION_ROW_HEIGHT,
            size::DROPDOWN_HEIGHT,
            size::CMD_PALETTE_ROW,
            width::TOP_BAR_MENU,
            width::SIDE_PANEL,
            width::MODAL_PREFS,
            width::MODAL_NEW,
            width::COMMAND_PALETTE,
            width::TOAST,
            height::COMMAND_PALETTE_MAX,
        ];
        for v in scalars {
            assert_eq!(v as u32 % 2, 0, "size scalar {v} is odd");
        }
        let vecs = [size::ICON_BUTTON, size::PREFS_LABEL, size::TOAST_ACCENT];
        for v in vecs {
            assert_eq!(v.x as u32 % 2, 0, "size vec x={} is odd", v.x);
            assert_eq!(v.y as u32 % 2, 0, "size vec y={} is odd", v.y);
        }
    }

    #[test]
    fn radii_strictly_increasing() {
        assert!(radius::XS < radius::SM);
        assert!(radius::SM < radius::MD);
        assert!(radius::MD < radius::LG);
    }
}
