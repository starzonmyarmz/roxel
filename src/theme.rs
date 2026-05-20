use bevy::prelude::*;
use bevy::winit::WINIT_WINDOWS;
use bevy_egui::egui;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::color_space::ColorSpace;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ThemeMode {
    Light,
    Dark,
}

#[derive(Resource, Clone, Copy)]
pub struct Theme {
    pub bg: egui::Color32,
    pub panel: egui::Color32,
    pub surface: egui::Color32,
    pub surface_hover: egui::Color32,
    pub accent: egui::Color32,
    pub accent_dim: egui::Color32,
    pub text: egui::Color32,
    pub text_dim: egui::Color32,
    pub border: egui::Color32,
    pub faint: egui::Color32,
    pub mode: ThemeMode,
}

impl Theme {
    pub const fn dark() -> Self {
        Self {
            bg: egui::Color32::from_rgb(0x19, 0x1A, 0x2E),
            panel: egui::Color32::from_rgb(26, 28, 34),
            surface: egui::Color32::from_rgb(38, 42, 50),
            surface_hover: egui::Color32::from_rgb(54, 60, 72),
            accent: egui::Color32::from_rgb(110, 165, 255),
            accent_dim: egui::Color32::from_rgb(60, 95, 155),
            text: egui::Color32::from_rgb(220, 225, 235),
            text_dim: egui::Color32::from_rgb(150, 158, 172),
            border: egui::Color32::from_rgb(44, 48, 58),
            faint: egui::Color32::from_rgb(32, 35, 42),
            mode: ThemeMode::Dark,
        }
    }
    pub const fn light() -> Self {
        Self {
            bg: egui::Color32::from_rgb(252, 252, 253),
            panel: egui::Color32::from_rgb(255, 255, 255),
            surface: egui::Color32::from_rgb(240, 242, 246),
            surface_hover: egui::Color32::from_rgb(228, 232, 240),
            accent: egui::Color32::from_rgb(60, 110, 220),
            accent_dim: egui::Color32::from_rgb(140, 175, 230),
            text: egui::Color32::from_rgb(32, 36, 44),
            text_dim: egui::Color32::from_rgb(110, 120, 135),
            border: egui::Color32::from_rgb(228, 232, 240),
            faint: egui::Color32::from_rgb(248, 249, 251),
            mode: ThemeMode::Light,
        }
    }
}

