use crate::io;
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;

#[derive(Clone)]
pub struct Palette {
    pub name: String,
    pub colors: Vec<[u8; 4]>,
    /// True for code-defined palettes (read-only in UI). User-created and
    /// imported palettes are `false` and persist via `io::palettes`.
    pub builtin: bool,
}

macro_rules! hex_palette {
    ($name:expr, $($r:literal $g:literal $b:literal),* $(,)?) => {
        Palette {
            name: String::from($name),
            colors: vec![$([$r, $g, $b, 255u8]),*],
            builtin: true,
        }
    };
}

#[derive(Resource)]
pub struct Palettes(pub Vec<Palette>);

impl Palettes {
    /// Built-ins followed by user palettes loaded from disk.
    pub fn with_user_loaded() -> Self {
        let mut me = Self::default();
        me.0.extend(io::palettes::load());
        me
    }
}

impl Default for Palettes {
    fn default() -> Self {
        Self(vec![
            hex_palette!(
                "Sweetie 16",
                0x1A 0x1C 0x2C, 0x5D 0x27 0x5D, 0xB1 0x3E 0x53, 0xEF 0x7D 0x57,
                0xFF 0xCD 0x75, 0xA7 0xF0 0x70, 0x38 0xB7 0x64, 0x25 0x71 0x79,
                0x29 0x36 0x6F, 0x3B 0x5D 0xC9, 0x41 0xA6 0xF6, 0x73 0xEF 0xF7,
                0xF4 0xF4 0xF4, 0x94 0xB0 0xC2, 0x56 0x6C 0x86, 0x33 0x3C 0x57,
            ),
            hex_palette!(
                "PICO-8",
                0x00 0x00 0x00, 0x1D 0x2B 0x53, 0x7E 0x25 0x53, 0x00 0x87 0x51,
                0xAB 0x52 0x36, 0x5F 0x57 0x4F, 0xC2 0xC3 0xC7, 0xFF 0xF1 0xE8,
                0xFF 0x00 0x4D, 0xFF 0xA3 0x00, 0xFF 0xEC 0x27, 0x00 0xE4 0x36,
                0x29 0xAD 0xFF, 0x83 0x76 0x9C, 0xFF 0x77 0xA8, 0xFF 0xCC 0xAA,
            ),
            hex_palette!(
                "DawnBringer 16",
                0x14 0x0C 0x1C, 0x44 0x24 0x34, 0x30 0x34 0x6D, 0x4E 0x4A 0x4E,
                0x85 0x4C 0x30, 0x34 0x65 0x24, 0xD0 0x46 0x48, 0x75 0x71 0x61,
                0x59 0x7D 0xCE, 0xD2 0x7D 0x2C, 0x85 0x95 0xA1, 0x6D 0xAA 0x2C,
                0xD2 0xAA 0x99, 0x6D 0xC2 0xCA, 0xDA 0xD4 0x5E, 0xDE 0xEE 0xD6,
            ),
            hex_palette!(
                "DawnBringer 32",
                0x00 0x00 0x00, 0x22 0x20 0x34, 0x45 0x28 0x3C, 0x66 0x39 0x31,
                0x8F 0x56 0x3B, 0xDF 0x71 0x26, 0xD9 0xA0 0x66, 0xEE 0xC3 0x9A,
                0xFB 0xF2 0x36, 0x99 0xE5 0x50, 0x6A 0xBE 0x30, 0x37 0x94 0x6E,
                0x4B 0x69 0x2F, 0x52 0x4B 0x24, 0x32 0x3C 0x39, 0x3F 0x3F 0x74,
                0x30 0x60 0x82, 0x5B 0x6E 0xE1, 0x63 0x9B 0xFF, 0x5F 0xCD 0xE4,
                0xCB 0xDB 0xFC, 0xFF 0xFF 0xFF, 0x9B 0xAD 0xB7, 0x84 0x7E 0x87,
                0x69 0x6A 0x6A, 0x59 0x56 0x52, 0x76 0x42 0x8A, 0xAC 0x32 0x32,
                0xD9 0x57 0x63, 0xD7 0x7B 0xBA, 0x8F 0x97 0x4A, 0x8A 0x6F 0x30,
            ),
            hex_palette!(
                "Endesga 32",
                0xBE 0x4A 0x2F, 0xD7 0x76 0x43, 0xEA 0xD4 0xAA, 0xE4 0xA6 0x72,
                0xB8 0x6F 0x50, 0x73 0x3E 0x39, 0x3E 0x27 0x31, 0xA2 0x26 0x33,
                0xE4 0x3B 0x44, 0xF7 0x76 0x22, 0xFE 0xAE 0x34, 0xFE 0xE7 0x61,
                0x63 0xC7 0x4D, 0x3E 0x89 0x48, 0x26 0x5C 0x42, 0x19 0x3C 0x3E,
                0x12 0x4E 0x89, 0x00 0x99 0xDB, 0x2C 0xE8 0xF5, 0xFF 0xFF 0xFF,
                0xC0 0xCB 0xDC, 0x8B 0x9B 0xB4, 0x5A 0x69 0x88, 0x3A 0x44 0x66,
                0x26 0x2B 0x44, 0x18 0x14 0x25, 0xFF 0x00 0x44, 0x68 0x38 0x6C,
                0xB5 0x50 0x88, 0xF6 0x75 0x7A, 0xE8 0xB7 0x96, 0xC2 0x85 0x69,
            ),
            hex_palette!(
                "NA16",
                0x8C 0x8F 0xAE, 0x58 0x45 0x63, 0x3E 0x21 0x37, 0x9A 0x63 0x48,
                0xD7 0x9B 0x7D, 0xF5 0xED 0xBA, 0xC0 0xC7 0x41, 0x64 0x7D 0x34,
                0xE4 0x94 0x3A, 0x9D 0x30 0x3B, 0xD2 0x64 0x71, 0x70 0x37 0x7F,
                0x7E 0xC4 0xC1, 0x34 0x85 0x9D, 0x17 0x43 0x4B, 0x1F 0x0E 0x1C,
            ),
            hex_palette!(
                "Basic",
                0x00 0x00 0x00, 0x80 0x80 0x80, 0xFF 0xFF 0xFF, 0xFF 0x00 0x00,
                0x00 0xFF 0x00, 0x00 0x00 0xFF, 0xFF 0xFF 0x00, 0xFF 0x00 0xFF,
                0x00 0xFF 0xFF, 0xFF 0x80 0x00, 0x80 0x00 0xFF, 0x00 0x80 0x40,
            ),
        ])
    }
}

