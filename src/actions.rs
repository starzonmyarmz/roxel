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
use crate::tools::{CurrentColor, ExtraColors, RecentColors, ShapeOptions, Tool, ToolState};
use crate::ui::{
    CommandPalette, CurrentProjectPath, DeleteConfirm, DialogResult, DiscardConfirm, GuardedAction,
    Palette, PaletteChoice, PaletteRenameState, PaletteSwitcher, Palettes, PendingDialog,
    RecentFiles, Toasts, UnsavedGuard, WorkingPalette,
};
use roxel::color_space::ColorSpace;
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

/// Modifiers held this frame. Super and Ctrl both count as `cmd` on every
/// platform.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Mods {
    pub cmd: bool,
    pub shift: bool,
    pub alt: bool,
}

impl Mods {
    pub fn from_keys(keys: &ButtonInput<KeyCode>) -> Self {
        Self {
            cmd: keys.any_pressed([
                KeyCode::SuperLeft,
                KeyCode::SuperRight,
                KeyCode::ControlLeft,
                KeyCode::ControlRight,
            ]),
            shift: keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]),
            alt: keys.any_pressed([KeyCode::AltLeft, KeyCode::AltRight]),
        }
    }
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

    /// Whether this chord fired this frame. Cmd and Alt must match exactly, and
    /// so must Shift unless the chord is a bare key (B, F, Backspace), which
    /// works with Shift held. Numpad
    /// numpad digits, numpad +/- and Delete count as their main-keyboard keys.
    pub fn matches(self, mods: Mods, just_pressed: impl Fn(KeyCode) -> bool) -> bool {
        let shift_matters = self.cmd || self.shift;
        if mods.cmd != self.cmd || (shift_matters && mods.shift != self.shift) || mods.alt {
            return false;
        }
        just_pressed(self.key) || key_alias(self.key).is_some_and(&just_pressed)
    }
}

fn key_alias(key: KeyCode) -> Option<KeyCode> {
    Some(match key {
        KeyCode::Digit0 => KeyCode::Numpad0,
        KeyCode::Digit1 => KeyCode::Numpad1,
        KeyCode::Digit3 => KeyCode::Numpad3,
        KeyCode::Digit5 => KeyCode::Numpad5,
        KeyCode::Digit7 => KeyCode::Numpad7,
        KeyCode::Equal => KeyCode::NumpadAdd,
        KeyCode::Minus => KeyCode::NumpadSubtract,
        KeyCode::Backspace => KeyCode::Delete,
        _ => return None,
    })
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
    ClearRecent,
    SaveProject,
    SaveProjectAs,
    Import(ImportFormat),
    Export(ExportFormat),
    ExportShot,
    Quit,

    Undo,
    Redo,
    FillSelection,
    DeleteSelectionContents,
    SelectAll,
    ClearSelection,
    Copy,
    Cut,
    Paste,
    DoubleDensity,
    HalveDensity,

    SelectTool(Tool),
    SelectShape(ShapePrimitive),
    CycleShape,

    FrameView,
    ViewPreset(CameraPreset),
    ZoomIn,
    ZoomOut,
    ToggleFlyby,
    ToggleFloorGrid,
    ToggleOriginAxes,
    SetColorSpace(ColorSpace),

    SwitchPalette,
    SelectPalette(usize),
    AddColorToPalette,
    NewPalette,
    DuplicatePalette,
    RenamePalette,
    DeletePalette,
    ImportAse,
    ExportAse,

    ShowCommandPalette,
    Preferences,
    SetTheme(ThemePref),
    ShowOnboarding,
    Changelog,
    CheckForUpdates,
    About,
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
    pub checking_updates: bool,
    pub palettes: &'a [Palette],
    pub palette_choice: usize,
    pub active_colors: &'a [Color8],
    pub current_color: Color8,
    pub prefs: &'a Preferences,
    pub recent: &'a [PathBuf],
}

