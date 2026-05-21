use super::dialogs::{DialogResult, PendingDialog};
use crate::camera::{
    CameraPreset, PendingViewPreset, ZOOM_STEP_IN, ZOOM_STEP_OUT, apply_zoom, fit_view,
};
use crate::grid::{Color8, NewProject, VoxelGrid};
use crate::history::History;
use crate::shapes::ShapePrimitive;
use crate::theme::{INTER_SEMIBOLD_FAMILY, Preferences, PreferencesWindow, Theme, ThemePref};
use crate::tools::{CurrentColor, ShapeOptions, Tool, ToolState};
use crate::ui::palette::{
    Palette, PaletteChoice, Palettes, next_palette_name, unique_palette_name,
};
use crate::ui::tokens::{font, gap, height, icon, radius, size, space, stroke, width};
use crate::ui::{icons, widgets};
use crate::{io, select};
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};
use bevy_panorbit_camera::PanOrbitCamera;

const CHANGELOG_URL: &str = "https://github.com/starzonmyarmz/roxel/blob/main/CHANGELOG.md";

/// Resource backing the Cmd+K command palette. Holds open/close state, the
/// search query, the current selection index in the filtered list, a one-shot
/// `just_opened` flag that drives auto-focus on the search input, and a
/// `pending` slot that the dispatch system drains each frame.
#[derive(Resource, Default)]
pub struct CommandPalette {
    pub open: bool,
    pub search: String,
    pub selected: usize,
    pub just_opened: bool,
    pub pending: Option<CommandAction>,
}

impl CommandPalette {
    fn open_fresh(&mut self) {
        self.open = true;
        self.search.clear();
        self.selected = 0;
        self.just_opened = true;
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Category {
    File,
    Edit,
    Tools,
    Shape,
    View,
    Palette,
    Color,
    Preferences,
    Help,
}

impl Category {
    fn label(self) -> &'static str {
        match self {
            Category::File => "File",
            Category::Edit => "Edit",
            Category::Tools => "Tool",
            Category::Shape => "Shape",
            Category::View => "View",
            Category::Palette => "Palette",
            Category::Color => "Color",
            Category::Preferences => "Pref",
            Category::Help => "Help",
        }
    }
}

#[derive(Clone, Debug)]
pub enum CommandAction {
    NewProject,
    OpenProject,
    SaveProject,
    SaveProjectAs,
    ImportVox,
    ImportQb,
    ImportGox,
    ExportVox,
    ExportObj,
    ExportFbx,
    ExportGltf,
    ExportPng,
    ExportSvg,
    ExportGox,

    Undo,
    Redo,
    DeleteSelectionContents,
    ClearSelection,
    CopySelection,
    CutSelection,
    Paste,

    SelectTool(Tool),
    SelectShape(ShapePrimitive),

    FrameView,
    ViewPreset(CameraPreset),
    ZoomIn,
    ZoomOut,
    ToggleFlyby,
    OpenPreferences,
    OpenChangelog,

    SelectPalette(usize),
    AddCurrentColorToPalette,
    NewPalette,
    DuplicatePalette,
    DeletePalette,
    ImportAse,
    ExportAse,

    PickColor(Color8),

    SetThemePref(ThemePref),
    ToggleShowFloorGrid,
    ToggleShowYAxis,
    ToggleShowOriginAxes,
}

#[derive(Clone)]
pub struct CatalogEntry {
    pub label: String,
    pub category: Category,
    pub keywords: String,
    pub shortcut: Option<&'static str>,
    pub enabled: bool,
    pub action: CommandAction,
}

// -------- Fuzzy matcher (pure) --------

/// Subsequence fuzzy matcher with light scoring. Returns `None` when `query`
/// is not a subsequence of `haystack` (case-insensitive). Higher score = better
/// match. Bonuses for: leading match, word-boundary match, contiguous run.
/// Penalty per gap to keep tight matches above loose ones.
pub fn fuzzy_match(haystack: &str, query: &str) -> Option<i32> {
    if query.is_empty() {
        return Some(0);
    }
    let hay: Vec<char> = haystack.chars().flat_map(|c| c.to_lowercase()).collect();
    let needle: Vec<char> = query.chars().flat_map(|c| c.to_lowercase()).collect();
    let mut score: i32 = 0;
    let mut hi = 0usize;
    let mut last_match: Option<usize> = None;
    for &nc in &needle {
        let mut found: Option<usize> = None;
        while hi < hay.len() {
            if hay[hi] == nc {
                found = Some(hi);
                break;
            }
            hi += 1;
        }
        let m = found?;
        let prev_is_boundary =
            m == 0 || matches!(hay[m - 1], ' ' | '\t' | '-' | '_' | '/' | '.' | '(' | '#');
        if m == 0 {
            score += 12;
        } else if prev_is_boundary {
            score += 6;
        }
        if let Some(prev) = last_match
            && m == prev + 1
        {
            score += 4; // contiguous
        }
        if let Some(prev) = last_match {
            let gap = (m - prev) as i32 - 1;
            score -= gap.min(8);
        }
        score += 1;
        last_match = Some(m);
        hi = m + 1;
    }
    Some(score)
}

fn matches_entry(entry: &CatalogEntry, query: &str) -> Option<i32> {
    if query.is_empty() {
        return Some(0);
    }
    let label_score = fuzzy_match(&entry.label, query);
    let kw_score = if entry.keywords.is_empty() {
        None
    } else {
        fuzzy_match(&entry.keywords, query).map(|s| s - 4) // small penalty vs label hits
    };
    match (label_score, kw_score) {
        (Some(a), Some(b)) => Some(a.max(b)),
        (Some(a), None) => Some(a),
        (None, Some(b)) => Some(b),
        (None, None) => None,
    }
}

// -------- Catalog builder --------

/// Snapshot of the world state the catalog needs to compute `enabled` for
/// state-dependent actions. Borrowed-only so the builder is a pure function
/// over inputs (testable without a Bevy app).
pub struct CatalogState<'a> {
    pub tool: Tool,
    pub shape: &'a ShapeOptions,
    pub has_undo: bool,
    pub has_redo: bool,
    pub has_selection: bool,
    pub has_clipboard: bool,
    pub dialog_busy: bool,
    pub palettes: &'a [Palette],
    pub palette_choice: usize,
    pub current_color: Color8,
    pub prefs: &'a Preferences,
    pub flyby_active: bool,
}

