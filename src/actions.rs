//! Every user command in one table. The command palette, the macOS menu bar,
//! the Win/Linux menu pill and the global shortcut system all read an
//! [`Action`]'s label, shortcut and enabled state from here, push it onto
//! [`ActionQueue`], and [`apply_actions_system`] runs it through [`dispatch`].

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy_panorbit_camera::PanOrbitCamera;
use std::borrow::Cow;
use std::path::{Path, PathBuf};

use crate::GridResource;
use crate::camera::{CameraPreset, FlybyState, PendingFrameView, PendingViewPreset};
use crate::camera::{ZOOM_STEP_IN, ZOOM_STEP_OUT, apply_zoom};
use crate::select::Selection;
use crate::theme::{Preferences, PreferencesWindow, ThemePref, save_preferences};
use crate::tools::{CurrentColor, ShapeOptions, Tool, ToolState};
use crate::ui::{
    CurrentProjectPath, DeleteConfirm, DialogResult, DiscardConfirm, GuardedAction, Palette,
    PaletteChoice, Palettes, PendingDialog, RecentFiles, Toasts, UnsavedGuard, WorkingPalette,
};
use roxel::grid::Color8;
use roxel::history::History;
use roxel::shapes::ShapePrimitive;

pub const CHANGELOG_URL: &str = "https://github.com/starzonmyarmz/roxel/blob/main/CHANGELOG.md";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Platform {
    Mac,
    Other,
}

impl Platform {
    pub const CURRENT: Platform = if cfg!(target_os = "macos") {
        Platform::Mac
    } else {
        Platform::Other
    };
}

/// A key plus modifiers. `cmd` is ⌘ on macOS and Ctrl elsewhere.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Chord {
    pub cmd: bool,
    pub shift: bool,
    pub key: KeyCode,
}

/// One chip of a displayed shortcut.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KeyToken {
    Command,
    Control,
    Shift,
    Key(Cow<'static, str>),
}

impl Chord {
    pub const fn key(key: KeyCode) -> Self {
        Self {
            cmd: false,
            shift: false,
            key,
        }
    }

    pub const fn cmd(key: KeyCode) -> Self {
        Self {
            cmd: true,
            shift: false,
            key,
        }
    }

    pub const fn cmd_shift(key: KeyCode) -> Self {
        Self {
            cmd: true,
            shift: true,
            key,
        }
    }

    /// Per-key chips in platform order: ⇧⌘S on macOS, Ctrl Shift S elsewhere.
    pub fn tokens(self, platform: Platform) -> Vec<KeyToken> {
        let mut out = Vec::with_capacity(3);
        match platform {
            Platform::Mac => {
                if self.shift {
                    out.push(KeyToken::Shift);
                }
                if self.cmd {
                    out.push(KeyToken::Command);
                }
            }
            Platform::Other => {
                if self.cmd {
                    out.push(KeyToken::Control);
                }
                if self.shift {
                    out.push(KeyToken::Shift);
                }
            }
        }
        out.push(KeyToken::Key(key_name(self.key, platform)));
        out
    }

    /// Shortcut text: "⇧⌘S" on macOS, "Ctrl+Shift+S" elsewhere.
    pub fn display(self, platform: Platform) -> String {
        let parts = self.tokens(platform).into_iter().map(|t| match t {
            KeyToken::Command => Cow::Borrowed("⌘"),
            KeyToken::Control => Cow::Borrowed("Ctrl"),
            KeyToken::Shift if platform == Platform::Mac => Cow::Borrowed("⇧"),
            KeyToken::Shift => Cow::Borrowed("Shift"),
            KeyToken::Key(k) => k,
        });
        let sep = if platform == Platform::Mac { "" } else { "+" };
        parts.collect::<Vec<_>>().join(sep)
    }
}

fn key_name(key: KeyCode, platform: Platform) -> Cow<'static, str> {
    match key {
        KeyCode::Backspace if platform == Platform::Mac => "⌫".into(),
        KeyCode::Backspace => "Backspace".into(),
        KeyCode::Comma => ",".into(),
        KeyCode::Equal => "=".into(),
        KeyCode::Minus => "-".into(),
        other => {
            let name = format!("{other:?}");
            let short = name
                .strip_prefix("Key")
                .or_else(|| name.strip_prefix("Digit"))
                .unwrap_or(&name);
            short.to_string().into()
        }
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
    Preferences,
    Help,
}

impl Category {
    pub fn label(self) -> &'static str {
        match self {
            Category::File => "File",
            Category::Edit => "Edit",
            Category::Tools => "Tool",
            Category::Shape => "Shape",
            Category::View => "View",
            Category::Palette => "Palette",
            Category::Preferences => "Pref",
            Category::Help => "Help",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImportFormat {
    Vox,
    Qb,
    Gox,
}

impl ImportFormat {
    fn filter_name(self) -> &'static str {
        match self {
            Self::Vox => "MagicaVoxel",
            Self::Qb => "Qubicle",
            Self::Gox => "Goxel",
        }
    }

    fn ext(self) -> &'static str {
        match self {
            Self::Vox => "vox",
            Self::Qb => "qb",
            Self::Gox => "gox",
        }
    }