#[derive(Resource, Default)]
pub struct PaletteChoice(pub usize);

/// In-session scratch edits to a built-in palette. Built-ins are never
/// persisted; the first edit copies the built-in's colors here and marks the
/// buffer dirty. Switching away (guarded by [`DiscardConfirm`]) or saving as a
/// new palette clears it, so reloading the built-in yields the pristine set.
#[derive(Resource, Default)]
pub struct WorkingPalette {
    /// Index into [`Palettes`] of the built-in being edited, if any.
    pub source: Option<usize>,
    pub colors: Vec<[u8; 4]>,
    pub dirty: bool,
}

impl WorkingPalette {
    pub fn clear(&mut self) {
        self.source = None;
        self.colors.clear();
        self.dirty = false;
    }

    /// True when `idx` is the built-in whose scratch edits are held here.
    pub fn editing(&self, idx: usize) -> bool {
        self.source == Some(idx)
    }

    /// True when there are unsaved scratch edits to `idx`.
    pub fn is_dirty_for(&self, idx: usize) -> bool {
        self.dirty && self.source == Some(idx)
    }
}

/// Staged discard confirmation. Switching away from a dirty built-in stores the
/// target palette index here so the UI can confirm before throwing edits away.
#[derive(Resource, Default)]
pub struct DiscardConfirm {
    pub pending: Option<usize>,
}

/// State for the command-palette-style palette switcher popover (opened from the
/// inspector's `…` menu). Mirrors `CommandPalette`: open flag, search query,
/// selected row, and a one-shot focus flag. Drawn by `palette_switcher::draw`.
#[derive(Resource, Default)]
pub struct PaletteSwitcher {
    pub open: bool,
    pub search: String,
    pub selected: usize,
    pub just_opened: bool,
}