pub fn build_catalog(state: &CatalogState) -> Vec<CatalogEntry> {
    let mut out: Vec<CatalogEntry> = Vec::with_capacity(80);

    let dialog_ok = !state.dialog_busy;
    let active_palette = state.palettes.get(state.palette_choice);
    let active_is_builtin = active_palette.map(|p| p.builtin).unwrap_or(true);
    let has_user_palette = state.palettes.iter().any(|p| !p.builtin);
    let active_has_color = active_palette
        .map(|p| p.colors.contains(&state.current_color))
        .unwrap_or(true);

    // File
    out.push(entry(
        "New project…",
        Category::File,
        "create empty",
        Some("⌘N"),
        true,
        CommandAction::NewProject,
    ));
    out.push(entry(
        "Open project…",
        Category::File,
        "roxel load file",
        Some("⌘O"),
        dialog_ok,
        CommandAction::OpenProject,
    ));
    out.push(entry(
        "Save project",
        Category::File,
        "roxel write file",
        Some("⌘S"),
        dialog_ok,
        CommandAction::SaveProject,
    ));
    out.push(entry(
        "Save project as…",
        Category::File,
        "roxel write file save as",
        Some("⇧⌘S"),
        dialog_ok,
        CommandAction::SaveProjectAs,
    ));
    out.push(entry(
        "Import MagicaVoxel (.vox)…",
        Category::File,
        "load import vox",
        None,
        dialog_ok,
        CommandAction::ImportVox,
    ));
    out.push(entry(
        "Import Qubicle (.qb)…",
        Category::File,
        "load import qubicle qb",
        None,
        dialog_ok,
        CommandAction::ImportQb,
    ));
    out.push(entry(
        "Import Goxel (.gox)…",
        Category::File,
        "load import goxel gox",
        None,
        dialog_ok,
        CommandAction::ImportGox,
    ));
    out.push(entry(
        "Export MagicaVoxel (.vox)…",
        Category::File,
        "save write vox",
        None,
        dialog_ok,
        CommandAction::ExportVox,
    ));
    out.push(entry(
        "Export Wavefront (.obj)…",
        Category::File,
        "save write obj wavefront mesh",
        None,
        dialog_ok,
        CommandAction::ExportObj,
    ));
    out.push(entry(
        "Export Autodesk (.fbx)…",
        Category::File,
        "save write fbx autodesk mesh",
        None,
        dialog_ok,
        CommandAction::ExportFbx,
    ));
    out.push(entry(
        "Export glTF (.glb)…",
        Category::File,
        "save write gltf glb mesh",
        None,
        dialog_ok,
        CommandAction::ExportGltf,
    ));
    out.push(entry(
        "Export Goxel (.gox)…",
        Category::File,
        "save write goxel gox",
        None,
        dialog_ok,
        CommandAction::ExportGox,
    ));
    out.push(entry(
        "Export Transparent PNG…",
        Category::File,
        "save write png screenshot image",
        None,
        dialog_ok,
        CommandAction::ExportPng,
    ));
    out.push(entry(
        "Export SVG…",
        Category::File,
        "save write svg vector image",
        None,
        dialog_ok,
        CommandAction::ExportSvg,
    ));

    // Edit
    out.push(entry(
        "Undo",
        Category::Edit,
        "history",
        Some("⌘Z"),
        state.has_undo,
        CommandAction::Undo,
    ));
    out.push(entry(
        "Redo",
        Category::Edit,
        "history",
        Some("⌘⇧Z"),
        state.has_redo,
        CommandAction::Redo,
    ));
    out.push(entry(
        "Delete selection contents",
        Category::Edit,
        "clear remove voxels",
        Some("⌫"),
        state.has_selection,
        CommandAction::DeleteSelectionContents,
    ));
    out.push(entry(
        "Clear selection",
        Category::Edit,
        "deselect",
        Some("Esc"),
        state.has_selection,
        CommandAction::ClearSelection,
    ));
    out.push(entry(
        "Copy",
        Category::Edit,
        "copy clipboard duplicate",
        Some("⌘C"),
        state.has_selection,
        CommandAction::CopySelection,
    ));
    out.push(entry(
        "Cut",
        Category::Edit,
        "cut clipboard",
        Some("⌘X"),
        state.has_selection,
        CommandAction::CutSelection,
    ));
    out.push(entry(
        "Paste",
        Category::Edit,
        "paste clipboard insert",
        Some("⌘V"),
        state.has_clipboard,
        CommandAction::Paste,
    ));

    // Tools
    for (kind, label, sc, kw) in [
        (Tool::Brush, "Switch to Brush", "B", "paint place draw"),
        (Tool::Erase, "Switch to Erase", "E", "delete remove"),
        (Tool::Paint, "Switch to Paint", "P", "recolor"),
        (
            Tool::Eyedropper,
            "Switch to Eyedropper",
            "I",
            "pick sample color",
        ),
        (
            Tool::Shape,
            "Switch to Shape",
            "S",
            "rectangle ellipse line",
        ),
        (Tool::Select, "Switch to Select", "M", "marquee region"),
        (Tool::Move, "Switch to Move", "V", "translate nudge"),
    ] {
        out.push(entry(
            label,
            Category::Tools,
            kw,
            Some(sc),
            state.tool != kind,
            CommandAction::SelectTool(kind),
        ));
    }

    // Shape
    out.push(entry(
        "Shape: Rectangle",
        Category::Shape,
        "rect square box",
        None,
        state.shape.primitive != ShapePrimitive::Rectangle,
        CommandAction::SelectShape(ShapePrimitive::Rectangle),
    ));
    out.push(entry(
        "Shape: Ellipse",
        Category::Shape,
        "circle oval",
        None,
        state.shape.primitive != ShapePrimitive::Ellipse,
        CommandAction::SelectShape(ShapePrimitive::Ellipse),
    ));
    out.push(entry(
        "Shape: Line",
        Category::Shape,
        "stroke",
        None,
        state.shape.primitive != ShapePrimitive::Line,
        CommandAction::SelectShape(ShapePrimitive::Line),
    ));

    // View
    out.push(entry(
        "Frame view (fit to content)",
        Category::View,
        "zoom fit center reframe",
        Some("⌘0"),
        true,
        CommandAction::FrameView,
    ));
    for (preset, shortcut, keywords) in [
        (CameraPreset::Front, "⌘1", "view angle camera +z forward"),
        (CameraPreset::Back, "⇧⌘1", "view angle camera -z behind"),
        (CameraPreset::Right, "⌘3", "view angle camera +x side"),
        (CameraPreset::Left, "⇧⌘3", "view angle camera -x side"),
        (CameraPreset::Top, "⌘7", "view angle camera down plan"),
        (
            CameraPreset::Iso,
            "⌘5",
            "view angle camera isometric default",
        ),
    ] {
        out.push(entry(
            &format!("{} view", preset.label()),
            Category::View,
            keywords,
            Some(shortcut),
            true,
            CommandAction::ViewPreset(preset),
        ));
    }
    out.push(entry(
        "Zoom in",
        Category::View,
        "camera closer",
        Some("⌘="),
        true,
        CommandAction::ZoomIn,
    ));
    out.push(entry(
        "Zoom out",
        Category::View,
        "camera farther",
        Some("⌘-"),
        true,
        CommandAction::ZoomOut,
    ));
    out.push(entry(
        if state.flyby_active {
            "Stop flyby camera"
        } else {
            "Start flyby camera"
        },
        Category::View,
        "drone tour cinematic auto orbit preview",
        if state.flyby_active {
            Some("Esc")
        } else {
            None
        },
        true,
        CommandAction::ToggleFlyby,
    ));
    out.push(entry(
        "Open Preferences…",
        Category::View,
        "settings options config",
        Some("⌘,"),
        true,
        CommandAction::OpenPreferences,
    ));

    // Palette
    out.push(entry(
        "Add current color to palette",
        Category::Palette,
        "swatch insert",
        None,
        !active_is_builtin && !active_has_color,
        CommandAction::AddCurrentColorToPalette,
    ));
    out.push(entry(
        "New palette",
        Category::Palette,
        "create empty",
        None,
        true,
        CommandAction::NewPalette,
    ));
    out.push(entry(
        "Duplicate palette",
        Category::Palette,
        "copy clone",
        None,
        active_palette.is_some(),
        CommandAction::DuplicatePalette,
    ));
    out.push(entry(
        "Delete palette",
        Category::Palette,
        "remove trash",
        None,
        !active_is_builtin && has_user_palette,
        CommandAction::DeletePalette,
    ));
    out.push(entry(
        "Import .ase palette…",
        Category::Palette,
        "adobe swatch exchange load",
        None,
        dialog_ok,
        CommandAction::ImportAse,
    ));
    out.push(entry(
        "Export .ase palette…",
        Category::Palette,
        "adobe swatch exchange save",
        None,
        dialog_ok && active_palette.is_some(),
        CommandAction::ExportAse,
    ));
    for (i, p) in state.palettes.iter().enumerate() {
        if i == state.palette_choice {
            continue;
        }
        out.push(entry(
            &format!("Switch to palette: {}", p.name),
            Category::Palette,
            if p.builtin { "built-in" } else { "user" },
            None,
            true,
            CommandAction::SelectPalette(i),
        ));
    }

    // Color (one per swatch in the active palette).
    if let Some(p) = active_palette {
        for c in &p.colors {
            if *c == state.current_color {
                continue;
            }
            let hex = widgets::hex_string([c[0], c[1], c[2]]);
            out.push(entry(
                &format!("Pick color {hex}"),
                Category::Color,
                &p.name,
                None,
                true,
                CommandAction::PickColor(*c),
            ));
        }
    }

    // Preferences
    out.push(entry(
        "Theme: System",
        Category::Preferences,
        "appearance auto os",
        None,
        state.prefs.theme != ThemePref::System,
        CommandAction::SetThemePref(ThemePref::System),
    ));
    out.push(entry(
        "Theme: Light",
        Category::Preferences,
        "appearance",
        None,
        state.prefs.theme != ThemePref::Light,
        CommandAction::SetThemePref(ThemePref::Light),
    ));
    out.push(entry(
        "Theme: Dark",
        Category::Preferences,
        "appearance",
        None,
        state.prefs.theme != ThemePref::Dark,
        CommandAction::SetThemePref(ThemePref::Dark),
    ));
    out.push(entry(
        if state.prefs.show_floor_grid {
            "Hide floor grid"
        } else {
            "Show floor grid"
        },
        Category::Preferences,
        "voxel cell lines on floor toggle",
        None,
        true,
        CommandAction::ToggleShowFloorGrid,
    ));
    out.push(entry(
        if state.prefs.show_y_axis {
            "Hide Y axis line"
        } else {
            "Show Y axis line"
        },
        Category::Preferences,
        "vertical origin sky line toggle",
        None,
        true,
        CommandAction::ToggleShowYAxis,
    ));
    out.push(entry(
        if state.prefs.show_origin_axes {
            "Hide origin axes"
        } else {
            "Show origin axes"
        },
        Category::Preferences,
        "red green blue center point triad toggle",
        None,
        true,
        CommandAction::ToggleShowOriginAxes,
    ));

    // Help
    out.push(entry(
        "Open Changelog",
        Category::Help,
        "release notes github",
        None,
        true,
        CommandAction::OpenChangelog,
    ));

    out
}