    fn wrap(self) -> fn(PathBuf) -> DialogResult {
        match self {
            Self::Vox => DialogResult::ImportVox,
            Self::Qb => DialogResult::ImportQb,
            Self::Gox => DialogResult::ImportGox,
        }
    }

    fn spec(self) -> ActionSpec {
        let (label, menu_label, keywords) = match self {
            Self::Vox => (
                "Import MagicaVoxel (.vox)…",
                "MagicaVoxel (.vox)…",
                "load import vox",
            ),
            Self::Qb => (
                "Import Qubicle (.qb)…",
                "Qubicle (.qb)…",
                "load import qubicle qb",
            ),
            Self::Gox => (
                "Import Goxel (.gox)…",
                "Goxel (.gox)…",
                "load import goxel gox",
            ),
        };
        spec(label, menu_label, Category::File, keywords, None)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExportFormat {
    Vox,
    Gox,
    Obj,
    Gltf,
    Png,
    Svg,
}

impl ExportFormat {
    fn filter_name(self) -> &'static str {
        match self {
            Self::Vox => "MagicaVoxel",
            Self::Gox => "Goxel",
            Self::Obj => "Wavefront OBJ",
            Self::Gltf => "glTF binary",
            Self::Png => "PNG image",
            Self::Svg => "SVG image",
        }
    }

    fn ext(self) -> &'static str {
        match self {
            Self::Vox => "vox",
            Self::Gox => "gox",
            Self::Obj => "obj",
            Self::Gltf => "glb",
            Self::Png => "png",
            Self::Svg => "svg",
        }
    }

    fn default_name(self) -> &'static str {
        match self {
            Self::Vox => "model.vox",
            Self::Gox => "model.gox",
            Self::Obj => "model.obj",
            Self::Gltf => "model.glb",
            Self::Png => "roxel.png",
            Self::Svg => "roxel.svg",
        }
    }

    fn wrap(self) -> fn(PathBuf) -> DialogResult {
        match self {
            Self::Vox => DialogResult::ExportVox,
            Self::Gox => DialogResult::ExportGox,
            Self::Obj => DialogResult::ExportObj,
            Self::Gltf => DialogResult::ExportGltf,
            Self::Png => DialogResult::ExportPng,
            Self::Svg => DialogResult::ExportSvg,
        }
    }

    fn spec(self) -> ActionSpec {
        let (label, menu_label, keywords) = match self {
            Self::Vox => (
                "Export MagicaVoxel (.vox)…",
                "MagicaVoxel (.vox)…",
                "save write vox",
            ),
            Self::Gox => (
                "Export Goxel (.gox)…",
                "Goxel (.gox)…",
                "save write goxel gox",
            ),
            Self::Obj => (
                "Export Wavefront (.obj)…",
                "Wavefront (.obj)…",
                "save write obj wavefront mesh",
            ),
            Self::Gltf => (
                "Export glTF (.glb)…",
                "glTF (.glb)…",
                "save write gltf glb mesh",
            ),
            Self::Png => (
                "Export Transparent PNG…",
                "Transparent PNG…",
                "save write png screenshot image",
            ),
            Self::Svg => ("Export SVG…", "SVG…", "save write svg vector image"),
        };
        spec(label, menu_label, Category::File, keywords, None)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    NewProject,
    OpenProject,
    OpenRecent(usize),
    SaveProject,
    SaveProjectAs,
    Import(ImportFormat),
    Export(ExportFormat),
    ExportShot,

    Undo,
    Redo,
    DeleteSelectionContents,
    ClearSelection,
    Copy,
    Cut,
    Paste,
    DoubleDensity,
    HalveDensity,

    SelectTool(Tool),
    SelectShape(ShapePrimitive),

    FrameView,
    ViewPreset(CameraPreset),
    ZoomIn,
    ZoomOut,
    ToggleFlyby,
    ToggleFloorGrid,
    ToggleOriginAxes,

    SelectPalette(usize),
    AddColorToPalette,
    NewPalette,
    DuplicatePalette,
    DeletePalette,
    ImportAse,
    ExportAse,

    Preferences,
    SetTheme(ThemePref),
    Changelog,
}

/// Static description of an [`Action`]. `label` stands alone (command
/// palette); `menu_label` sits under a menu or submenu title that gives it
/// context.
#[derive(Clone, Copy, Debug)]
pub struct ActionSpec {
    pub label: &'static str,
    pub menu_label: &'static str,
    pub category: Category,
    pub keywords: &'static str,
    pub shortcut: Option<Chord>,
}

const fn spec(
    label: &'static str,
    menu_label: &'static str,
    category: Category,
    keywords: &'static str,
    shortcut: Option<Chord>,
) -> ActionSpec {
    ActionSpec {
        label,
        menu_label,
        category,
        keywords,
        shortcut,
    }
}

/// World state that decides whether an action is enabled, checked or how it
/// reads. A borrowed snapshot so the table stays testable without an `App`.
pub struct ActionState<'a> {
    pub tool: Tool,
    pub shape: ShapePrimitive,
    pub has_undo: bool,
    pub has_redo: bool,
    pub has_selection: bool,
    pub selection_idle: bool,
    pub has_clipboard: bool,
    pub has_voxels: bool,
    pub dialog_busy: bool,
    pub flyby_active: bool,
    pub palettes: &'a [Palette],
    pub palette_choice: usize,
    pub active_colors: &'a [Color8],
    pub current_color: Color8,
    pub prefs: &'a Preferences,
    pub recent: &'a [PathBuf],
}