impl PaletteSwitcher {
    pub fn open_fresh(&mut self) {
        self.open = true;
        self.search.clear();
        self.selected = 0;
        self.just_opened = true;
    }
}

/// Colors to display for palette `idx`: the scratch buffer when a built-in is
/// being edited, otherwise the palette's own colors.
pub fn display_colors<'a>(
    palettes: &'a Palettes,
    working: &'a WorkingPalette,
    idx: usize,
) -> &'a [[u8; 4]] {
    if working.editing(idx) {
        &working.colors
    } else {
        &palettes.0[idx].colors
    }
}

/// Mutable colors for editing palette `idx`. For a built-in, lazily seeds the
/// scratch buffer from the built-in and marks it dirty. Returns `true` when the
/// edit should be persisted to disk (user palettes only — built-ins live in the
/// scratch buffer until saved as a new palette).
pub fn edit_colors<'a>(
    palettes: &'a mut Palettes,
    working: &'a mut WorkingPalette,
    idx: usize,
) -> (&'a mut Vec<[u8; 4]>, bool) {
    if palettes.0[idx].builtin {
        if !working.editing(idx) {
            working.colors = palettes.0[idx].colors.clone();
            working.source = Some(idx);
        }
        working.dirty = true;
        (&mut working.colors, false)
    } else {
        (&mut palettes.0[idx].colors, true)
    }
}

/// Switch the active palette to `target`, or stage a discard confirmation when
/// the current palette is a built-in with unsaved scratch edits.
pub fn request_select(
    target: usize,
    choice: &mut PaletteChoice,
    working: &mut WorkingPalette,
    discard: &mut DiscardConfirm,
) {
    if target == choice.0 {
        return;
    }
    if working.is_dirty_for(choice.0) {
        discard.pending = Some(target);
    } else {
        working.clear();
        choice.0 = target;
    }
}

/// Fork the active palette's current colors (including any scratch edits) into a
/// new user palette named `"<name> copy"`, select it, and clear the scratch
/// buffer. Returns the new index. Caller persists via `io::palettes::save`.
pub fn save_as_new(
    palettes: &mut Palettes,
    choice: &mut PaletteChoice,
    working: &mut WorkingPalette,
) -> usize {
    let idx = choice.0;
    let colors = if working.editing(idx) {
        working.colors.clone()
    } else {
        palettes.0[idx].colors.clone()
    };
    let base = format!("{} copy", palettes.0[idx].name);
    let name = unique_palette_name(&palettes.0, &base);
    palettes.0.push(Palette {
        name,
        colors,
        builtin: false,
    });
    let new_idx = palettes.0.len() - 1;
    choice.0 = new_idx;
    working.clear();
    new_idx
}

/// Transient UI state for inline rename of the active palette.
#[derive(Default)]
pub struct PaletteRenameState {
    pub editing: Option<usize>,
    pub buf: String,
}

#[derive(SystemParam)]
pub struct PaletteParams<'w, 's> {
    pub palettes: ResMut<'w, Palettes>,
    pub choice: ResMut<'w, PaletteChoice>,
    pub rename: Local<'s, PaletteRenameState>,
    pub working: ResMut<'w, WorkingPalette>,
    pub discard: ResMut<'w, DiscardConfirm>,
    pub switcher: ResMut<'w, PaletteSwitcher>,
}

pub fn unique_palette_name(palettes: &[Palette], base: &str) -> String {
    if !palettes.iter().any(|p| p.name == base) {
        return base.to_string();
    }
    for n in 2.. {
        let candidate = format!("{base} {n}");
        if !palettes.iter().any(|p| p.name == candidate) {
            return candidate;
        }
    }
    unreachable!()
}

pub fn next_palette_name(palettes: &[Palette]) -> String {
    unique_palette_name(palettes, "Untitled")
}