fn entry(
    label: &str,
    category: Category,
    keywords: &str,
    shortcut: Option<&'static str>,
    enabled: bool,
    action: CommandAction,
) -> CatalogEntry {
    CatalogEntry {
        label: label.to_string(),
        category,
        keywords: keywords.to_string(),
        shortcut,
        enabled,
        action,
    }
}

/// Filter + rank `entries` by `query`. Returns indices into `entries` in
/// display order. Disabled entries are kept (greyed-out in the UI) but pushed
/// below enabled entries with the same score.
pub fn rank(entries: &[CatalogEntry], query: &str) -> Vec<usize> {
    let mut scored: Vec<(usize, i32)> = entries
        .iter()
        .enumerate()
        .filter_map(|(i, e)| matches_entry(e, query).map(|s| (i, s)))
        .collect();
    scored.sort_by(|a, b| {
        let ea = entries[a.0].enabled;
        let eb = entries[b.0].enabled;
        eb.cmp(&ea) // enabled first
            .then(b.1.cmp(&a.1)) // higher score first
            .then(entries[a.0].label.cmp(&entries[b.0].label)) // tie-break alpha
    });
    scored.into_iter().map(|(i, _)| i).collect()
}

// -------- Shortcut system (Cmd+K / Ctrl+K) --------