impl ActionState<'_> {
    fn active_palette(&self) -> Option<&Palette> {
        self.palettes.get(self.palette_choice)
    }

    fn active_is_builtin(&self) -> bool {
        self.active_palette().is_none_or(|p| p.builtin)
    }
}

impl Action {
    /// Every action without an index, in palette/menu order.
    pub const STATIC: &'static [Action] = &[
        Action::NewProject,
        Action::OpenProject,
        Action::SaveProject,
        Action::SaveProjectAs,
        Action::Import(ImportFormat::Vox),
        Action::Import(ImportFormat::Qb),
        Action::Import(ImportFormat::Gox),
        Action::Export(ExportFormat::Vox),
        Action::Export(ExportFormat::Gox),
        Action::Export(ExportFormat::Obj),
        Action::Export(ExportFormat::Gltf),
        Action::Export(ExportFormat::Png),
        Action::ExportShot,
        Action::Export(ExportFormat::Svg),
        Action::Undo,
        Action::Redo,
        Action::DeleteSelectionContents,
        Action::ClearSelection,
        Action::Cut,
        Action::Copy,
        Action::Paste,
        Action::DoubleDensity,
        Action::HalveDensity,
        Action::SelectTool(Tool::Brush),
        Action::SelectTool(Tool::Erase),
        Action::SelectTool(Tool::Paint),
        Action::SelectTool(Tool::Eyedropper),
        Action::SelectTool(Tool::Shape),
        Action::SelectTool(Tool::Select),
        Action::SelectTool(Tool::Move),
        Action::SelectShape(ShapePrimitive::Rectangle),
        Action::SelectShape(ShapePrimitive::Ellipse),
        Action::SelectShape(ShapePrimitive::Line),
        Action::SelectShape(ShapePrimitive::Sphere),
        Action::FrameView,
        Action::ViewPreset(CameraPreset::Front),
        Action::ViewPreset(CameraPreset::Back),
        Action::ViewPreset(CameraPreset::Right),
        Action::ViewPreset(CameraPreset::Left),
        Action::ViewPreset(CameraPreset::Top),
        Action::ViewPreset(CameraPreset::Iso),
        Action::ZoomIn,
        Action::ZoomOut,
        Action::ToggleFlyby,
        Action::ToggleFloorGrid,
        Action::ToggleOriginAxes,
        Action::AddColorToPalette,
        Action::NewPalette,
        Action::DuplicatePalette,
        Action::DeletePalette,
        Action::ImportAse,
        Action::ExportAse,
        Action::Preferences,
        Action::SetTheme(ThemePref::System),
        Action::SetTheme(ThemePref::Light),
        Action::SetTheme(ThemePref::Dark),
        Action::Changelog,
    ];

    /// Every action the command palette lists for `s`: [`Action::STATIC`]
    /// plus one entry per recent file and per other palette.
    pub fn all(s: &ActionState) -> Vec<Action> {
        let mut out = Action::STATIC.to_vec();
        out.extend((0..s.recent.len()).map(Action::OpenRecent));
        out.extend(
            (0..s.palettes.len())
                .filter(|&i| i != s.palette_choice)
                .map(Action::SelectPalette),
        );
        out
    }

    pub fn shortcut(self) -> Option<Chord> {
        self.spec().shortcut
    }

    /// Shortcut text for tooltips and menus, e.g. "⌘Z" or "Ctrl+Z".
    pub fn shortcut_text(self) -> Option<String> {
        self.shortcut().map(|c| c.display(Platform::CURRENT))
    }

    pub fn spec(self) -> ActionSpec {
        use Category as C;
        use KeyCode as K;
        match self {
            Action::NewProject => spec(
                "New project…",
                "New…",
                C::File,
                "create empty",
                Some(Chord::cmd(K::KeyN)),
            ),
            Action::OpenProject => spec(
                "Open project…",
                "Open…",
                C::File,
                "roxel load file",
                Some(Chord::cmd(K::KeyO)),
            ),
            Action::OpenRecent(_) => spec("Open recent", "Open recent", C::File, "recent", None),
            Action::SaveProject => spec(
                "Save project",
                "Save",
                C::File,
                "roxel write file",
                Some(Chord::cmd(K::KeyS)),
            ),
            Action::SaveProjectAs => spec(
                "Save project as…",
                "Save As…",
                C::File,
                "roxel write file save as",
                Some(Chord::cmd_shift(K::KeyS)),
            ),
            Action::Import(f) => f.spec(),
            Action::Export(f) => f.spec(),
            Action::ExportShot => spec(
                "Export Shot…",
                "Roxel Shot (PNG)…",
                C::File,
                "save write png image share social shot dribbble render poster",
                None,
            ),

            Action::Undo => spec(
                "Undo",
                "Undo",
                C::Edit,
                "history",
                Some(Chord::cmd(K::KeyZ)),
            ),
            Action::Redo => spec(
                "Redo",
                "Redo",
                C::Edit,
                "history",
                Some(Chord::cmd_shift(K::KeyZ)),
            ),
            Action::DeleteSelectionContents => spec(
                "Delete selection contents",
                "Delete Selection Contents",
                C::Edit,
                "clear remove voxels",
                Some(Chord::key(K::Backspace)),
            ),
            Action::ClearSelection => spec(
                "Clear selection",
                "Clear Selection",
                C::Edit,
                "deselect",
                Some(Chord::cmd(K::KeyD)),
            ),
            Action::Copy => spec(
                "Copy",
                "Copy",
                C::Edit,
                "copy clipboard duplicate",
                Some(Chord::cmd(K::KeyC)),
            ),
            Action::Cut => spec(
                "Cut",
                "Cut",
                C::Edit,
                "cut clipboard",
                Some(Chord::cmd(K::KeyX)),
            ),
            Action::Paste => spec(
                "Paste",
                "Paste",
                C::Edit,
                "paste clipboard insert",
                Some(Chord::cmd(K::KeyV)),
            ),
            Action::DoubleDensity => spec(
                "Double density",
                "Double Density",
                C::Edit,
                "resample upscale 2x detail subdivide",
                None,
            ),
            Action::HalveDensity => spec(
                "Halve density",
                "Halve Density",
                C::Edit,
                "resample downscale half simplify merge",
                None,
            ),

            Action::SelectTool(tool) => {
                let (label, menu_label, keywords, key) = match tool {
                    Tool::Brush => ("Switch to Brush", "Brush", "paint place draw", K::KeyB),
                    Tool::Erase => ("Switch to Erase", "Erase", "delete remove", K::KeyE),
                    Tool::Paint => (
                        "Switch to Paint",
                        "Paint",
                        "recolor bucket flood fill region selection",
                        K::KeyP,
                    ),
                    Tool::Eyedropper => (
                        "Switch to Eyedropper",
                        "Eyedropper",
                        "pick sample color",
                        K::KeyI,
                    ),
                    Tool::Shape => (
                        "Switch to Shape",
                        "Shape",
                        "rectangle ellipse line",
                        K::KeyS,
                    ),
                    Tool::Select => ("Switch to Select", "Select", "marquee region", K::KeyM),
                    Tool::Move => ("Switch to Move", "Move", "translate nudge", K::KeyV),
                };
                spec(label, menu_label, C::Tools, keywords, Some(Chord::key(key)))
            }
            Action::SelectShape(prim) => {
                let (label, menu_label, keywords) = match prim {
                    ShapePrimitive::Rectangle => {
                        ("Shape: Rectangle", "Rectangle", "rect square box")
                    }
                    ShapePrimitive::Ellipse => ("Shape: Ellipse", "Ellipse", "circle oval"),
                    ShapePrimitive::Line => ("Shape: Line", "Line", "stroke"),
                    ShapePrimitive::Sphere => {
                        ("Shape: Sphere", "Sphere", "ball ellipsoid globe round")
                    }
                };
                spec(label, menu_label, C::Shape, keywords, None)
            }

            Action::FrameView => spec(
                "Frame view (fit to content)",
                "Frame View",
                C::View,
                "zoom fit center reframe",
                Some(Chord::cmd(K::Digit0)),
            ),
            Action::ViewPreset(preset) => {
                let (label, keywords, shortcut) = match preset {
                    CameraPreset::Front => (
                        "Front view",
                        "view angle camera +z forward",
                        Chord::cmd(K::Digit1),
                    ),
                    CameraPreset::Back => (
                        "Back view",
                        "view angle camera -z behind",
                        Chord::cmd_shift(K::Digit1),
                    ),
                    CameraPreset::Right => (
                        "Right view",
                        "view angle camera +x side",
                        Chord::cmd(K::Digit3),
                    ),
                    CameraPreset::Left => (
                        "Left view",
                        "view angle camera -x side",
                        Chord::cmd_shift(K::Digit3),
                    ),
                    CameraPreset::Top => (
                        "Top view",
                        "view angle camera down plan",
                        Chord::cmd(K::Digit7),
                    ),
                    CameraPreset::Iso => (
                        "Isometric view",
                        "view angle camera isometric default",
                        Chord::cmd(K::Digit5),
                    ),
                };
                spec(label, preset.label(), C::View, keywords, Some(shortcut))
            }
            Action::ZoomIn => spec(
                "Zoom in",
                "Zoom In",
                C::View,
                "camera closer",
                Some(Chord::cmd(K::Equal)),
            ),
            Action::ZoomOut => spec(
                "Zoom out",
                "Zoom Out",
                C::View,
                "camera farther",
                Some(Chord::cmd(K::Minus)),
            ),
            Action::ToggleFlyby => spec(
                "Start flyby camera",
                "Flyby Camera",
                C::View,
                "drone tour cinematic auto orbit preview",
                None,
            ),
            Action::ToggleFloorGrid => spec(
                "Show floor grid",
                "Floor Grid",
                C::View,
                "voxel cell lines on floor toggle",
                None,
            ),
            Action::ToggleOriginAxes => spec(
                "Show origin axes",
                "Origin Axes",
                C::View,
                "red green blue center point triad toggle",
                None,
            ),

            Action::SelectPalette(_) => spec(
                "Switch to palette",
                "Switch to palette",
                C::Palette,
                "",
                None,
            ),
            Action::AddColorToPalette => spec(
                "Add current color to palette",
                "Add Current Color",
                C::Palette,
                "swatch insert",
                None,
            ),
            Action::NewPalette => spec(
                "New palette",
                "New palette",
                C::Palette,
                "create empty",
                None,
            ),
            Action::DuplicatePalette => spec(
                "Duplicate palette",
                "Duplicate",
                C::Palette,
                "copy clone",
                None,
            ),
            Action::DeletePalette => {
                spec("Delete palette", "Delete", C::Palette, "remove trash", None)
            }
            Action::ImportAse => spec(
                "Import .ase palette…",
                "Import .ase…",
                C::Palette,
                "adobe swatch exchange load",
                None,
            ),
            Action::ExportAse => spec(
                "Export .ase palette…",
                "Export .ase…",
                C::Palette,
                "adobe swatch exchange save",
                None,
            ),

            Action::Preferences => spec(
                "Open Preferences…",
                "Preferences…",
                C::View,
                "settings options config",
                Some(Chord::cmd(K::Comma)),
            ),
            Action::SetTheme(theme) => {
                let (label, menu_label, keywords) = match theme {
                    ThemePref::System => ("Theme: System", "System", "appearance auto os"),
                    ThemePref::Light => ("Theme: Light", "Light", "appearance"),
                    ThemePref::Dark => ("Theme: Dark", "Dark", "appearance"),
                };
                spec(label, menu_label, C::Preferences, keywords, None)
            }
            Action::Changelog => spec(
                "Open Changelog",
                "Changelog",
                C::Help,
                "release notes github",
                None,
            ),
        }
    }

    /// Standalone label for `s` — the command palette's text.
    pub fn label(self, s: &ActionState) -> Cow<'static, str> {
        match self {
            Action::OpenRecent(i) => s
                .recent
                .get(i)
                .map(|p| format!("Open recent: {}", file_name(p)).into())
                .unwrap_or(Cow::Borrowed(self.spec().label)),
            Action::SelectPalette(i) => s
                .palettes
                .get(i)
                .map(|p| format!("Switch to palette: {}", p.name).into())
                .unwrap_or(Cow::Borrowed(self.spec().label)),
            Action::ToggleFlyby if s.flyby_active => "Stop flyby camera".into(),
            Action::ToggleFloorGrid if s.prefs.show_floor_grid => "Hide floor grid".into(),
            Action::ToggleOriginAxes if s.prefs.show_origin_axes => "Hide origin axes".into(),
            _ => self.spec().label.into(),
        }
    }

    /// Search keywords for `s`.
    pub fn keywords(self, s: &ActionState) -> &'static str {
        match self {
            Action::SelectPalette(i) => match s.palettes.get(i) {
                Some(p) if !p.builtin => "user",
                _ => "built-in",
            },
            _ => self.spec().keywords,
        }
    }

    pub fn enabled(self, s: &ActionState) -> bool {
        let dialog_ok = !s.dialog_busy;
        match self {
            Action::NewProject => true,
            Action::OpenProject
            | Action::SaveProject
            | Action::SaveProjectAs
            | Action::Import(_)
            | Action::Export(_)
            | Action::ImportAse => dialog_ok,
            Action::OpenRecent(i) => i < s.recent.len(),
            Action::ExportShot => s.has_voxels,
            Action::Undo => s.has_undo,
            Action::Redo => s.has_redo,
            Action::DeleteSelectionContents => s.has_selection && s.selection_idle,
            Action::ClearSelection | Action::Copy | Action::Cut => s.has_selection,
            Action::Paste => s.has_clipboard,
            Action::DoubleDensity | Action::HalveDensity => s.has_voxels,
            Action::SelectTool(t) => s.tool != t,
            Action::SelectShape(p) => s.shape != p,
            Action::FrameView
            | Action::ViewPreset(_)
            | Action::ZoomIn
            | Action::ZoomOut
            | Action::ToggleFlyby
            | Action::ToggleFloorGrid
            | Action::ToggleOriginAxes
            | Action::NewPalette
            | Action::Preferences
            | Action::Changelog => true,
            Action::SelectPalette(i) => i < s.palettes.len() && i != s.palette_choice,
            Action::AddColorToPalette => {
                !s.active_is_builtin() && !s.active_colors.contains(&s.current_color)
            }
            Action::DuplicatePalette => s.active_palette().is_some(),
            Action::DeletePalette => !s.active_is_builtin(),
            Action::ExportAse => dialog_ok && s.active_palette().is_some(),
            Action::SetTheme(t) => s.prefs.theme != t,
        }
    }
}

