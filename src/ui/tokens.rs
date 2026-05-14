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
    fn radii_strictly_increasing() {
        assert!(radius::XS < radius::SM);
        assert!(radius::SM < radius::MD);
        assert!(radius::MD < radius::LG);
    }
}