impl Default for Theme {
    fn default() -> Self {
        Theme::dark()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dark_bg_is_191a2e() {
        assert_eq!(Theme::dark().bg, egui::Color32::from_rgb(0x19, 0x1A, 0x2E));
    }

    #[test]
    fn dark_and_light_carry_correct_mode() {
        assert_eq!(Theme::dark().mode, ThemeMode::Dark);
        assert_eq!(Theme::light().mode, ThemeMode::Light);
    }

    #[test]
    fn default_theme_is_dark() {
        assert_eq!(Theme::default().mode, ThemeMode::Dark);
    }

    #[test]
    fn preferences_defaults() {
        let p = Preferences::default();
        assert_eq!(p.theme, ThemePref::System);
        assert_eq!(p.canvas_bg, CanvasBgPref::MatchTheme);
        assert!(p.show_floor_grid);
        assert!(p.show_y_axis);
        assert!(p.show_origin_axes);
    }

    #[test]
    fn resolve_canvas_match_theme_uses_neutral_dark_not_theme_bg() {
        // Important: must not pick up the bluish UI panel bg (#191A2E).
        // Voxel hues stay true when the canvas is near-neutral grey.
        let prefs = Preferences {
            canvas_bg: CanvasBgPref::MatchTheme,
            ..Default::default()
        };
        let dark = resolve_canvas_color(&prefs, &Theme::dark());
        assert_eq!(dark, [0x1C, 0x1C, 0x1E]);
        let light = resolve_canvas_color(&prefs, &Theme::light());
        assert_eq!(light, [0xFC, 0xFC, 0xFD]);
    }

    #[test]
    fn dark_canvas_is_near_neutral() {
        // R, G, B channels within 4/255 of each other = effectively neutral.
        let [r, g, b] = canvas_match_color(ThemeMode::Dark);
        let max = r.max(g).max(b);
        let min = r.min(g).min(b);
        assert!(max - min <= 4, "dark canvas not neutral: r={r} g={g} b={b}");
    }

    #[test]
    fn resolve_canvas_custom_returns_custom() {
        let prefs = Preferences {
            canvas_bg: CanvasBgPref::Custom([10, 20, 30]),
            ..Default::default()
        };
        assert_eq!(resolve_canvas_color(&prefs, &Theme::dark()), [10, 20, 30]);
    }

    #[test]
    fn preferences_loads_with_missing_new_fields() {
        // Old prefs file shape — only `theme` present. Serde defaults must fill
        // in the rest so we don't wipe a user's existing preferences.ron.
        let ron = "(theme: Dark)";
        let p: Preferences = ron::from_str(ron).expect("parse");
        assert_eq!(p.theme, ThemePref::Dark);
        assert_eq!(p.canvas_bg, CanvasBgPref::MatchTheme);
        assert!(p.show_floor_grid);
        assert!(p.show_y_axis);
        assert!(p.show_origin_axes);
    }

    #[test]
    fn preferences_roundtrip_origin_axes_false() {
        // Explicit `show_origin_axes: false` must survive a save/load cycle so
        // turning the origin triad off persists across sessions.
        let prefs = Preferences {
            show_origin_axes: false,
            ..Default::default()
        };
        let ron = ron::ser::to_string(&prefs).expect("serialize");
        let parsed: Preferences = ron::from_str(&ron).expect("parse");
        assert!(!parsed.show_origin_axes);
    }

    #[test]
    fn preferences_loads_without_color_space() {
        // Older preferences.ron lacking `color_space` must still load with
        // the default (Hex).
        let ron = "(theme: Dark, canvas_bg: MatchTheme, show_floor_grid: true, show_y_axis: true, show_origin_axes: true)";
        let p: Preferences = ron::from_str(ron).expect("parse");
        assert_eq!(p.color_space, ColorSpace::Hex);
    }

    #[test]
    fn preferences_roundtrip_color_space() {
        let prefs = Preferences {
            color_space: ColorSpace::Oklch,
            ..Default::default()
        };
        let s = ron::ser::to_string(&prefs).expect("serialize");
        let parsed: Preferences = ron::from_str(&s).expect("parse");
        assert_eq!(parsed.color_space, ColorSpace::Oklch);
    }

    #[test]
    fn preferences_loads_after_floor_fields_removed() {
        // Older preferences.ron carrying now-removed fields (`show_floor`,
        // `floor_color`, `show_walls`, `wall_color`) must still load. Serde
        // silently drops unknown fields.
        let ron = "(theme: Dark, show_floor: true, floor_color: MatchTheme, show_walls: true, wall_color: MatchTheme)";
        let p: Preferences = ron::from_str(ron).expect("parse");
        assert_eq!(p.theme, ThemePref::Dark);
        assert!(p.show_floor_grid);
    }
}

#[derive(Resource, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Debug)]
pub struct Preferences {
    pub theme: ThemePref,
    #[serde(default = "default_canvas_bg")]
    pub canvas_bg: CanvasBgPref,
    #[serde(default = "default_show_floor_grid")]
    pub show_floor_grid: bool,
    #[serde(default = "default_show_y_axis")]
    pub show_y_axis: bool,
    #[serde(default = "default_show_origin_axes")]
    pub show_origin_axes: bool,
    #[serde(default)]
    pub color_space: ColorSpace,
}

fn default_canvas_bg() -> CanvasBgPref {
    CanvasBgPref::MatchTheme
}
fn default_show_floor_grid() -> bool {
    true
}
fn default_show_y_axis() -> bool {
    true
}
fn default_show_origin_axes() -> bool {
    true
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            theme: ThemePref::default(),
            canvas_bg: default_canvas_bg(),
            show_floor_grid: default_show_floor_grid(),
            show_y_axis: default_show_y_axis(),
            show_origin_axes: default_show_origin_axes(),
            color_space: ColorSpace::default(),
        }
    }
}

#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Debug, Default)]
pub enum ThemePref {
    Light,
    Dark,
    #[default]
    System,
}

#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Debug)]
pub enum CanvasBgPref {
    MatchTheme,
    Custom([u8; 3]),
}

/// Per-mode default canvas color used when `CanvasBgPref::MatchTheme` is set.
/// Dark canvas is deliberately neutral grey (not the bluish UI panel bg) so
/// voxel hues aren't tinted; light canvas mirrors the UI bg.
pub fn canvas_match_color(mode: ThemeMode) -> [u8; 3] {
    match mode {
        ThemeMode::Dark => [0x1C, 0x1C, 0x1E],
        ThemeMode::Light => [0xFC, 0xFC, 0xFD],
    }
}