fn file_name(p: &Path) -> &str {
    p.file_name()
        .and_then(|s| s.to_str())
        .unwrap_or_else(|| p.to_str().unwrap_or("file"))
}

/// Where an action was triggered. Paste lands under the cursor only for
/// `Canvas` triggers (a shortcut or the macOS menu bar); from egui chrome the
/// pointer is over the UI, not the scene.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Origin {
    Canvas,
    Ui,
}

#[derive(Resource, Default)]
pub struct ActionQueue(pub Vec<(Action, Origin)>);

impl ActionQueue {
    pub fn push(&mut self, action: Action, origin: Origin) {
        self.0.push((action, origin));
    }
}

#[derive(SystemParam)]
pub struct DocParams<'w> {
    pending: ResMut<'w, PendingDialog>,
    history: ResMut<'w, History>,
    grid: ResMut<'w, GridResource>,
    guard: ResMut<'w, UnsavedGuard>,
    current_path: Res<'w, CurrentProjectPath>,
    recent: Res<'w, RecentFiles>,
    toasts: ResMut<'w, Toasts>,
    shot_panel: ResMut<'w, crate::shot::ShotPanel>,
}

#[derive(SystemParam)]
pub struct EditParams<'w, 's> {
    selection: ResMut<'w, Selection>,
    clipboard: ResMut<'w, crate::clipboard::Clipboard>,
    color: Res<'w, CurrentColor>,
    cursor: crate::clipboard::CursorView<'w, 's>,
}