pub fn command_palette_shortcut_system(
    keys: Res<ButtonInput<KeyCode>>,
    mut contexts: EguiContexts,
    mut palette: ResMut<CommandPalette>,
) {
    if !keys.just_pressed(KeyCode::KeyK) {
        return;
    }
    let cmd = keys.pressed(KeyCode::SuperLeft)
        || keys.pressed(KeyCode::SuperRight)
        || keys.pressed(KeyCode::ControlLeft)
        || keys.pressed(KeyCode::ControlRight);
    if !cmd {
        return;
    }
    // If egui already owns the keyboard (e.g. inline rename editor) AND the
    // palette isn't already open, defer to it.
    let captured = contexts
        .ctx_mut()
        .map(|c| c.wants_keyboard_input())
        .unwrap_or(false);
    if captured && !palette.open {
        return;
    }
    if palette.open {
        palette.open = false;
    } else {
        palette.open_fresh();
    }
}

// -------- Renderer (called from ui_system) --------

/// Render the centered command-palette modal and handle keyboard navigation
/// inside it. Returns nothing; sets `palette.pending` when the user runs a
/// command, which the dispatcher consumes next frame.
pub fn draw(
    ctx: &egui::Context,
    theme: &Theme,
    palette: &mut CommandPalette,
    catalog: &[CatalogEntry],
) {
    if !palette.open {
        return;
    }
    // Pre-consume arrow/enter/esc keys so the search TextEdit doesn't read
    // them this frame. This keeps the text caret stable while the user is
    // navigating the result list.
    let (mut down, mut up, mut enter, mut esc) = (false, false, false, false);
    ctx.input_mut(|i| {
        down = i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown);
        up = i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp);
        enter = i.consume_key(egui::Modifiers::NONE, egui::Key::Enter);
        esc = i.consume_key(egui::Modifiers::NONE, egui::Key::Escape);
    });

    let prev_search = palette.search.clone();
    let mut open_flag = true;
    let mut close_after = false;
    let mut pending: Option<CommandAction> = None;

    let order = rank(catalog, &palette.search);
    // Clamp selection inside the filtered list.
    if order.is_empty() {
        palette.selected = 0;
    } else if palette.selected >= order.len() {
        palette.selected = order.len() - 1;
    }

    // Apply navigation (skip disabled entries).
    if !order.is_empty() {
        if down {
            palette.selected = next_enabled(catalog, &order, palette.selected, 1);
        }
        if up {
            palette.selected = next_enabled(catalog, &order, palette.selected, -1);
        }
    }
    if esc {
        close_after = true;
    }
    if enter
        && let Some(&idx) = order.get(palette.selected)
        && catalog[idx].enabled
    {
        pending = Some(catalog[idx].action.clone());
        close_after = true;
    }

    egui::Window::new(
        egui::RichText::new("Command palette")
            .family(egui::FontFamily::Name(INTER_SEMIBOLD_FAMILY.into()))
            .size(font::BODY),
    )
    .collapsible(false)
    .resizable(false)
    .open(&mut open_flag)
    .anchor(egui::Align2::CENTER_TOP, [0.0, 60.0])
    .default_width(width::COMMAND_PALETTE)
    .min_width(width::COMMAND_PALETTE)
    .frame(
        egui::Frame::window(&ctx.style())
            .fill(theme.panel)
            .inner_margin(egui::Margin::symmetric(14, 12))
            .stroke(egui::Stroke::new(stroke::HAIR, theme.border))
            .corner_radius(egui::CornerRadius::same(radius::LG)),
    )
    .show(ctx, |ui| {
        ui.set_min_width(492.0);

        let resp = ui.add(
            egui::TextEdit::singleline(&mut palette.search)
                .desired_width(f32::INFINITY)
                .hint_text("Type a command…")
                .font(egui::TextStyle::Body),
        );
        if palette.just_opened {
            resp.request_focus();
            palette.just_opened = false;
        } else if !resp.has_focus() && !resp.lost_focus() {
            resp.request_focus();
        }

        ui.add_space(space::XS);
        ui.painter().hline(
            ui.clip_rect().x_range(),
            ui.cursor().min.y,
            egui::Stroke::new(stroke::HAIR, theme.border),
        );
        ui.add_space(space::SX);

        if order.is_empty() {
            ui.add_space(space::SM);
            widgets::hint_label(ui, theme, "No commands match.");
            ui.add_space(space::SM);
        } else {
            egui::ScrollArea::vertical()
                .max_height(height::COMMAND_PALETTE_MAX)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = gap::NONE.y;
                    for (visual_idx, &idx) in order.iter().enumerate() {
                        let entry = &catalog[idx];
                        let selected = visual_idx == palette.selected;
                        if draw_row(ui, theme, entry, selected) && entry.enabled {
                            pending = Some(entry.action.clone());
                            close_after = true;
                        }
                    }
                });
        }

        ui.add_space(space::SX);
        ui.painter().hline(
            ui.clip_rect().x_range(),
            ui.cursor().min.y,
            egui::Stroke::new(stroke::HAIR, theme.border),
        );
        ui.add_space(space::SX);
        draw_footer_hint(ui, theme);
    });

    if palette.search != prev_search {
        // Re-clamp + reset to first enabled when the user types.
        let new_order = rank(catalog, &palette.search);
        if let Some(first) = new_order
            .iter()
            .position(|&i| catalog[i].enabled)
            .or(Some(0))
        {
            palette.selected = first;
        }
    }
    if let Some(action) = pending {
        palette.pending = Some(action);
    }
    if close_after || !open_flag {
        palette.open = false;
    }
}