pub fn sanitize_filename(s: &str) -> String {
    let cleaned: String = s
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if cleaned.is_empty() {
        "palette".into()
    } else {
        cleaned
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(name: &str) -> Palette {
        Palette {
            name: name.into(),
            colors: Vec::new(),
            builtin: false,
        }
    }

    #[test]
    fn unique_name_returns_base_when_unused() {
        let palettes = vec![p("Foo")];
        assert_eq!(unique_palette_name(&palettes, "Bar"), "Bar");
    }

    #[test]
    fn unique_name_appends_counter_when_taken() {
        let palettes = vec![p("Foo"), p("Foo 2")];
        assert_eq!(unique_palette_name(&palettes, "Foo"), "Foo 3");
    }

    #[test]
    fn next_palette_name_is_untitled() {
        assert_eq!(next_palette_name(&[]), "Untitled");
        assert_eq!(next_palette_name(&[p("Untitled")]), "Untitled 2");
    }

    fn builtin(name: &str, colors: Vec<[u8; 4]>) -> Palette {
        Palette {
            name: name.into(),
            colors,
            builtin: true,
        }
    }

    #[test]
    fn edit_builtin_seeds_scratch_and_marks_dirty() {
        let mut palettes = Palettes(vec![builtin("Sweetie", vec![[1, 1, 1, 255]])]);
        let mut working = WorkingPalette::default();
        let (colors, persist) = edit_colors(&mut palettes, &mut working, 0);
        colors.push([9, 9, 9, 255]);
        assert!(!persist, "built-in edits are not persisted");
        assert!(working.is_dirty_for(0));
        assert_eq!(working.colors, vec![[1, 1, 1, 255], [9, 9, 9, 255]]);
        // The built-in itself is untouched.
        assert_eq!(palettes.0[0].colors, vec![[1, 1, 1, 255]]);
    }

    #[test]
    fn edit_user_palette_mutates_in_place_and_persists() {
        let mut palettes = Palettes(vec![p("Mine")]);
        let mut working = WorkingPalette::default();
        let (colors, persist) = edit_colors(&mut palettes, &mut working, 0);
        colors.push([2, 2, 2, 255]);
        assert!(persist);
        assert!(!working.dirty);
        assert_eq!(palettes.0[0].colors, vec![[2, 2, 2, 255]]);
    }

    #[test]
    fn display_colors_prefers_scratch_when_editing() {
        let mut palettes = Palettes(vec![builtin("B", vec![[1, 1, 1, 255]])]);
        let mut working = WorkingPalette::default();
        assert_eq!(display_colors(&palettes, &working, 0), &[[1, 1, 1, 255]]);
        edit_colors(&mut palettes, &mut working, 0)
            .0
            .push([2, 2, 2, 255]);
        assert_eq!(
            display_colors(&palettes, &working, 0),
            &[[1, 1, 1, 255], [2, 2, 2, 255]]
        );
    }

    #[test]
    fn request_select_switches_when_clean() {
        let mut choice = PaletteChoice(0);
        let mut working = WorkingPalette::default();
        let mut discard = DiscardConfirm::default();
        request_select(2, &mut choice, &mut working, &mut discard);
        assert_eq!(choice.0, 2);
        assert_eq!(discard.pending, None);
    }

    #[test]
    fn request_select_stages_discard_when_dirty_builtin() {
        let mut choice = PaletteChoice(0);
        let mut working = WorkingPalette {
            source: Some(0),
            colors: vec![[1, 1, 1, 255]],
            dirty: true,
        };
        let mut discard = DiscardConfirm::default();
        request_select(3, &mut choice, &mut working, &mut discard);
        assert_eq!(choice.0, 0, "stays put until confirmed");
        assert_eq!(discard.pending, Some(3));
    }

    #[test]
    fn save_as_new_forks_scratch_into_user_palette() {
        let mut palettes = Palettes(vec![builtin("Sweetie", vec![[1, 1, 1, 255]])]);
        let mut choice = PaletteChoice(0);
        let mut working = WorkingPalette::default();
        edit_colors(&mut palettes, &mut working, 0)
            .0
            .push([7, 7, 7, 255]);
        let new_idx = save_as_new(&mut palettes, &mut choice, &mut working);
        assert_eq!(new_idx, 1);
        assert_eq!(choice.0, 1);
        let forked = &palettes.0[1];
        assert_eq!(forked.name, "Sweetie copy");
        assert!(!forked.builtin);
        assert_eq!(forked.colors, vec![[1, 1, 1, 255], [7, 7, 7, 255]]);
        assert!(!working.dirty, "scratch cleared after save");
        assert_eq!(working.source, None);
    }
}