/// Resolve canvas (3D viewport) background color from preferences + theme.
/// Returns sRGB 8-bit components for downstream `Color::srgb_u8` use.
pub fn resolve_canvas_color(prefs: &Preferences, theme: &Theme) -> [u8; 3] {
    match prefs.canvas_bg {
        CanvasBgPref::Custom(rgb) => rgb,
        CanvasBgPref::MatchTheme => canvas_match_color(theme.mode),
    }
}

#[derive(Resource, Default)]
pub struct PreferencesWindow {
    pub open: bool,
}

fn prefs_path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("roxel").join("preferences.ron"))
}

pub fn load_preferences() -> Preferences {
    let Some(path) = prefs_path() else {
        return Preferences::default();
    };
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Preferences::default();
    };
    ron::from_str(&text).unwrap_or_default()
}

pub fn save_preferences(prefs: &Preferences) {
    let Some(path) = prefs_path() else { return };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(text) = ron::ser::to_string_pretty(prefs, ron::ser::PrettyConfig::default()) {
        let _ = std::fs::write(&path, text);
    }
}

fn detect_system_theme() -> ThemeMode {
    let detected = WINIT_WINDOWS.with(|cell| {
        let w = cell.borrow();
        w.windows
            .values()
            .next()
            .and_then(|win| win.theme())
            .map(|t| match t {
                winit::window::Theme::Light => ThemeMode::Light,
                winit::window::Theme::Dark => ThemeMode::Dark,
            })
    });
    detected.unwrap_or(ThemeMode::Dark)
}

pub fn resolve_theme(pref: ThemePref) -> Theme {
    match pref {
        ThemePref::Light => Theme::light(),
        ThemePref::Dark => Theme::dark(),
        ThemePref::System => match detect_system_theme() {
            ThemeMode::Light => Theme::light(),
            ThemeMode::Dark => Theme::dark(),
        },
    }
}

/// Recomputes the [`Theme`] resource from [`Preferences`] each frame so that
/// `ThemePref::System` tracks live OS appearance changes. Cheap (struct copy).
pub fn refresh_theme_system(
    _marker: bevy::ecs::system::NonSendMarker,
    prefs: Res<Preferences>,
    mut theme: ResMut<Theme>,
) {
    *theme = resolve_theme(prefs.theme);
}

const NUNITO_400: &[u8] = include_bytes!("../assets/Nunito-400.ttf");
const NUNITO_500: &[u8] = include_bytes!("../assets/Nunito-500.ttf");
const NUNITO_600: &[u8] = include_bytes!("../assets/Nunito-600.ttf");
const NUNITO_700: &[u8] = include_bytes!("../assets/Nunito-700.ttf");
const DM_MONO_400: &[u8] = include_bytes!("../assets/DMMono-400.ttf");

pub const NUNITO_500_FAMILY: &str = "Nunito500";
pub const NUNITO_600_FAMILY: &str = "Nunito600";
pub const NUNITO_700_FAMILY: &str = "Nunito700";

pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();

    let nunito_400_name = "Nunito400".to_string();
    let dm_mono_name = "DMMono400".to_string();

    for (name, bytes) in [
        (nunito_400_name.clone(), NUNITO_400),
        (NUNITO_500_FAMILY.to_string(), NUNITO_500),
        (NUNITO_600_FAMILY.to_string(), NUNITO_600),
        (NUNITO_700_FAMILY.to_string(), NUNITO_700),
        (dm_mono_name.clone(), DM_MONO_400),
    ] {
        fonts.font_data.insert(
            name.clone(),
            std::sync::Arc::new(egui::FontData::from_static(bytes)),
        );
    }

    // Proportional default → Nunito 400.
    fonts
        .families
        .entry(egui::FontFamily::Proportional)
        .or_default()
        .insert(0, nunito_400_name);

    // Monospace default → DM Mono 400.
    fonts
        .families
        .entry(egui::FontFamily::Monospace)
        .or_default()
        .insert(0, dm_mono_name);

    // Named families for heavier weights (use via FontFamily::Name).
    for fam in [NUNITO_500_FAMILY, NUNITO_600_FAMILY, NUNITO_700_FAMILY] {
        fonts
            .families
            .insert(egui::FontFamily::Name(fam.into()), vec![fam.to_string()]);
    }

    ctx.set_fonts(fonts);
}