fn draw_footer_hint(ui: &mut egui::Ui, theme: &Theme) {
    let dim = theme.text_dim;
    let footer_icon = |src| {
        egui::Image::new(src)
            .fit_to_exact_size(icon::sm_square())
            .tint(dim)
    };
    let text = |s: &str| {
        egui::Label::new(egui::RichText::new(s).color(dim).size(font::SMALL)).selectable(false)
    };
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = gap::TIGHT.x;
        ui.add(footer_icon(icons::arrow_up()));
        ui.add(footer_icon(icons::arrow_down()));
        ui.add_space(space::XXS);
        ui.add(text("navigate"));
        ui.add_space(space::FOOTER_GROUP);
        ui.add(footer_icon(icons::corner_down_left()));
        ui.add_space(space::XXS);
        ui.add(text("run"));
        ui.add_space(space::FOOTER_GROUP);
        ui.add(
            egui::Label::new(
                egui::RichText::new("Esc")
                    .monospace()
                    .color(dim)
                    .size(font::SMALL),
            )
            .selectable(false),
        );
        ui.add_space(space::XXS);
        ui.add(text("close"));
    });
}

fn next_enabled(catalog: &[CatalogEntry], order: &[usize], current: usize, step: i32) -> usize {
    let n = order.len();
    if n == 0 {
        return 0;
    }
    let mut i = current as i32;
    for _ in 0..n {
        i += step;
        if i < 0 {
            i = (n - 1) as i32;
        } else if i >= n as i32 {
            i = 0;
        }
        if catalog[order[i as usize]].enabled {
            return i as usize;
        }
    }
    current
}

fn draw_row(ui: &mut egui::Ui, theme: &Theme, entry: &CatalogEntry, selected: bool) -> bool {
    let row_h = size::CMD_PALETTE_ROW;
    let full = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(full, row_h), egui::Sense::click());
    let bg = if selected {
        theme.surface_hover
    } else if resp.hovered() && entry.enabled {
        theme.surface
    } else {
        egui::Color32::TRANSPARENT
    };
    ui.painter()
        .rect_filled(rect, egui::CornerRadius::same(radius::SM), bg);

    let text_color = if entry.enabled {
        theme.text
    } else {
        theme.text_dim
    };
    let dim = theme.text_dim;

    // Category chip (left).
    let chip_x = rect.left() + 10.0;
    let chip_text = entry.category.label();
    let chip_galley = ui.painter().layout_no_wrap(
        chip_text.to_string(),
        egui::FontId::proportional(font::SMALL),
        dim,
    );
    let chip_y = rect.center().y - chip_galley.size().y * 0.5;
    ui.painter()
        .galley(egui::pos2(chip_x, chip_y), chip_galley.clone(), dim);

    // Label.
    let label_x = chip_x + 56.0;
    let label_galley = ui.painter().layout_no_wrap(
        entry.label.clone(),
        egui::FontId::proportional(font::BODY),
        text_color,
    );
    let label_y = rect.center().y - label_galley.size().y * 0.5;
    ui.painter()
        .galley(egui::pos2(label_x, label_y), label_galley, text_color);

    // Shortcut (right).
    if let Some(sc) = entry.shortcut {
        let sc_galley =
            ui.painter()
                .layout_no_wrap(sc.to_string(), egui::FontId::monospace(font::SMALL), dim);
        let sc_x = rect.right() - 10.0 - sc_galley.size().x;
        let sc_y = rect.center().y - sc_galley.size().y * 0.5;
        ui.painter().galley(egui::pos2(sc_x, sc_y), sc_galley, dim);
    }

    resp.clicked() && entry.enabled
}

// -------- Dispatch --------

#[derive(SystemParam)]
pub struct DispatchParams<'w> {
    palette: ResMut<'w, CommandPalette>,
    pending: ResMut<'w, PendingDialog>,
    history: ResMut<'w, History>,
    grid: ResMut<'w, VoxelGrid>,
    new_project: ResMut<'w, NewProject>,
    prefs_window: ResMut<'w, PreferencesWindow>,
    tool: ResMut<'w, ToolState>,
    shape: ResMut<'w, ShapeOptions>,
    selection: ResMut<'w, select::Selection>,
    palette_choice: ResMut<'w, PaletteChoice>,
    palettes: ResMut<'w, Palettes>,
    color: ResMut<'w, CurrentColor>,
    prefs: ResMut<'w, Preferences>,
    current_path: Res<'w, super::dialogs::CurrentProjectPath>,
    flyby: ResMut<'w, crate::camera::FlybyState>,
    toasts: ResMut<'w, super::Toasts>,
    view_preset: ResMut<'w, PendingViewPreset>,
    clipboard: ResMut<'w, crate::clipboard::Clipboard>,
}

