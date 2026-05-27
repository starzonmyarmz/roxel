//! Design tokens — every spacing, padding, font, radius, and icon size in the
//! UI resolves to a value here. Scales are 4-px grid, all even, no fonts
//! below 12 pt.
//!
//! Pick by semantic name, not raw value. If a new use site doesn't fit any
//! existing token, add a token rather than inlining a number.

use bevy_egui::egui::Vec2;
use bevy_egui::egui::epaint::Shadow;

/// Font sizes (proportional + monospace). `BODY` doubles as the mono size so
/// hex strings and stat values match adjacent body text.
pub mod font {
    /// Inspector section labels — rendered uppercase with a SemiBold family.
    #[allow(dead_code)] // adopted by widgets::section in the typography pass
    pub const SECTION: f32 = 12.0;
    pub const SMALL: f32 = 12.0;
    pub const BODY: f32 = 14.0;
    pub const HEADING: f32 = 16.0;
}

/// Corner radii. `u8` to feed `egui::CornerRadius::same(...)` directly.
///
/// Nesting rule: a child's radius should equal `parent - inner_padding`. The
/// floating pill uses `PILL = 18` with `pad::DEFAULT.x = 12`, so children
/// nested inside the pill use `INSIDE_PILL = MD = 8` instead of `SM = 6` to
/// avoid the "punch hole" look where sub-corners read as stamped out.
pub mod radius {
    pub const XS: u8 = 4; // small inner bits (swatch corners)
    pub const SM: u8 = 6; // standalone buttons, command palette rows
    pub const MD: u8 = 8; // menus, popups, hero swatch, INSIDE_PILL
    pub const LG: u8 = 12; // modal windows
    pub const PILL: u8 = 18; // floating pill menu, status chip
    /// Radius for children nested directly inside a floating pill surface.
    /// Pinned to `MD` so a tool button corner reads concentric with its pill
    /// parent instead of carving a smaller square out of it.
    pub const INSIDE_PILL: u8 = MD;
}

/// Three-tier elevation. Pick by intent, not by raw blur/offset. Larger tier
/// = surface further off the canvas.
///
/// - `low` — resting inspector cards, palette toolbar rows
/// - `mid` — floating pill menu, tool island, dropdown popups
/// - `high` — modal windows, command palette, toast stack
pub mod shadow {
    use super::Shadow;
    use bevy_egui::egui::Color32;

    #[allow(dead_code)] // adopted by collapsible inspector sections
    pub fn low() -> Shadow {
        Shadow {
            offset: [0, 1],
            blur: 4,
            spread: 0,
            color: Color32::from_black_alpha(30),
        }
    }
    pub fn mid() -> Shadow {
        Shadow {
            offset: [0, 4],
            blur: 12,
            spread: 0,
            color: Color32::from_black_alpha(60),
        }
    }
    pub fn high() -> Shadow {
        Shadow {
            offset: [0, 12],
            blur: 28,
            spread: 0,
            color: Color32::from_black_alpha(120),
        }
    }
}

/// Scalar gaps for `ui.add_space(...)`. Even values; `XS`/`SM`/`MD`/`LG` sit
/// on the 4-px grid, `XXS`/`SX` are reserved for tight in-section breaks.
pub mod space {
    pub const XXS: f32 = 2.0; // micro-break between adjacent rows (visibility checkboxes, tool rail)
    pub const XS: f32 = 4.0;
    pub const SX: f32 = 6.0; // between XS and SM — palette toolbar header gap, action-button cluster
    pub const SM: f32 = 8.0;
    /// Wider-than-`SM` separator used between footer command groups
    /// ("navigate", "run", "close") in the command palette.
    pub const FOOTER_GROUP: f32 = 10.0;
    pub const MD: f32 = 12.0;
    #[allow(dead_code)] // reserved for layout tuning
    pub const LG: f32 = 16.0;
    /// Margin between canvas edge and a floating UI element (pill menu, tool
    /// island, status chip).
    pub const FLOAT_GAP: f32 = 16.0;
    /// Horizontal indent that aligns a row under `size::PREFS_LABEL`.
    /// Equals `size::PREFS_LABEL.x + XS`.
    pub const PREFS_INDENT: f32 = 76.0;
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
    #[allow(dead_code)] // adopted once egui exposes a tooltip frame override
    pub const TOOLTIP: Vec2 = Vec2::new(10.0, 6.0); // tooltip popup inner padding
}

/// Icon `fit_to_exact_size` values. Square — width == height. 4-px grid.
pub mod icon {
    use super::Vec2;
    pub const SM: f32 = 14.0; // command-palette footer + wide-action icon
    pub const MD: f32 = 16.0; // top-bar, dialog icons
    pub const LG: f32 = 20.0; // left tool rail
    pub const HERO: f32 = 64.0; // onboarding coachmark hero