/// Read-only system params that build an [`ActionState`].
#[derive(SystemParam)]
pub struct ActionStateParams<'w> {
    tool: Res<'w, ToolState>,
    shape: Res<'w, ShapeOptions>,
    history: Res<'w, History>,
    selection: Res<'w, Selection>,
    select_state: Res<'w, crate::select::SelectState>,
    clipboard: Res<'w, crate::clipboard::Clipboard>,
    grid: Res<'w, GridResource>,
    pending: Res<'w, PendingDialog>,
    flyby: Res<'w, FlybyState>,
    updater: Res<'w, crate::updater::UpdateCheck>,
    palettes: Res<'w, Palettes>,
    choice: Res<'w, PaletteChoice>,
    working: Res<'w, WorkingPalette>,
    color: Res<'w, CurrentColor>,
    prefs: Res<'w, Preferences>,
    recent: Res<'w, RecentFiles>,
}

impl ActionStateParams<'_> {
    pub fn get(&self) -> ActionState<'_> {
        let choice = self.choice.0.min(self.palettes.0.len().saturating_sub(1));
        ActionState {
            tool: self.tool.current,
            shape: self.shape.primitive,
            has_undo: !self.history.undo.is_empty(),
            has_redo: !self.history.redo.is_empty(),
            has_selection: self.selection.aabb.is_some(),
            selection_idle: self.select_state.phase == crate::select::SelectPhase::Idle,
            has_clipboard: self.clipboard.has_stamp(),
            has_voxels: self.grid.count() > 0,
            dialog_busy: self.pending.is_active(),
            flyby_active: self.flyby.active,
            checking_updates: self.updater.is_checking(),
            palettes: &self.palettes.0,
            palette_choice: choice,
            active_colors: crate::ui::display_colors(&self.palettes, &self.working, choice),
            current_color: self.color.0,
            prefs: &self.prefs,
            recent: &self.recent.0,
        }
    }
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
        Action::ClearRecent,
        Action::Quit,
        Action::Undo,
        Action::Redo,
        Action::FillSelection,
        Action::DeleteSelectionContents,
        Action::SelectAll,
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
        Action::CycleShape,
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
        Action::SetColorSpace(ColorSpace::Hex),
        Action::SetColorSpace(ColorSpace::Rgb),
        Action::SetColorSpace(ColorSpace::Hsl),
        Action::SetColorSpace(ColorSpace::Hsb),
        Action::SetColorSpace(ColorSpace::Oklch),
        Action::SwitchPalette,
        Action::AddColorToPalette,
        Action::NewPalette,
        Action::DuplicatePalette,
        Action::RenamePalette,
        Action::DeletePalette,
        Action::ImportAse,
        Action::ExportAse,
        Action::ShowCommandPalette,
        Action::Preferences,
        Action::SetTheme(ThemePref::System),
        Action::SetTheme(ThemePref::Light),
        Action::SetTheme(ThemePref::Dark),
        Action::ShowOnboarding,
        Action::Changelog,
        Action::CheckForUpdates,
        Action::About,
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
            Action::ClearRecent => spec(
                "Clear recent files",
                "Clear Menu",
                C::File,
                "recent history forget",
                None,
            ),
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
            Action::Quit => spec(
                "Quit Roxel",
                QUIT_MENU_LABEL,
                C::File,
                "exit close",
                if cfg!(target_os = "macos") {
                    Some(Chord::cmd(K::KeyQ))
                } else {
                    None
                },
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
            Action::FillSelection => spec(
                "Fill selection",
                "Fill Selection",
                C::Edit,
                "recolor paint bucket",
                Some(Chord::key(K::KeyF)),
            ),
            Action::DeleteSelectionContents => spec(
                "Delete selection contents",
                "Delete Selection Contents",
                C::Edit,
                "clear remove voxels",
                Some(Chord::key(K::Backspace)),
            ),
            Action::SelectAll => spec(
                "Select all",
                "Select All",
                C::Edit,
                "everything voxels",
                Some(Chord::cmd(K::KeyA)),
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
            Action::CycleShape => spec(
                "Next shape",
                "Next Shape",
                C::Shape,
                "cycle rectangle ellipse line sphere",
                Some(Chord::key(K::KeyS)),
            ),

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
            Action::SetColorSpace(space) => {
                let label = match space {
                    ColorSpace::Hex => "Color format: Hex",
                    ColorSpace::Rgb => "Color format: RGB",
                    ColorSpace::Hsl => "Color format: HSL",
                    ColorSpace::Hsb => "Color format: HSB",
                    ColorSpace::Oklch => "Color format: OKLCH",
                };
                spec(
                    label,
                    space.label(),
                    C::View,
                    "color space readout picker",
                    None,
                )
            }

            Action::SwitchPalette => spec(
                "Switch palette…",
                "Switch palette…",
                C::Palette,
                "choose change browse",
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
                "copy clone save as new",
                None,
            ),
            Action::RenamePalette => {
                spec("Rename palette…", "Rename…", C::Palette, "name title", None)
            }
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

            Action::ShowCommandPalette => spec(
                "Command palette",
                "Command Palette…",
                C::View,
                "search actions",
                Some(Chord::cmd(K::KeyK)),
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
            Action::ShowOnboarding => spec(
                "Show onboarding tour",
                "Show Onboarding Tour…",
                C::Help,
                "tutorial intro welcome help",
                None,
            ),
            Action::Changelog => spec(
                "Open Changelog",
                "Changelog",
                C::Help,
                "release notes github",
                None,
            ),
            Action::CheckForUpdates => spec(
                "Check for updates…",
                "Check for Updates…",
                C::Help,
                "version release new download",
                None,
            ),
            Action::About => spec(
                "About Roxel",
                "About Roxel",
                C::Help,
                "version copyright credits",
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
            Action::DuplicatePalette if s.active_is_builtin() => "Save as new palette".into(),
            _ => self.spec().label.into(),
        }
    }

    /// Label for `s` inside a menu.
    pub fn menu_label(self, s: &ActionState) -> Cow<'static, str> {
        match self {
            Action::DuplicatePalette if s.active_is_builtin() => "Save as new palette".into(),
            _ => self.spec().menu_label.into(),
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
            Action::NewProject
            | Action::ClearRecent
            | Action::Quit
            | Action::SetColorSpace(_)
            | Action::ShowCommandPalette
            | Action::ShowOnboarding
            | Action::About => true,
            Action::CheckForUpdates => !s.checking_updates,
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
            Action::FillSelection | Action::DeleteSelectionContents => {
                s.has_selection && s.selection_idle
            }
            Action::ClearSelection | Action::Copy | Action::Cut => s.has_selection,
            Action::Paste => s.has_clipboard,
            Action::SelectAll | Action::DoubleDensity | Action::HalveDensity => s.has_voxels,
            Action::SelectTool(t) => s.tool != t,
            Action::SelectShape(p) => s.shape != p,
            // Shares S with SelectTool(Shape), which is disabled once Shape is
            // the current tool.
            Action::CycleShape => s.tool == Tool::Shape,
            Action::FrameView
            | Action::ViewPreset(_)
            | Action::ZoomIn
            | Action::ZoomOut
            | Action::ToggleFlyby
            | Action::ToggleFloorGrid
            | Action::ToggleOriginAxes
            | Action::NewPalette
            | Action::SwitchPalette
            | Action::Preferences
            | Action::Changelog => true,
            Action::SelectPalette(i) => i < s.palettes.len() && i != s.palette_choice,
            Action::AddColorToPalette => {
                s.active_palette().is_some() && !s.active_colors.contains(&s.current_color)
            }
            Action::DuplicatePalette => s.active_palette().is_some(),
            Action::RenamePalette | Action::DeletePalette => !s.active_is_builtin(),
            Action::ExportAse => dialog_ok && s.active_palette().is_some(),
            Action::SetTheme(t) => s.prefs.theme != t,
        }
    }
}