#[derive(SystemParam)]
pub struct ViewParams<'w, 's> {
    tool: ResMut<'w, ToolState>,
    shape: ResMut<'w, ShapeOptions>,
    view_preset: ResMut<'w, PendingViewPreset>,
    frame_view: ResMut<'w, PendingFrameView>,
    flyby: ResMut<'w, FlybyState>,
    cameras: Query<'w, 's, &'static mut PanOrbitCamera>,
}

#[derive(SystemParam)]
pub struct PaletteOpParams<'w> {
    palettes: ResMut<'w, Palettes>,
    choice: ResMut<'w, PaletteChoice>,
    working: ResMut<'w, WorkingPalette>,
    discard: ResMut<'w, DiscardConfirm>,
    delete: ResMut<'w, DeleteConfirm>,
}

#[derive(SystemParam)]
pub struct AppParams<'w> {
    prefs: ResMut<'w, Preferences>,
    prefs_window: ResMut<'w, PreferencesWindow>,
}

#[derive(SystemParam)]
pub struct ActionContext<'w, 's> {
    doc: DocParams<'w>,
    edit: EditParams<'w, 's>,
    view: ViewParams<'w, 's>,
    pal: PaletteOpParams<'w>,
    app: AppParams<'w>,
}

pub fn apply_actions_system(mut queue: ResMut<ActionQueue>, mut cx: ActionContext) {
    for (action, origin) in std::mem::take(&mut queue.0) {
        dispatch(action, origin, &mut cx);
    }
}