pub fn apply_egui_style(ctx: &egui::Context, theme: &Theme) {
    use crate::ui::tokens::{font, gap, pad, radius, stroke};

    let mut visuals = match theme.mode {
        ThemeMode::Dark => egui::Visuals::dark(),
        ThemeMode::Light => egui::Visuals::light(),
    };

    visuals.override_text_color = Some(theme.text);
    visuals.panel_fill = theme.panel;
    visuals.window_fill = theme.panel;
    visuals.extreme_bg_color = theme.bg;
    visuals.faint_bg_color = theme.faint;

    let r_sm = egui::CornerRadius::same(radius::SM);

    visuals.widgets.noninteractive.bg_fill = theme.panel;
    visuals.widgets.noninteractive.weak_bg_fill = theme.panel;
    visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(stroke::NORMAL, theme.border);
    visuals.widgets.noninteractive.fg_stroke = egui::Stroke::new(stroke::NORMAL, theme.text_dim);
    visuals.widgets.noninteractive.corner_radius = r_sm;

    visuals.widgets.inactive.bg_fill = theme.surface;
    visuals.widgets.inactive.weak_bg_fill = theme.surface;
    visuals.widgets.inactive.bg_stroke = egui::Stroke::NONE;
    visuals.widgets.inactive.fg_stroke = egui::Stroke::new(stroke::NORMAL, theme.text);
    visuals.widgets.inactive.corner_radius = r_sm;

    visuals.widgets.hovered.bg_fill = theme.surface_hover;
    visuals.widgets.hovered.weak_bg_fill = theme.surface_hover;
    visuals.widgets.hovered.bg_stroke = egui::Stroke::new(stroke::NORMAL, theme.accent_dim);
    visuals.widgets.hovered.fg_stroke = egui::Stroke::new(stroke::NORMAL, theme.text);
    visuals.widgets.hovered.corner_radius = r_sm;

    visuals.widgets.active.bg_fill = theme.accent;
    visuals.widgets.active.weak_bg_fill = theme.accent;
    visuals.widgets.active.bg_stroke = egui::Stroke::new(stroke::NORMAL, theme.accent);
    visuals.widgets.active.fg_stroke = egui::Stroke::new(stroke::NORMAL, egui::Color32::WHITE);
    visuals.widgets.active.corner_radius = r_sm;

    visuals.widgets.open.bg_fill = theme.surface_hover;
    visuals.widgets.open.weak_bg_fill = theme.surface_hover;
    visuals.widgets.open.corner_radius = r_sm;

    visuals.selection.bg_fill = theme.accent_dim;
    visuals.selection.stroke = egui::Stroke::new(stroke::NORMAL, theme.accent);
    visuals.hyperlink_color = theme.accent;
    visuals.window_corner_radius = egui::CornerRadius::same(radius::LG);
    visuals.menu_corner_radius = egui::CornerRadius::same(radius::MD);
    visuals.window_stroke = egui::Stroke::new(stroke::NORMAL, theme.border);
    visuals.window_shadow = egui::epaint::Shadow {
        offset: [0, 4],
        blur: 12,
        spread: 0,
        color: egui::Color32::from_black_alpha(120),
    };
    visuals.popup_shadow = visuals.window_shadow;

    ctx.set_visuals(visuals);

    let mut style: egui::Style = (*ctx.style()).clone();
    style.spacing.item_spacing = gap::DEFAULT;
    style.spacing.button_padding = pad::DEFAULT;
    style.spacing.menu_margin = egui::Margin::same(8);
    style.spacing.window_margin = egui::Margin::same(12);
    style.spacing.slider_width = 160.0;
    style.spacing.interact_size.y = 26.0;
    style.interaction.selectable_labels = false;

    use egui::{FontFamily, FontId, TextStyle};
    let bold = FontFamily::Name(NUNITO_700_FAMILY.into());
    style
        .text_styles
        .insert(TextStyle::Heading, FontId::new(font::HEADING, bold.clone()));
    style.text_styles.insert(
        TextStyle::Body,
        FontId::new(font::BODY, FontFamily::Proportional),
    );
    style.text_styles.insert(
        TextStyle::Button,
        FontId::new(font::BODY, FontFamily::Proportional),
    );
    style.text_styles.insert(
        TextStyle::Small,
        FontId::new(font::SMALL, FontFamily::Proportional),
    );
    style.text_styles.insert(
        TextStyle::Monospace,
        FontId::new(font::BODY, FontFamily::Monospace),
    );
    ctx.set_style(style);
}
