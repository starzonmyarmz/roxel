use bevy::prelude::*;
use bevy::winit::WINIT_WINDOWS;
use bevy_egui::egui;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::SystemTime;

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
        // Neutral greys — no chroma in the UI panel so the iOS-blue voxels
        // and the violet brand accent both read cleanly against the surface.
        Self {
            bg: egui::Color32::from_rgb(0x14, 0x14, 0x16),
            panel: egui::Color32::from_rgb(0x1A, 0x1B, 0x1E),
            surface: egui::Color32::from_rgb(0x26, 0x27, 0x2B),
            surface_hover: egui::Color32::from_rgb(0x33, 0x34, 0x39),
            // Brand violet (Retcon/Spline-adjacent). Distinct from iOS blue so
            // selected swatches/tools don't compete with the system accent
            // mac users may have wired up elsewhere.
            accent: egui::Color32::from_rgb(0x7C, 0x5C, 0xFF),
            accent_dim: egui::Color32::from_rgb(0x3B, 0x2D, 0x7A),
            text: egui::Color32::from_rgb(220, 225, 235),
            text_dim: egui::Color32::from_rgb(150, 158, 172),
            border: egui::Color32::from_rgb(0x2C, 0x2D, 0x32),
            faint: egui::Color32::from_rgb(0x1F, 0x20, 0x23),
            mode: ThemeMode::Dark,
        }
    }
    pub const fn light() -> Self {
        Self {
            bg: egui::Color32::from_rgb(252, 252, 253),
            panel: egui::Color32::from_rgb(255, 255, 255),
            surface: egui::Color32::from_rgb(240, 242, 246),
            surface_hover: egui::Color32::from_rgb(228, 232, 240),
            accent: egui::Color32::from_rgb(0x6B, 0x47, 0xFF),
            accent_dim: egui::Color32::from_rgb(0xC8, 0xBA, 0xFF),
            text: egui::Color32::from_rgb(32, 36, 44),
            text_dim: egui::Color32::from_rgb(110, 120, 135),
            border: egui::Color32::from_rgb(210, 215, 225),
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
    fn dark_panel_is_neutral_grey() {
        // R/G/B within 4/255 = effectively neutral. Hardcoding the exact hex
        // here would lock the palette; the property worth guarding is "no
        // chroma in the panel surface" so voxel hues stay true and the brand
        // accent reads against a neutral background.
        let p = Theme::dark().panel;
        let max = p.r().max(p.g()).max(p.b());
        let min = p.r().min(p.g()).min(p.b());
        assert!(
            max - min <= 4,
            "dark panel not neutral: r={} g={} b={}",
            p.r(),
            p.g(),
            p.b()
        );
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
        assert_eq!(light, [0xF2, 0xF3, 0xF6]);
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
        let ron =
            "(theme: Dark, canvas_bg: MatchTheme, show_floor_grid: true, show_origin_axes: true)";
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
    fn preferences_loads_without_last_update_check() {
        // Older preferences.ron lacking `last_update_check` must still load with
        // None so the updater treats it as never-checked.
        let ron = "(theme: Dark, canvas_bg: MatchTheme, show_floor_grid: true, show_origin_axes: true, color_space: Hex)";
        let p: Preferences = ron::from_str(ron).expect("parse");
        assert!(p.last_update_check.is_none());
    }

    #[test]
    fn preferences_roundtrip_last_update_check() {
        let prefs = Preferences {
            last_update_check: Some(
                SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000),
            ),
            ..Default::default()
        };
        let s = ron::ser::to_string(&prefs).expect("serialize");
        let parsed: Preferences = ron::from_str(&s).expect("parse");
        assert_eq!(parsed.last_update_check, prefs.last_update_check);
    }

    #[test]
    fn preferences_loads_without_onboarding_seen_field() {
        // Older preferences.ron written before the onboarding tour landed
        // must still load with `onboarding_seen == false`, so the tour fires
        // on the user's next launch instead of being silently skipped.
        let ron = "(theme: Dark, canvas_bg: MatchTheme, show_floor_grid: true, show_origin_axes: true, color_space: Hex)";
        let p: Preferences = ron::from_str(ron).expect("parse");
        assert!(!p.onboarding_seen);
    }

    #[test]
    fn preferences_roundtrip_onboarding_seen_true() {
        let prefs = Preferences {
            onboarding_seen: true,
            ..Default::default()
        };
        let s = ron::ser::to_string(&prefs).expect("serialize");
        let parsed: Preferences = ron::from_str(&s).expect("parse");
        assert!(parsed.onboarding_seen);
    }

    #[test]
    fn preferences_loads_without_floating_ui_fields() {
        // preferences.ron written before show_floating_menu_bar landed must
        // still parse, falling back to the per-platform default.
        let ron = "(theme: Dark, canvas_bg: MatchTheme, show_floor_grid: true, show_origin_axes: true, color_space: Hex)";
        let p: Preferences = ron::from_str(ron).expect("parse");
        assert_eq!(p.show_floating_menu_bar, !cfg!(target_os = "macos"));
    }

    #[test]
    fn preferences_drops_removed_show_chip_and_label_fields() {
        // `show_status_chip` and `show_tool_labels` were retired when the
        // inspector and tool island committed to a single layout. Older
        // preferences.ron carrying them must still load (serde drops unknown
        // fields silently).
        let ron = "(theme: Dark, show_status_chip: false, show_tool_labels: true)";
        let p: Preferences = ron::from_str(ron).expect("parse");
        assert_eq!(p.theme, ThemePref::Dark);
    }

    #[test]
    fn preferences_roundtrip_floating_menu_bar_field() {
        let prefs = Preferences {
            show_floating_menu_bar: true,
            ..Default::default()
        };
        let s = ron::ser::to_string(&prefs).expect("serialize");
        let parsed: Preferences = ron::from_str(&s).expect("parse");
        assert!(parsed.show_floating_menu_bar);
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

    #[test]
    fn preferences_loads_after_show_y_axis_field_removed() {
        // `show_y_axis` was dropped when the long Y-axis sky line was retired.
        // Older preferences.ron carrying the field must still load (serde
        // silently drops unknown struct fields).
        let ron = "(theme: Dark, show_floor_grid: true, show_y_axis: true, show_origin_axes: true)";
        let p: Preferences = ron::from_str(ron).expect("parse");
        assert_eq!(p.theme, ThemePref::Dark);
        assert!(p.show_floor_grid);
        assert!(p.show_origin_axes);
    }
}

#[derive(Resource, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Debug)]
pub struct Preferences {
    pub theme: ThemePref,
    #[serde(default = "default_canvas_bg")]
    pub canvas_bg: CanvasBgPref,
    #[serde(default = "default_show_floor_grid")]
    pub show_floor_grid: bool,
    #[serde(default = "default_show_origin_axes")]
    pub show_origin_axes: bool,
    #[serde(default)]
    pub color_space: ColorSpace,
    #[serde(default)]
    pub last_update_check: Option<SystemTime>,
    #[serde(default = "default_onboarding_seen")]
    pub onboarding_seen: bool,
    #[serde(default = "default_show_floating_menu_bar")]
    pub show_floating_menu_bar: bool,
}

fn default_canvas_bg() -> CanvasBgPref {
    CanvasBgPref::MatchTheme
}
fn default_show_floor_grid() -> bool {
    true
}
fn default_show_origin_axes() -> bool {
    true
}
fn default_onboarding_seen() -> bool {
    false
}
fn default_show_floating_menu_bar() -> bool {
    !cfg!(target_os = "macos")
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            theme: ThemePref::default(),
            canvas_bg: default_canvas_bg(),
            show_floor_grid: default_show_floor_grid(),
            show_origin_axes: default_show_origin_axes(),
            color_space: ColorSpace::default(),
            last_update_check: None,
            onboarding_seen: default_onboarding_seen(),
            show_floating_menu_bar: default_show_floating_menu_bar(),
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
        ThemeMode::Light => [0xF2, 0xF3, 0xF6],
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

const INTER_MEDIUM: &[u8] = include_bytes!("../assets/Inter-Medium.ttf");
const INTER_SEMIBOLD: &[u8] = include_bytes!("../assets/Inter-SemiBold.ttf");

pub const INTER_SEMIBOLD_FAMILY: &str = "InterSemiBold";

pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();

    let inter_medium = "InterMedium".to_string();
    fonts.font_data.insert(
        inter_medium.clone(),
        std::sync::Arc::new(egui::FontData::from_static(INTER_MEDIUM)),
    );
    fonts.font_data.insert(
        INTER_SEMIBOLD_FAMILY.to_string(),
        std::sync::Arc::new(egui::FontData::from_static(INTER_SEMIBOLD)),
    );

    fonts
        .families
        .entry(egui::FontFamily::Proportional)
        .or_default()
        .insert(0, inter_medium);

    fonts.families.insert(
        egui::FontFamily::Name(INTER_SEMIBOLD_FAMILY.into()),
        vec![INTER_SEMIBOLD_FAMILY.to_string()],
    );

    if let Some(bytes) = load_system_monospace() {
        let name = "system_mono".to_string();
        fonts.font_data.insert(
            name.clone(),
            std::sync::Arc::new(egui::FontData::from_owned(bytes)),
        );
        fonts
            .families
            .entry(egui::FontFamily::Monospace)
            .or_default()
            .insert(0, name);
    }

    ctx.set_fonts(fonts);
}

fn load_first_existing(paths: &[&str]) -> Option<Vec<u8>> {
    for p in paths {
        if let Ok(bytes) = std::fs::read(p) {
            return Some(bytes);
        }
    }
    None
}

fn load_system_monospace() -> Option<Vec<u8>> {
    // Monaco reads with a slightly heavier stroke than SFNSMono.ttf (which is
    // the Light weight on macOS and looks anemic at body sizes), so prefer it
    // for inspector readouts. Fall back to SFNSMono if Monaco is missing.
    #[cfg(target_os = "macos")]
    let paths: &[&str] = &[
        "/System/Library/Fonts/Monaco.ttf",
        "/System/Library/Fonts/SFNSMono.ttf",
    ];
    #[cfg(target_os = "windows")]
    let paths: &[&str] = &[
        "C:\\Windows\\Fonts\\consola.ttf",
        "C:\\Windows\\Fonts\\cour.ttf",
    ];
    #[cfg(all(unix, not(target_os = "macos")))]
    let paths: &[&str] = &[
        "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf",
        "/usr/share/fonts/TTF/DejaVuSansMono.ttf",
        "/usr/share/fonts/dejavu/DejaVuSansMono.ttf",
        "/usr/share/fonts/truetype/ubuntu/UbuntuMono-R.ttf",
        "/usr/share/fonts/liberation/LiberationMono-Regular.ttf",
        "/usr/share/fonts/truetype/liberation/LiberationMono-Regular.ttf",
    ];
    load_first_existing(paths)
}

pub fn apply_egui_style(ctx: &egui::Context, theme: &Theme) {
    use crate::ui::tokens::{font, gap, pad, radius, shadow, stroke};

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
    // Elevation language: shadows carry depth, no hairline border on floating
    // surfaces. Modals + popups sit at the highest tier; menus at mid.
    visuals.window_stroke = egui::Stroke::NONE;
    visuals.window_shadow = shadow::high();
    visuals.popup_shadow = shadow::mid();

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
    let semibold = FontFamily::Name(INTER_SEMIBOLD_FAMILY.into());
    style.text_styles.insert(
        TextStyle::Heading,
        FontId::new(font::HEADING, semibold.clone()),
    );
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