pub fn dispatch_command_palette_system(
    mut p: DispatchParams,
    mut cameras: Query<&mut PanOrbitCamera>,
) {
    let Some(action) = p.palette.pending.take() else {
        return;
    };
    match action {
        CommandAction::NewProject => {
            p.new_project.dialog_open = true;
        }
        CommandAction::OpenProject => spawn_open(&mut p.pending),
        CommandAction::SaveProject => super::dialogs::spawn_save(&mut p.pending, &p.current_path),
        CommandAction::SaveProjectAs => {
            super::dialogs::spawn_save_as(&mut p.pending, &p.current_path)
        }
        CommandAction::ImportVox => spawn_import(
            &mut p.pending,
            "MagicaVoxel",
            "vox",
            DialogResult::ImportVox,
        ),
        CommandAction::ImportQb => {
            spawn_import(&mut p.pending, "Qubicle", "qb", DialogResult::ImportQb)
        }
        CommandAction::ImportGox => {
            spawn_import(&mut p.pending, "Goxel", "gox", DialogResult::ImportGox)
        }
        CommandAction::ExportVox => spawn_export(
            &mut p.pending,
            "MagicaVoxel",
            "vox",
            "model.vox",
            DialogResult::ExportVox,
        ),
        CommandAction::ExportObj => spawn_export(
            &mut p.pending,
            "Wavefront OBJ",
            "obj",
            "model.obj",
            DialogResult::ExportObj,
        ),
        CommandAction::ExportFbx => spawn_export(
            &mut p.pending,
            "Autodesk FBX",
            "fbx",
            "model.fbx",
            DialogResult::ExportFbx,
        ),
        CommandAction::ExportGltf => spawn_export(
            &mut p.pending,
            "glTF binary",
            "glb",
            "model.glb",
            DialogResult::ExportGltf,
        ),
        CommandAction::ExportPng => spawn_export(
            &mut p.pending,
            "PNG image",
            "png",
            "roxel.png",
            DialogResult::ExportPng,
        ),
        CommandAction::ExportSvg => spawn_export(
            &mut p.pending,
            "SVG image",
            "svg",
            "roxel.svg",
            DialogResult::ExportSvg,
        ),
        CommandAction::ExportGox => spawn_export(
            &mut p.pending,
            "Goxel",
            "gox",
            "model.gox",
            DialogResult::ExportGox,
        ),
        CommandAction::Undo => p.history.undo(&mut p.grid),
        CommandAction::Redo => p.history.redo(&mut p.grid),
        CommandAction::DeleteSelectionContents => {
            if p.selection.aabb.is_some() {
                select::clear_selection(&mut p.grid, &mut p.history, &p.selection);
            }
        }
        CommandAction::ClearSelection => p.selection.clear(),
        CommandAction::CopySelection => {
            if let Some(stamp) = crate::clipboard::copy_selection(&p.grid, &p.selection) {
                let n = stamp.voxel_count();
                p.clipboard.stamp = Some(stamp);
                p.toasts.info(format!("Copied {n} voxels"));
            }
        }
        CommandAction::CutSelection => {
            if let Some(stamp) =
                crate::clipboard::cut_selection(&mut p.grid, &mut p.history, &p.selection)
            {
                let n = stamp.voxel_count();
                p.clipboard.stamp = Some(stamp);
                p.toasts.info(format!("Cut {n} voxels"));
            }
        }
        CommandAction::Paste => {
            if let Some(stamp) = p.clipboard.stamp.clone() {
                crate::clipboard::execute_paste(
                    &mut p.grid,
                    &mut p.history,
                    &mut p.selection,
                    &mut p.toasts,
                    &stamp,
                    None,
                );
            }
        }
        CommandAction::SelectTool(t) => {
            if p.tool.current != t {
                p.tool.previous = p.tool.current;
                p.tool.current = t;
            }
        }
        CommandAction::SelectShape(prim) => p.shape.primitive = prim,
        CommandAction::FrameView => {
            let (centroid, radius) =
                fit_view(&p.grid).unwrap_or((Vec3::ZERO, crate::camera::EMPTY_WORLD_RADIUS));
            for mut cam in &mut cameras {
                cam.target_focus = centroid;
                cam.target_radius = radius;
            }
        }
        CommandAction::ViewPreset(preset) => {
            p.view_preset.0 = Some(preset);
        }
        CommandAction::ZoomIn => {
            for mut cam in &mut cameras {
                cam.target_radius = apply_zoom(
                    cam.target_radius,
                    ZOOM_STEP_IN,
                    cam.zoom_lower_limit,
                    cam.zoom_upper_limit,
                );
            }
        }
        CommandAction::ZoomOut => {
            for mut cam in &mut cameras {
                cam.target_radius = apply_zoom(
                    cam.target_radius,
                    ZOOM_STEP_OUT,
                    cam.zoom_lower_limit,
                    cam.zoom_upper_limit,
                );
            }
        }
        CommandAction::ToggleFlyby => {
            p.flyby.active = !p.flyby.active;
            if p.flyby.active {
                p.toasts.info("Flyby active — Esc to exit");
            }
        }
        CommandAction::OpenPreferences => p.prefs_window.open = true,
        CommandAction::OpenChangelog => open_url(CHANGELOG_URL),
        CommandAction::SelectPalette(i) => {
            if i < p.palettes.0.len() {
                p.palette_choice.0 = i;
            }
        }
        CommandAction::AddCurrentColorToPalette => {
            let i = p.palette_choice.0.min(p.palettes.0.len().saturating_sub(1));
            if let Some(pal) = p.palettes.0.get_mut(i)
                && !pal.builtin
                && !pal.colors.contains(&p.color.0)
            {
                pal.colors.push(p.color.0);
                io::palettes::save(&p.palettes.0);
            }
        }
        CommandAction::NewPalette => {
            let name = next_palette_name(&p.palettes.0);
            p.palettes.0.push(Palette {
                name,
                colors: Vec::new(),
                builtin: false,
            });
            p.palette_choice.0 = p.palettes.0.len() - 1;
            io::palettes::save(&p.palettes.0);
        }
        CommandAction::DuplicatePalette => {
            let i = p.palette_choice.0.min(p.palettes.0.len().saturating_sub(1));
            if let Some(src) = p.palettes.0.get(i) {
                let base = if src.builtin {
                    src.name.clone()
                } else {
                    format!("{} copy", src.name)
                };
                let name = unique_palette_name(&p.palettes.0, &base);
                let copy = Palette {
                    name,
                    colors: src.colors.clone(),
                    builtin: false,
                };
                p.palettes.0.push(copy);
                p.palette_choice.0 = p.palettes.0.len() - 1;
                io::palettes::save(&p.palettes.0);
            }
        }
        CommandAction::DeletePalette => {
            let i = p.palette_choice.0;
            if i < p.palettes.0.len() && !p.palettes.0[i].builtin {
                p.palettes.0.remove(i);
                if p.palette_choice.0 >= p.palettes.0.len() {
                    p.palette_choice.0 = p.palettes.0.len().saturating_sub(1);
                }
                io::palettes::save(&p.palettes.0);
            }
        }
        CommandAction::ImportAse => {
            if !p.pending.is_active() {
                p.pending.spawn(async move {
                    rfd::AsyncFileDialog::new()
                        .add_filter("Adobe Swatch Exchange", &["ase"])
                        .pick_file()
                        .await
                        .map(|f| DialogResult::ImportAse(f.path().to_path_buf()))
                });
            }
        }
        CommandAction::ExportAse => {
            if p.pending.is_active() {
                return;
            }
            let i = p.palette_choice.0.min(p.palettes.0.len().saturating_sub(1));
            let Some(current) = p.palettes.0.get(i) else {
                return;
            };
            let export_name = current.name.clone();
            let export_colors = current.colors.clone();
            let default_filename = format!(
                "{}.ase",
                crate::ui::palette::sanitize_filename(&current.name)
            );
            p.pending.spawn(async move {
                rfd::AsyncFileDialog::new()
                    .add_filter("Adobe Swatch Exchange", &["ase"])
                    .set_file_name(&default_filename)
                    .save_file()
                    .await
                    .map(|f| {
                        DialogResult::ExportAse(f.path().to_path_buf(), export_name, export_colors)
                    })
            });
        }
        CommandAction::PickColor(c) => p.color.0 = c,
        CommandAction::SetThemePref(t) => {
            p.prefs.theme = t;
            crate::theme::save_preferences(&p.prefs);
        }
        CommandAction::ToggleShowFloorGrid => {
            p.prefs.show_floor_grid = !p.prefs.show_floor_grid;
            crate::theme::save_preferences(&p.prefs);
        }
        CommandAction::ToggleShowYAxis => {
            p.prefs.show_y_axis = !p.prefs.show_y_axis;
            crate::theme::save_preferences(&p.prefs);
        }
        CommandAction::ToggleShowOriginAxes => {
            p.prefs.show_origin_axes = !p.prefs.show_origin_axes;
            crate::theme::save_preferences(&p.prefs);
        }
    }
}