impl Action {
    /// Every chord that triggers this action: its shortcut, plus Ctrl+Y for
    /// Redo off macOS and Cmd+Shift+= (Cmd+Plus) for Zoom In.
    pub fn chords(self) -> impl Iterator<Item = Chord> {
        let extra = match self {
            Action::Redo if Platform::CURRENT == Platform::Other => Some(Chord::cmd(KeyCode::KeyY)),
            Action::ZoomIn => Some(Chord::cmd_shift(KeyCode::Equal)),
            _ => None,
        };
        self.shortcut().into_iter().chain(extra)
    }

    /// Whether the macOS menu must disable this action while a text field has
    /// focus: AppKit would otherwise take its key before the field sees it.
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    pub fn clashes_with_typing(self) -> bool {
        matches!(self, Action::SelectAll | Action::ClearSelection)
            || self.shortcut().is_some_and(|c| !c.cmd)
    }

    /// Shown as a check item in menus.
    pub fn is_check(self) -> bool {
        matches!(
            self,
            Action::ToggleFloorGrid | Action::ToggleOriginAxes | Action::SetColorSpace(_)
        )
    }

    /// Check state for [`Action::is_check`] actions.
    pub fn checked(self, s: &ActionState) -> bool {
        match self {
            Action::ToggleFloorGrid => s.prefs.show_floor_grid,
            Action::ToggleOriginAxes => s.prefs.show_origin_axes,
            Action::SetColorSpace(space) => s.prefs.color_space == space,
            _ => false,
        }
    }
}