pub fn dispatch(action: Action, origin: Origin, cx: &mut ActionContext) {
    let ActionContext {
        doc,
        edit,
        view,
        pal,
        app,
    } = cx;
    let last_dir = app.prefs.last_dir.clone();
    match action {
        Action::NewProject => doc.guard.request(GuardedAction::New),
        Action::OpenProject => doc.guard.request(GuardedAction::Open(None)),
        Action::OpenRecent(i) => {
            if let Some(path) = doc.recent.0.get(i).cloned() {
                doc.guard.request(GuardedAction::Open(Some(path)));
            }
        }
        Action::SaveProject => crate::ui::spawn_save(&mut doc.pending, &doc.current_path, last_dir),
        Action::SaveProjectAs => {
            crate::ui::spawn_save_as(&mut doc.pending, &doc.current_path, last_dir)
        }
        Action::Import(f) => crate::ui::spawn_import(
            &mut doc.pending,
            f.filter_name(),
            f.ext(),
            f.wrap(),
            last_dir,
        ),
        Action::Export(f) => crate::ui::spawn_export(
            &mut doc.pending,
            f.filter_name(),
            f.ext(),
            f.default_name(),
            f.wrap(),
            last_dir,
        ),
        // Opens the tweak panel; its "Export…" button spawns the save dialog.
        Action::ExportShot => {
            if doc.grid.count() == 0 {
                doc.toasts.error("Nothing to export — the scene is empty");
            } else {
                doc.shot_panel.open_panel();
            }
        }

        Action::Undo => doc.history.undo(&mut doc.grid),
        Action::Redo => doc.history.redo(&mut doc.grid),
        Action::DeleteSelectionContents => {
            if edit.selection.aabb.is_some() {
                crate::select::clear_selection(&mut doc.grid, &mut doc.history, &edit.selection);
            }
        }
        Action::ClearSelection => edit.selection.clear(),
        Action::Copy => {
            if let Some(stamp) = crate::clipboard::copy_selection(&doc.grid, &edit.selection) {
                let n = stamp.voxel_count();
                edit.clipboard.stamp = Some(stamp);
                doc.toasts.info(format!("Copied {n} voxels"));
            }
        }
        Action::Cut => {
            if let Some(stamp) =
                crate::clipboard::cut_selection(&mut doc.grid, &mut doc.history, &edit.selection)
            {
                let n = stamp.voxel_count();
                edit.clipboard.stamp = Some(stamp);
                doc.toasts.info(format!("Cut {n} voxels"));
            }
        }
        Action::Paste => {
            if let Some(stamp) = edit.clipboard.stamp.clone() {
                let at_cursor = match origin {
                    Origin::Canvas => edit.cursor.paste_target(&doc.grid),
                    Origin::Ui => None,
                };
                crate::clipboard::execute_paste(
                    &mut doc.grid,
                    &mut doc.history,
                    &mut edit.selection,
                    &mut doc.toasts,
                    &stamp,
                    at_cursor,
                );
            }
        }
        Action::DoubleDensity => crate::resample::apply_resample(
            &mut doc.grid,
            &mut doc.history,
            &mut doc.toasts,
            crate::resample::ResampleOp::Double,
        ),
        Action::HalveDensity => crate::resample::apply_resample(
            &mut doc.grid,
            &mut doc.history,
            &mut doc.toasts,
            crate::resample::ResampleOp::Halve,
        ),

        Action::SelectTool(t) => {
            if view.tool.current != t {
                view.tool.previous = view.tool.current;
                view.tool.current = t;
            }
        }
        Action::SelectShape(prim) => {
            view.shape.primitive = prim;
            app.prefs.last_shape = prim;
            save_preferences(&app.prefs);
        }

        Action::FrameView => view.frame_view.0 = true,
        Action::ViewPreset(preset) => view.view_preset.0 = Some(preset),
        Action::ZoomIn | Action::ZoomOut => {
            let factor = if action == Action::ZoomIn {
                ZOOM_STEP_IN
            } else {
                ZOOM_STEP_OUT
            };
            for mut cam in &mut view.cameras {
                cam.target_radius = apply_zoom(
                    cam.target_radius,
                    factor,
                    cam.zoom_lower_limit,
                    cam.zoom_upper_limit,
                );
            }
        }
        Action::ToggleFlyby => {
            view.flyby.active = !view.flyby.active;
            if view.flyby.active {
                doc.toasts.info("Flyby active — Esc to exit");
            }
        }
        Action::ToggleFloorGrid => {
            app.prefs.show_floor_grid = !app.prefs.show_floor_grid;
            save_preferences(&app.prefs);
        }
        Action::ToggleOriginAxes => {
            app.prefs.show_origin_axes = !app.prefs.show_origin_axes;
            save_preferences(&app.prefs);
        }

        Action::SelectPalette(i) => {
            if i < pal.palettes.0.len() {
                crate::ui::request_select(i, &mut pal.choice, &mut pal.working, &mut pal.discard);
            }
        }
        Action::AddColorToPalette => {
            let i = active_index(pal);
            if !crate::ui::display_colors(&pal.palettes, &pal.working, i).contains(&edit.color.0) {
                let (colors, persist) =
                    crate::ui::edit_colors(&mut pal.palettes, &mut pal.working, i);
                colors.push(edit.color.0);
                if persist {
                    roxel::io::palettes::save(&pal.palettes.0);
                }
            }
        }
        Action::NewPalette => {
            pal.working.clear();
            let name = crate::ui::next_palette_name(&pal.palettes.0);
            pal.palettes.0.push(Palette {
                name,
                colors: Vec::new(),
                builtin: false,
            });
            pal.choice.0 = pal.palettes.0.len() - 1;
            roxel::io::palettes::save(&pal.palettes.0);
        }
        Action::DuplicatePalette => {
            pal.choice.0 = active_index(pal);
            crate::ui::save_as_new(&mut pal.palettes, &mut pal.choice, &mut pal.working);
            roxel::io::palettes::save(&pal.palettes.0);
        }
        Action::DeletePalette => pal.delete.request(&pal.palettes, pal.choice.0),
        Action::ImportAse => crate::ui::spawn_import(
            &mut doc.pending,
            "Adobe Swatch Exchange",
            "ase",
            DialogResult::ImportAse,
            last_dir,
        ),
        Action::ExportAse => {
            let i = active_index(pal);
            if let Some(p) = pal.palettes.0.get(i) {
                let name = p.name.clone();
                let colors = crate::ui::display_colors(&pal.palettes, &pal.working, i).to_vec();
                crate::ui::spawn_export_ase(&mut doc.pending, name, colors, last_dir);
            }
        }

        Action::Preferences => app.prefs_window.open = !app.prefs_window.open,
        Action::SetTheme(t) => {
            app.prefs.theme = t;
            save_preferences(&app.prefs);
        }
        Action::Changelog => crate::updater::open_url(CHANGELOG_URL),
    }
}