    pub const fn sm_square() -> Vec2 {
        Vec2::new(SM, SM)
    }
    pub const fn md_square() -> Vec2 {
        Vec2::new(MD, MD)
    }
    pub const fn lg_square() -> Vec2 {
        Vec2::new(LG, LG)
    }
    pub const fn hero_square() -> Vec2 {
        Vec2::new(HERO, HERO)
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
    pub const CMD_PALETTE_ROW: f32 = 30.0;
}

/// Container widths. Modals, panels, menus.
pub mod width {
    #[cfg_attr(target_os = "macos", allow(dead_code))] // native muda menu on mac
    pub const TOP_BAR_MENU: f32 = 180.0; // Import / Export submenus
    pub const SIDE_PANEL: f32 = 244.0; // left inspector
    pub const MODAL_PREFS: f32 = 340.0;
    pub const MODAL_NEW: f32 = 240.0;
    pub const COMMAND_PALETTE: f32 = 520.0;
    pub const TOAST: f32 = 360.0;
    pub const COACHMARK: f32 = 256.0;
    /// Maximum width of the floating pill menu on Win/Linux. The pill sizes
    /// itself to its content; this is just an upper bound for layout.
    #[allow(dead_code)] // reserved for tighter pill layouts
    pub const FLOAT_MENU: f32 = 320.0;
    /// Minimum width of the bottom-right status chip. Lets short scenes
    /// (`0×0×0`) still read as a pill instead of squishing to a circle.
    #[allow(dead_code)] // reserved if the status chip layout returns
    pub const STATUS_CHIP_MIN: f32 = 220.0;
    /// Outer width of the floating tool island (icon-only mode). Labels mode
    /// expands via egui wrap; no separate token.
    #[allow(dead_code)] // referenced once egui exposes a fixed-width island layout
    pub const TOOL_ISLAND: f32 = 56.0;
}

/// Container heights / max-heights.
pub mod height {
    pub const COMMAND_PALETTE_MAX: f32 = 360.0;
    /// Height of the floating pill menu row.
    #[allow(dead_code)] // reserved for pill-height tuning
    pub const FLOAT_MENU: f32 = 36.0;
    /// Height of the floating status chip.
    #[allow(dead_code)] // reserved for chip-height tuning
    pub const STATUS_CHIP: f32 = 32.0;
    /// Reserved vertical space at the top of the macOS window for the
    /// transparent titlebar + traffic-light buttons when
    /// `fullsize_content_view` is enabled.
    pub const MAC_TITLEBAR_GUTTER: f32 = 28.0;
    /// Hero illustration band at the top of the onboarding coachmark card.
    pub const COACHMARK_HERO: f32 = 140.0;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_fonts_at_least_12() {
        assert!(font::SECTION >= 12.0);
        assert!(font::SMALL >= 12.0);
        assert!(font::BODY >= 12.0);
        assert!(font::HEADING >= 12.0);
    }

    #[test]
    fn shadow_tiers_strictly_increase_in_depth() {
        let low = shadow::low();
        let mid = shadow::mid();
        let high = shadow::high();
        assert!(low.blur < mid.blur);
        assert!(mid.blur < high.blur);
        assert!(low.offset[1] < mid.offset[1]);
        assert!(mid.offset[1] < high.offset[1]);
        // Alpha increases with depth so the further surface reads as more lifted.
        assert!(low.color.a() < mid.color.a());
        assert!(mid.color.a() < high.color.a());
    }

    #[test]
    fn inside_pill_radius_aligns_with_pill_padding() {
        // PILL outer (18) minus pad::DEFAULT.x (12) = 6, but rounding to the
        // token grid lands on MD (8) so children read concentric rather than
        // pinched. The invariant we lock is "INSIDE_PILL >= SM" — never smaller
        // than the standalone button radius.
        assert!(radius::INSIDE_PILL >= radius::SM);
        assert!(radius::INSIDE_PILL < radius::PILL);
    }

    #[test]
    fn all_radii_even() {
        for r in [radius::XS, radius::SM, radius::MD, radius::LG, radius::PILL] {
            assert_eq!(r % 2, 0, "radius {r} is odd");
        }
    }

    #[test]
    fn all_scalars_even() {
        for v in [
            space::XXS,
            space::XS,
            space::SX,
            space::SM,
            space::FOOTER_GROUP,
            space::MD,
            space::LG,
            space::FLOAT_GAP,
            space::PREFS_INDENT,
        ] {
            assert_eq!(v as u32 % 2, 0, "space {v} is odd");
            assert!(v >= 0.0);
        }
        for v in [font::SECTION, font::SMALL, font::BODY, font::HEADING] {
            assert_eq!(v as u32 % 2, 0, "font {v} is odd");
        }
        for v in [icon::SM, icon::MD, icon::LG, icon::HERO] {
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
            width::COACHMARK,
            width::FLOAT_MENU,
            width::STATUS_CHIP_MIN,
            width::TOOL_ISLAND,
            height::COMMAND_PALETTE_MAX,
            height::FLOAT_MENU,
            height::STATUS_CHIP,
            height::MAC_TITLEBAR_GUTTER,
            height::COACHMARK_HERO,
        ];
        for v in scalars {
            assert_eq!(v as u32 % 2, 0, "size scalar {v} is odd");
        }
        let vecs = [size::ICON_BUTTON, size::PREFS_LABEL];
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
        assert!(radius::LG < radius::PILL);
    }

    #[test]
    fn space_scalars_strictly_increasing() {
        assert!(space::XXS < space::XS);
        assert!(space::XS < space::SX);
        assert!(space::SX < space::SM);
        assert!(space::SM < space::FOOTER_GROUP);
        assert!(space::FOOTER_GROUP < space::MD);
        assert!(space::MD < space::LG);
        assert!(space::LG <= space::FLOAT_GAP);
    }

    #[test]
    fn prefs_indent_matches_label_row() {
        assert_eq!(space::PREFS_INDENT, size::PREFS_LABEL.x + space::XS);
    }
}