const QUIT_MENU_LABEL: &str = if cfg!(target_os = "macos") {
    "Quit Roxel"
} else if cfg!(windows) {
    "Exit"
} else {
    "Quit"
};

fn file_name(p: &Path) -> &str {
    p.file_name()
        .and_then(|s| s.to_str())
        .unwrap_or_else(|| p.to_str().unwrap_or("file"))
}

/// What's holding the keyboard when a shortcut fires.
#[derive(Clone, Copy, Debug, Default)]
pub struct ShortcutGate {
    pub typing: bool,
    pub modal: bool,
    pub palette_open: bool,
    pub prefs_open: bool,
}

impl ShortcutGate {
    /// Shortcuts stay out of focused text fields and open modals, except the
    /// two that close the modal they opened.
    fn allows(self, action: Action) -> bool {
        let free = !self.typing && !self.modal;
        match action {
            Action::ShowCommandPalette => free || self.palette_open,
            Action::Preferences => free || self.prefs_open,
            _ => free,
        }
    }
}

/// The action whose chord fired this frame, if it's enabled and allowed.
pub fn shortcut_action(
    mods: Mods,
    just_pressed: impl Fn(KeyCode) -> bool,
    s: &ActionState,
    gate: ShortcutGate,
) -> Option<Action> {
    Action::STATIC.iter().copied().find(|&action| {
        action.chords().any(|c| c.matches(mods, &just_pressed))
            && action.enabled(s)
            && gate.allows(action)
    })
}

#[derive(SystemParam)]
pub struct ShortcutInput<'w, 's> {
    keys: Res<'w, ButtonInput<KeyCode>>,
    contexts: bevy_egui::EguiContexts<'w, 's>,
    modal: Res<'w, crate::ui::ModalActive>,
    cmd_palette: Res<'w, CommandPalette>,
    prefs_window: Res<'w, PreferencesWindow>,
}

/// The one keyboard-shortcut handler: matches this frame's keys against every
/// action's chords. On macOS a menu key equivalent consumes its key before
/// winit sees it, so this only catches chords the menu bar doesn't own.
pub fn shortcut_system(
    mut input: ShortcutInput,
    state: ActionStateParams,
    mut queue: ResMut<ActionQueue>,
) {
    if input.keys.get_just_pressed().next().is_none() {
        return;
    }
    let gate = ShortcutGate {
        typing: input
            .contexts
            .ctx_mut()
            .map(|c| c.wants_keyboard_input())
            .unwrap_or(false),
        modal: input.modal.0,
        palette_open: input.cmd_palette.open,
        prefs_open: input.prefs_window.open,
    };
    let mods = Mods::from_keys(&input.keys);
    let keys = &input.keys;
    if let Some(action) = shortcut_action(mods, |k| keys.just_pressed(k), &state.get(), gate) {
        queue.push(action, Origin::Canvas);
    }
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
    recent: ResMut<'w, RecentFiles>,
    toasts: ResMut<'w, Toasts>,
    shot_panel: ResMut<'w, crate::shot::ShotPanel>,
}

#[derive(SystemParam)]
pub struct EditParams<'w, 's> {
    selection: ResMut<'w, Selection>,
    clipboard: ResMut<'w, crate::clipboard::Clipboard>,
    color: Res<'w, CurrentColor>,
    extras: Res<'w, ExtraColors>,
    recent_colors: ResMut<'w, RecentColors>,
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
    rename: ResMut<'w, PaletteRenameState>,
    switcher: ResMut<'w, PaletteSwitcher>,
}