fn spawn_open(pending: &mut PendingDialog) {
    if pending.is_active() {
        return;
    }
    pending.spawn(async move {
        rfd::AsyncFileDialog::new()
            .add_filter("Roxel project", &["rox"])
            .pick_file()
            .await
            .map(|f| DialogResult::OpenProject(f.path().to_path_buf()))
    });
}

fn spawn_import(
    pending: &mut PendingDialog,
    label: &'static str,
    ext: &'static str,
    wrap: fn(std::path::PathBuf) -> DialogResult,
) {
    if pending.is_active() {
        return;
    }
    pending.spawn(async move {
        rfd::AsyncFileDialog::new()
            .add_filter(label, &[ext])
            .pick_file()
            .await
            .map(|f| wrap(f.path().to_path_buf()))
    });
}

fn spawn_export(
    pending: &mut PendingDialog,
    label: &'static str,
    ext: &'static str,
    default_name: &'static str,
    wrap: fn(std::path::PathBuf) -> DialogResult,
) {
    if pending.is_active() {
        return;
    }
    pending.spawn(async move {
        rfd::AsyncFileDialog::new()
            .add_filter(label, &[ext])
            .set_file_name(default_name)
            .save_file()
            .await
            .map(|f| wrap(f.path().to_path_buf()))
    });
}

fn open_url(url: &str) {
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg(url).spawn();
    #[cfg(target_os = "linux")]
    let _ = std::process::Command::new("xdg-open").arg(url).spawn();
    #[cfg(target_os = "windows")]
    let _ = std::process::Command::new("cmd")
        .args(["/C", "start", "", url])
        .spawn();
}