fn active_index(pal: &PaletteOpParams) -> usize {
    pal.choice.0.min(pal.palettes.0.len().saturating_sub(1))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state<'a>(palettes: &'a [Palette], prefs: &'a Preferences) -> ActionState<'a> {
        ActionState {
            tool: Tool::Brush,
            shape: ShapePrimitive::Rectangle,
            has_undo: false,
            has_redo: false,
            has_selection: false,
            selection_idle: true,
            has_clipboard: false,
            has_voxels: false,
            dialog_busy: false,
            flyby_active: false,
            palettes,
            palette_choice: 0,
            active_colors: &[],
            current_color: [10, 20, 30, 255],
            prefs,
            recent: &[],
        }
    }

    fn pal(name: &str, builtin: bool) -> Palette {
        Palette {
            name: name.into(),
            colors: vec![[1, 2, 3, 255]],
            builtin,
        }
    }

    #[test]
    fn chord_display_follows_platform_conventions() {
        let save_as = Chord::cmd_shift(KeyCode::KeyS);
        assert_eq!(save_as.display(Platform::Mac), "⇧⌘S");
        assert_eq!(save_as.display(Platform::Other), "Ctrl+Shift+S");
        assert_eq!(Chord::cmd(KeyCode::Digit0).display(Platform::Mac), "⌘0");
        assert_eq!(
            Chord::cmd(KeyCode::Comma).display(Platform::Other),
            "Ctrl+,"
        );
        assert_eq!(Chord::key(KeyCode::KeyB).display(Platform::Other), "B");
        assert_eq!(Chord::key(KeyCode::Backspace).display(Platform::Mac), "⌫");
        assert_eq!(
            Chord::key(KeyCode::Backspace).display(Platform::Other),
            "Backspace"
        );
    }

    #[test]
    fn chord_tokens_put_modifiers_in_platform_order() {
        let save_as = Chord::cmd_shift(KeyCode::KeyS);
        assert_eq!(
            save_as.tokens(Platform::Mac),
            vec![
                KeyToken::Shift,
                KeyToken::Command,
                KeyToken::Key("S".into())
            ]
        );
        assert_eq!(
            save_as.tokens(Platform::Other),
            vec![
                KeyToken::Control,
                KeyToken::Shift,
                KeyToken::Key("S".into())
            ]
        );
    }

    #[test]
    fn static_actions_are_unique() {
        for (i, a) in Action::STATIC.iter().enumerate() {
            assert!(!Action::STATIC[..i].contains(a), "{a:?} listed twice");
        }
    }

    #[test]
    fn every_import_and_export_format_is_listed() {
        for f in [ImportFormat::Vox, ImportFormat::Qb, ImportFormat::Gox] {
            assert!(Action::STATIC.contains(&Action::Import(f)));
            assert!(f.spec().label.contains(&format!("(.{})", f.ext())));
        }
        for f in [
            ExportFormat::Vox,
            ExportFormat::Gox,
            ExportFormat::Obj,
            ExportFormat::Gltf,
            ExportFormat::Png,
            ExportFormat::Svg,
        ] {
            assert!(Action::STATIC.contains(&Action::Export(f)));
            assert!(f.default_name().ends_with(&format!(".{}", f.ext())));
        }
    }

    #[test]
    fn all_adds_recent_files_and_other_palettes() {
        let palettes = vec![pal("A", true), pal("B", false)];
        let prefs = Preferences::default();
        let recent = vec![PathBuf::from("/tmp/a.rox"), PathBuf::from("/tmp/b.rox")];
        let mut s = state(&palettes, &prefs);
        s.recent = &recent;
        let all = Action::all(&s);
        assert!(all.contains(&Action::OpenRecent(1)));
        assert!(all.contains(&Action::SelectPalette(1)));
        assert!(!all.contains(&Action::SelectPalette(0)));
    }

    #[test]
    fn toggle_labels_follow_state() {
        let palettes = vec![pal("A", true)];
        let mut prefs = Preferences::default();
        prefs.show_floor_grid = true;
        let mut s = state(&palettes, &prefs);
        assert_eq!(Action::ToggleFloorGrid.label(&s), "Hide floor grid");
        assert_eq!(Action::ToggleFlyby.label(&s), "Start flyby camera");
        s.flyby_active = true;
        assert_eq!(Action::ToggleFlyby.label(&s), "Stop flyby camera");
    }

    #[test]
    fn selection_actions_need_a_selection() {
        let palettes = vec![pal("A", true)];
        let prefs = Preferences::default();
        let mut s = state(&palettes, &prefs);
        assert!(!Action::Copy.enabled(&s));
        s.has_selection = true;
        assert!(Action::Copy.enabled(&s));
        assert!(Action::DeleteSelectionContents.enabled(&s));
        s.selection_idle = false;
        assert!(!Action::DeleteSelectionContents.enabled(&s));
    }

    #[test]
    fn file_dialog_actions_wait_for_an_open_dialog() {
        let palettes = vec![pal("A", true)];
        let prefs = Preferences::default();
        let mut s = state(&palettes, &prefs);
        s.dialog_busy = true;
        assert!(!Action::OpenProject.enabled(&s));
        assert!(!Action::Export(ExportFormat::Obj).enabled(&s));
        assert!(Action::NewProject.enabled(&s));
    }

    #[test]
    fn current_tool_is_disabled() {
        let palettes = vec![pal("A", true)];
        let prefs = Preferences::default();
        let s = state(&palettes, &prefs);
        assert!(!Action::SelectTool(Tool::Brush).enabled(&s));
        assert!(Action::SelectTool(Tool::Erase).enabled(&s));
    }

    #[test]
    fn shortcut_text_uses_the_current_platform() {
        let expected = if cfg!(target_os = "macos") {
            "⌘Z"
        } else {
            "Ctrl+Z"
        };
        assert_eq!(Action::Undo.shortcut_text().as_deref(), Some(expected));
        assert_eq!(Action::DoubleDensity.shortcut_text(), None);
    }
}