#[derive(SystemParam)]
pub struct AppParams<'w> {
    prefs: ResMut<'w, Preferences>,
    prefs_window: ResMut<'w, PreferencesWindow>,
    cmd_palette: ResMut<'w, CommandPalette>,
    onboarding: ResMut<'w, crate::onboarding::Onboarding>,
    updater: ResMut<'w, crate::updater::UpdateCheck>,
    about: ResMut<'w, crate::menu::AboutWindow>,
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
        Action::ClearRecent => doc.recent.clear(),
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
        Action::Quit => doc.guard.request(GuardedAction::Quit),

        Action::Undo => doc.history.undo(&mut doc.grid),
        Action::Redo => doc.history.redo(&mut doc.grid),
        Action::FillSelection => {
            if edit.selection.aabb.is_some() {
                let pool = crate::tools::color_pool(edit.color.0, &edit.extras.0);
                for c in crate::select::recolor_selection(
                    &mut doc.grid,
                    &mut doc.history,
                    &edit.selection,
                    &pool,
                ) {
                    edit.recent_colors.push(c);
                }
            }
        }
        Action::DeleteSelectionContents => {
            if edit.selection.aabb.is_some() {
                crate::select::clear_selection(&mut doc.grid, &mut doc.history, &edit.selection);
            }
        }
        Action::SelectAll => {
            let cells: std::collections::HashSet<IVec3> =
                doc.grid.iter_occupied().map(|(p, _)| p).collect();
            if !cells.is_empty() {
                edit.selection.set_cells(cells);
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

        Action::CycleShape => {
            let next = match view.shape.primitive {
                ShapePrimitive::Rectangle => ShapePrimitive::Ellipse,
                ShapePrimitive::Ellipse => ShapePrimitive::Line,
                ShapePrimitive::Line => ShapePrimitive::Sphere,
                ShapePrimitive::Sphere => ShapePrimitive::Rectangle,
            };
            view.shape.primitive = next;
            app.prefs.last_shape = next;
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
        Action::SetColorSpace(space) => {
            if app.prefs.color_space != space {
                app.prefs.color_space = space;
                save_preferences(&app.prefs);
            }
        }

        Action::SwitchPalette => pal.switcher.open_fresh(),
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
            let i = pal.palettes.0.len() - 1;
            pal.choice.0 = i;
            start_rename(pal, i);
            roxel::io::palettes::save(&pal.palettes.0);
        }
        Action::DuplicatePalette => {
            pal.choice.0 = active_index(pal);
            let i = crate::ui::save_as_new(&mut pal.palettes, &mut pal.choice, &mut pal.working);
            start_rename(pal, i);
            roxel::io::palettes::save(&pal.palettes.0);
        }
        Action::RenamePalette => {
            let i = active_index(pal);
            if pal.palettes.0.get(i).is_some_and(|p| !p.builtin) {
                start_rename(pal, i);
            }
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

        Action::ShowCommandPalette => app.cmd_palette.toggle(),
        Action::Preferences => app.prefs_window.open = !app.prefs_window.open,
        Action::SetTheme(t) => {
            app.prefs.theme = t;
            save_preferences(&app.prefs);
        }
        Action::ShowOnboarding => app.onboarding.start(),
        Action::Changelog => crate::updater::open_url(CHANGELOG_URL),
        Action::CheckForUpdates => crate::updater::start_check(&mut app.updater, true),
        Action::About => app.about.0 = true,
    }
}

fn start_rename(pal: &mut PaletteOpParams, i: usize) {
    pal.rename.editing = Some(i);
    pal.rename.buf = pal.palettes.0[i].name.clone();
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
            checking_updates: false,
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
        let prefs = Preferences {
            show_floor_grid: true,
            ..Default::default()
        };
        let mut s = state(&palettes, &prefs);
        assert_eq!(Action::ToggleFloorGrid.label(&s), "Hide floor grid");
        assert_eq!(Action::ToggleFlyby.label(&s), "Start flyby camera");
        s.flyby_active = true;
        assert_eq!(Action::ToggleFlyby.label(&s), "Stop flyby camera");
    }

    #[test]
    fn palette_actions_follow_the_active_palette() {
        let palettes = vec![pal("Built", true), pal("Mine", false)];
        let prefs = Preferences::default();
        let mut s = state(&palettes, &prefs);
        assert!(!Action::RenamePalette.enabled(&s));
        assert!(!Action::DeletePalette.enabled(&s));
        assert_eq!(
            Action::DuplicatePalette.menu_label(&s),
            "Save as new palette"
        );
        assert!(Action::AddColorToPalette.enabled(&s));
        s.palette_choice = 1;
        assert!(Action::RenamePalette.enabled(&s));
        assert!(Action::DeletePalette.enabled(&s));
        assert_eq!(Action::DuplicatePalette.menu_label(&s), "Duplicate");
        s.active_colors = &[[10, 20, 30, 255]];
        assert!(!Action::AddColorToPalette.enabled(&s));
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
    fn check_items_reflect_preferences() {
        let palettes = vec![pal("A", true)];
        let prefs = Preferences {
            show_origin_axes: false,
            color_space: ColorSpace::Hsl,
            ..Default::default()
        };
        let s = state(&palettes, &prefs);
        assert!(!Action::ToggleOriginAxes.checked(&s));
        assert!(Action::SetColorSpace(ColorSpace::Hsl).checked(&s));
        assert!(!Action::SetColorSpace(ColorSpace::Hex).checked(&s));
        assert!(Action::SetColorSpace(ColorSpace::Hex).is_check());
        assert!(!Action::Undo.is_check());
    }

    #[test]
    fn fill_selection_waits_for_an_idle_selection() {
        let palettes = vec![pal("A", true)];
        let prefs = Preferences::default();
        let mut s = state(&palettes, &prefs);
        s.has_selection = true;
        assert!(Action::FillSelection.enabled(&s));
        s.selection_idle = false;
        assert!(!Action::FillSelection.enabled(&s));
    }

    fn fire(s: &ActionState, mods: Mods, key: KeyCode, gate: ShortcutGate) -> Option<Action> {
        shortcut_action(mods, |k| k == key, s, gate)
    }

    const CMD: Mods = Mods {
        cmd: true,
        shift: false,
        alt: false,
    };
    const CMD_SHIFT: Mods = Mods {
        cmd: true,
        shift: true,
        alt: false,
    };
    const NONE: Mods = Mods {
        cmd: false,
        shift: false,
        alt: false,
    };

    #[test]
    fn chord_needs_exact_modifiers() {
        let undo = Chord::cmd(KeyCode::KeyZ);
        assert!(undo.matches(CMD, |k| k == KeyCode::KeyZ));
        assert!(!undo.matches(CMD_SHIFT, |k| k == KeyCode::KeyZ));
        assert!(!undo.matches(NONE, |k| k == KeyCode::KeyZ));
        let alt = Mods { alt: true, ..NONE };
        assert!(!Chord::key(KeyCode::KeyB).matches(alt, |k| k == KeyCode::KeyB));
    }

    #[test]
    fn bare_key_chords_ignore_shift_but_cmd_chords_do_not() {
        let shift = Mods {
            shift: true,
            ..NONE
        };
        assert!(Chord::key(KeyCode::KeyB).matches(shift, |k| k == KeyCode::KeyB));
        assert!(!Chord::cmd(KeyCode::KeyZ).matches(CMD_SHIFT, |k| k == KeyCode::KeyZ));
    }

    #[test]
    fn zoom_in_also_fires_on_cmd_shift_equal() {
        let palettes = vec![pal("A", true)];
        let prefs = Preferences::default();
        let s = state(&palettes, &prefs);
        assert_eq!(
            fire(&s, CMD_SHIFT, KeyCode::Equal, ShortcutGate::default()),
            Some(Action::ZoomIn)
        );
    }

    #[test]
    fn plain_keys_and_select_chords_clash_with_typing() {
        assert!(Action::FillSelection.clashes_with_typing());
        assert!(Action::SelectAll.clashes_with_typing());
        assert!(Action::ClearSelection.clashes_with_typing());
        assert!(!Action::SaveProject.clashes_with_typing());
    }

    #[test]
    fn chord_accepts_numpad_and_delete_aliases() {
        assert!(Chord::cmd(KeyCode::Digit1).matches(CMD, |k| k == KeyCode::Numpad1));
        assert!(Chord::cmd(KeyCode::Equal).matches(CMD, |k| k == KeyCode::NumpadAdd));
        assert!(Chord::key(KeyCode::Backspace).matches(NONE, |k| k == KeyCode::Delete));
    }

    #[test]
    fn shortcuts_dispatch_their_actions() {
        let palettes = vec![pal("A", true)];
        let prefs = Preferences::default();
        let mut s = state(&palettes, &prefs);
        s.has_undo = true;
        s.has_redo = true;
        let free = ShortcutGate::default();
        assert_eq!(fire(&s, CMD, KeyCode::KeyZ, free), Some(Action::Undo));
        assert_eq!(fire(&s, CMD_SHIFT, KeyCode::KeyZ, free), Some(Action::Redo));
        assert_eq!(
            fire(&s, CMD, KeyCode::Digit1, free),
            Some(Action::ViewPreset(CameraPreset::Front))
        );
        assert_eq!(
            fire(&s, CMD_SHIFT, KeyCode::Digit3, free),
            Some(Action::ViewPreset(CameraPreset::Left))
        );
        assert_eq!(
            fire(&s, NONE, KeyCode::KeyE, free),
            Some(Action::SelectTool(Tool::Erase))
        );
        assert_eq!(fire(&s, CMD, KeyCode::KeyN, free), Some(Action::NewProject));
    }

    #[test]
    fn ctrl_y_redoes_only_off_macos() {
        let palettes = vec![pal("A", true)];
        let prefs = Preferences::default();
        let mut s = state(&palettes, &prefs);
        s.has_redo = true;
        let expected = (!cfg!(target_os = "macos")).then_some(Action::Redo);
        assert_eq!(
            fire(&s, CMD, KeyCode::KeyY, ShortcutGate::default()),
            expected
        );
    }

    #[test]
    fn s_switches_to_shape_then_cycles_shapes() {
        let palettes = vec![pal("A", true)];
        let prefs = Preferences::default();
        let mut s = state(&palettes, &prefs);
        let free = ShortcutGate::default();
        assert_eq!(
            fire(&s, NONE, KeyCode::KeyS, free),
            Some(Action::SelectTool(Tool::Shape))
        );
        s.tool = Tool::Shape;
        assert_eq!(
            fire(&s, NONE, KeyCode::KeyS, free),
            Some(Action::CycleShape)
        );
    }

    #[test]
    fn no_two_actions_fire_on_the_same_chord() {
        let palettes = vec![pal("A", true)];
        let prefs = Preferences::default();
        for tool in [Tool::Brush, Tool::Shape] {
            let mut s = state(&palettes, &prefs);
            s.tool = tool;
            s.has_undo = true;
            s.has_redo = true;
            s.has_selection = true;
            s.has_clipboard = true;
            s.has_voxels = true;
            let live: Vec<(Action, Chord)> = Action::STATIC
                .iter()
                .filter(|a| a.enabled(&s))
                .flat_map(|&a| a.chords().map(move |c| (a, c)))
                .collect();
            for (i, (a, c)) in live.iter().enumerate() {
                for (b, d) in &live[..i] {
                    assert!(c != d, "{a:?} and {b:?} both on {c:?}");
                }
            }
        }
    }

    #[test]
    fn typing_and_modals_block_shortcuts() {
        let palettes = vec![pal("A", true)];
        let prefs = Preferences::default();
        let mut s = state(&palettes, &prefs);
        s.has_undo = true;
        let typing = ShortcutGate {
            typing: true,
            ..Default::default()
        };
        assert_eq!(fire(&s, CMD, KeyCode::KeyZ, typing), None);
        assert_eq!(fire(&s, NONE, KeyCode::KeyB, typing), None);
        let modal = ShortcutGate {
            modal: true,
            ..Default::default()
        };
        assert_eq!(fire(&s, CMD, KeyCode::KeyZ, modal), None);
        assert_eq!(fire(&s, CMD, KeyCode::KeyK, modal), None);
    }

    #[test]
    fn open_palette_and_prefs_close_with_their_own_shortcut() {
        let palettes = vec![pal("A", true)];
        let prefs = Preferences::default();
        let s = state(&palettes, &prefs);
        let palette = ShortcutGate {
            typing: true,
            modal: true,
            palette_open: true,
            prefs_open: false,
        };
        assert_eq!(
            fire(&s, CMD, KeyCode::KeyK, palette),
            Some(Action::ShowCommandPalette)
        );
        let prefs_gate = ShortcutGate {
            typing: false,
            modal: true,
            palette_open: false,
            prefs_open: true,
        };
        assert_eq!(
            fire(&s, CMD, KeyCode::Comma, prefs_gate),
            Some(Action::Preferences)
        );
        assert_eq!(fire(&s, CMD, KeyCode::KeyK, prefs_gate), None);
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