// -------- Tests --------

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_state(palettes: &[Palette]) -> (ShapeOptions, Preferences, CatalogState<'_>) {
        let shape = ShapeOptions::default();
        let prefs = Preferences::default();
        (shape, prefs, dummy(palettes))
    }

    fn dummy(palettes: &[Palette]) -> CatalogState<'_> {
        // Caller holds shape/prefs; this helper is unused outside sample_state.
        CatalogState {
            tool: Tool::Brush,
            shape: Box::leak(Box::new(ShapeOptions::default())),
            has_undo: true,
            has_redo: false,
            has_selection: false,
            has_clipboard: false,
            dialog_busy: false,
            palettes,
            palette_choice: 0,
            current_color: [10, 20, 30, 255],
            prefs: Box::leak(Box::new(Preferences::default())),
            flyby_active: false,
        }
    }

    fn pal(name: &str, builtin: bool) -> Palette {
        Palette {
            name: name.into(),
            colors: vec![[1, 2, 3, 255], [4, 5, 6, 255]],
            builtin,
        }
    }

    #[test]
    fn fuzzy_subsequence_basic() {
        assert!(fuzzy_match("Export glTF (.glb)", "exg").is_some());
        assert!(fuzzy_match("Save project", "xyz").is_none());
    }

    #[test]
    fn fuzzy_is_case_insensitive() {
        assert!(fuzzy_match("Export Transparent PNG", "png").is_some());
        assert!(fuzzy_match("Export Transparent PNG", "PNG").is_some());
    }

    #[test]
    fn fuzzy_prefers_word_boundary() {
        // "fv" lands on word boundaries in "First Voxel" (F at start, V after
        // space) and mid-word in "Coffee Vintage" (f inside "Coffee"). Both
        // are valid subsequences; the boundary one should score higher.
        let a = fuzzy_match("First Voxel", "fv").expect("match a");
        let b = fuzzy_match("Coffee Vintage", "fv").expect("match b");
        assert!(
            a > b,
            "expected First Voxel > Coffee Vintage, got {a} vs {b}"
        );
    }

    #[test]
    fn fuzzy_prefers_contiguous() {
        // "exp" should be contiguous in "Export"; in "Extra people" it's
        // gappy. Contiguous should win.
        let a = fuzzy_match("Export Wavefront", "exp").expect("a");
        let b = fuzzy_match("Extra people", "exp").expect("b");
        assert!(a > b, "{a} !> {b}");
    }

    #[test]
    fn empty_query_matches_everything() {
        assert_eq!(fuzzy_match("anything", ""), Some(0));
    }

    #[test]
    fn catalog_disables_undo_when_history_empty() {
        let palettes = vec![pal("p1", true)];
        let shape = ShapeOptions::default();
        let prefs = Preferences::default();
        let state = CatalogState {
            tool: Tool::Brush,
            shape: &shape,
            has_undo: false,
            has_redo: false,
            has_selection: false,
            has_clipboard: false,
            dialog_busy: false,
            palettes: &palettes,
            palette_choice: 0,
            current_color: [10, 20, 30, 255],
            prefs: &prefs,
            flyby_active: false,
        };
        let cat = build_catalog(&state);
        let undo = cat
            .iter()
            .find(|e| matches!(e.action, CommandAction::Undo))
            .expect("undo entry");
        assert!(!undo.enabled);
    }

    #[test]
    fn catalog_lists_other_palettes_as_switch_entries() {
        let palettes = vec![pal("Alpha", true), pal("Beta", false), pal("Gamma", false)];
        let shape = ShapeOptions::default();
        let prefs = Preferences::default();
        let state = CatalogState {
            tool: Tool::Brush,
            shape: &shape,
            has_undo: false,
            has_redo: false,
            has_selection: false,
            has_clipboard: false,
            dialog_busy: false,
            palettes: &palettes,
            palette_choice: 0,
            current_color: [10, 20, 30, 255],
            prefs: &prefs,
            flyby_active: false,
        };
        let cat = build_catalog(&state);
        let switches: Vec<_> = cat
            .iter()
            .filter(|e| matches!(e.action, CommandAction::SelectPalette(_)))
            .collect();
        // Two other palettes (active palette is skipped).
        assert_eq!(switches.len(), 2);
        assert!(switches.iter().any(|e| e.label.contains("Beta")));
        assert!(switches.iter().any(|e| e.label.contains("Gamma")));
        assert!(switches.iter().all(|e| !e.label.contains("Alpha")));
    }

    #[test]
    fn rank_orders_better_matches_first() {
        let palettes = vec![pal("p1", true)];
        let shape = ShapeOptions::default();
        let prefs = Preferences::default();
        let state = CatalogState {
            tool: Tool::Brush,
            shape: &shape,
            has_undo: true,
            has_redo: true,
            has_selection: false,
            has_clipboard: false,
            dialog_busy: false,
            palettes: &palettes,
            palette_choice: 0,
            current_color: [10, 20, 30, 255],
            prefs: &prefs,
            flyby_active: false,
        };
        let cat = build_catalog(&state);
        let order = rank(&cat, "frame");
        let first = &cat[order[0]];
        assert!(first.label.starts_with("Frame view"), "got {}", first.label);
    }

    #[test]
    fn rank_keeps_disabled_in_results_but_below_enabled() {
        let palettes = vec![pal("p1", true)];
        let shape = ShapeOptions::default();
        let prefs = Preferences::default();
        let state = CatalogState {
            tool: Tool::Brush,
            shape: &shape,
            has_undo: false,
            has_redo: false,
            has_selection: false,
            has_clipboard: false,
            dialog_busy: false,
            palettes: &palettes,
            palette_choice: 0,
            current_color: [10, 20, 30, 255],
            prefs: &prefs,
            flyby_active: false,
        };
        let cat = build_catalog(&state);
        let order = rank(&cat, "undo");
        let undo_pos = order
            .iter()
            .position(|&i| matches!(cat[i].action, CommandAction::Undo))
            .expect("Undo entry should appear in results");
        // The Undo entry stays disabled (no history) but is still in the list.
        assert!(!cat[order[undo_pos]].enabled);
        // Every entry above the disabled Undo must itself be enabled.
        for &i in &order[..undo_pos] {
            assert!(
                cat[i].enabled,
                "disabled entry {:?} ranked above disabled Undo",
                cat[i].label
            );
        }
    }

    #[test]
    fn unused_sample_state_helper_compiles() {
        // Kept so a future test can quickly grab a default state without
        // re-typing every field. Calling once exercises the path.
        let palettes = vec![pal("p", true)];
        let (_shape, _prefs, state) = sample_state(&palettes);
        assert_eq!(state.tool, Tool::Brush);
    }
}
