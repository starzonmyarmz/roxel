use crate::actions::{Action, ActionState, Category, Chord, KeyToken, Platform};
use crate::theme::Theme;
use crate::ui::icons;
use crate::ui::search_popover::{self, SearchNav, SearchPopover};
use crate::ui::tokens::{font, icon, radius, size, space};
use bevy::prelude::*;
use bevy_egui::egui;

/// Resource backing the Cmd+K command palette. Holds open/close state, the
/// search query, the current selection index in the filtered list, and a
/// one-shot `just_opened` flag that drives auto-focus on the search input.
#[derive(Resource, Default)]
pub struct CommandPalette {
    pub open: bool,
    pub search: String,
    pub selected: usize,
    pub just_opened: bool,
}

impl CommandPalette {
    pub fn toggle(&mut self) {
        if self.open {
            self.open = false;
        } else {
            self.open_fresh();
        }
    }

    fn open_fresh(&mut self) {
        self.open = true;
        self.search.clear();
        self.selected = 0;
        self.just_opened = true;
    }
}

#[derive(Clone)]
pub struct CatalogEntry {
    pub label: String,
    pub category: Category,
    pub keywords: &'static str,
    pub shortcut: Option<Chord>,
    pub enabled: bool,
    pub action: Action,
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
        fuzzy_match(entry.keywords, query).map(|s| s - 4) // small penalty vs label hits
    };
    match (label_score, kw_score) {
        (Some(a), Some(b)) => Some(a.max(b)),
        (Some(a), None) => Some(a),
        (None, Some(b)) => Some(b),
        (None, None) => None,
    }
}

// -------- Catalog builder --------

pub fn build_catalog(state: &ActionState) -> Vec<CatalogEntry> {
    Action::all(state)
        .into_iter()
        .filter(|&a| a != Action::ShowCommandPalette)
        .map(|action| {
            let spec = action.spec();
            CatalogEntry {
                label: action.label(state).into_owned(),
                category: spec.category,
                keywords: action.keywords(state),
                shortcut: spec.shortcut,
                enabled: action.enabled(state),
                action,
            }
        })
        .collect()
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

// -------- Renderer (called from ui_system) --------

/// Render the centered command-palette modal and handle keyboard navigation
/// inside it. Returns the command the user ran, if any.
pub fn draw(
    ctx: &egui::Context,
    theme: &Theme,
    palette: &mut CommandPalette,
    catalog: &[CatalogEntry],
) -> Option<Action> {
    if !palette.open {
        return None;
    }
    // Taken before the search field reads them so the caret stays put.
    let nav = SearchNav::consume(ctx);

    let prev_search = palette.search.clone();
    let mut close_after = nav.esc;
    let mut pending: Option<Action> = None;

    let order = rank(catalog, &palette.search);
    // Clamp selection inside the filtered list.
    if order.is_empty() {
        palette.selected = 0;
    } else if palette.selected >= order.len() {
        palette.selected = order.len() - 1;
    }

    // Apply navigation (skip disabled entries).
    if !order.is_empty() {
        if nav.down {
            palette.selected = next_enabled(catalog, &order, palette.selected, 1);
        }
        if nav.up {
            palette.selected = next_enabled(catalog, &order, palette.selected, -1);
        }
    }
    if nav.enter
        && let Some(&idx) = order.get(palette.selected)
        && catalog[idx].enabled
    {
        pending = Some(catalog[idx].action);
        close_after = true;
    }

    let popover = SearchPopover {
        id: "Command palette",
        hint: "Type a command…",
        empty: "No commands match.",
        enter_label: "Run",
    };
    let clicked_outside = search_popover::show(
        ctx,
        theme,
        &popover,
        &mut palette.search,
        &mut palette.just_opened,
        !order.is_empty(),
        |ui| {
            for (visual_idx, &idx) in order.iter().enumerate() {
                let entry = &catalog[idx];
                let selected = visual_idx == palette.selected;
                if draw_row(ui, theme, entry, selected) && entry.enabled {
                    pending = Some(entry.action);
                    close_after = true;
                }
            }
        },
    );
    close_after |= clicked_outside;

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
    if close_after {
        palette.open = false;
    }
    pending
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
    let chip_color = theme.text;

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

    // Shortcut (right) — per-key chips (text or Lucide icons), surface fill.
    if let Some(sc) = entry.shortcut {
        let chip_h = icon::XS + space::SM; // 20, matches footer chip
        let tokens: Vec<ShortcutToken> = sc
            .tokens(Platform::CURRENT)
            .into_iter()
            .map(ShortcutToken::from)
            .collect();
        let gap = space::XXS;
        let widths: Vec<f32> = tokens
            .iter()
            .map(|t| match t {
                ShortcutToken::Text(s) => {
                    let g = ui.painter().layout_no_wrap(
                        s.clone(),
                        egui::FontId::monospace(font::SMALL),
                        chip_color,
                    );
                    g.size().x + space::SX * 2.0
                }
                ShortcutToken::Command | ShortcutToken::Shift => icon::XS + space::SX * 2.0,
            })
            .collect();
        let total: f32 = widths.iter().sum::<f32>() + gap * (tokens.len().saturating_sub(1)) as f32;
        let mut x = rect.right() - 10.0 - total;
        let y = rect.center().y - chip_h * 0.5;
        for tok in &tokens {
            let w = match tok {
                ShortcutToken::Text(s) => {
                    let g = ui.painter().layout_no_wrap(
                        s.clone(),
                        egui::FontId::monospace(font::SMALL),
                        chip_color,
                    );
                    let tw = g.size().x + space::SX * 2.0;
                    let chip_rect =
                        egui::Rect::from_min_size(egui::pos2(x, y), egui::vec2(tw, chip_h));
                    ui.painter().rect_filled(
                        chip_rect,
                        egui::CornerRadius::same(radius::XS),
                        theme.surface,
                    );
                    let pos = egui::pos2(
                        chip_rect.center().x - g.size().x * 0.5,
                        chip_rect.center().y - g.size().y * 0.5,
                    );
                    ui.painter().galley(pos, g, chip_color);
                    tw
                }
                ShortcutToken::Command | ShortcutToken::Shift => {
                    let tw = icon::XS + space::SX * 2.0;
                    let chip_rect =
                        egui::Rect::from_min_size(egui::pos2(x, y), egui::vec2(tw, chip_h));
                    ui.painter().rect_filled(
                        chip_rect,
                        egui::CornerRadius::same(radius::XS),
                        theme.surface,
                    );
                    let img_rect = egui::Rect::from_center_size(
                        chip_rect.center(),
                        egui::vec2(icon::XS, icon::XS),
                    );
                    if let Some(src) = tok.icon_source() {
                        egui::Image::new(src)
                            .tint(chip_color)
                            .paint_at(ui, img_rect);
                    }
                    tw
                }
            };
            x += w + gap;
        }
    }

    resp.clicked() && entry.enabled
}

/// A single chip in a shortcut display — either text or a Lucide icon for a
/// modifier key.
#[derive(Debug, PartialEq)]
enum ShortcutToken {
    Text(String),
    Command,
    Shift,
}

impl From<KeyToken> for ShortcutToken {
    fn from(t: KeyToken) -> Self {
        match t {
            KeyToken::Command => ShortcutToken::Command,
            KeyToken::Shift => ShortcutToken::Shift,
            KeyToken::Control => ShortcutToken::Text("Ctrl".into()),
            KeyToken::Key(k) => ShortcutToken::Text(k.into_owned()),
        }
    }
}

impl ShortcutToken {
    fn icon_source(&self) -> Option<egui::ImageSource<'static>> {
        match self {
            ShortcutToken::Command => Some(icons::command()),
            ShortcutToken::Shift => Some(icons::arrow_big_up()),
            _ => None,
        }
    }
}

// -------- Tests --------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::Preferences;
    use crate::tools::Tool;
    use crate::ui::palette::Palette;
    use roxel::shapes::ShapePrimitive;

    fn state<'a>(palettes: &'a [Palette], prefs: &'a Preferences) -> ActionState<'a> {
        ActionState {
            tool: Tool::Brush,
            shape: ShapePrimitive::Rectangle,
            has_undo: false,
            has_redo: false,
            stroke_open: false,
            has_selection: false,
            selection_idle: true,
            has_clipboard: false,
            has_voxels: false,
            dialog_busy: false,
            flyby_active: false,
            focus_mode: false,
            checking_updates: false,
            update_tag: None,
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
            colors: vec![[1, 2, 3, 255], [4, 5, 6, 255]],
            builtin,
        }
    }

    #[test]
    fn catalog_lists_app_level_commands() {
        let palettes = vec![pal("Built", true), pal("Mine", false)];
        let prefs = Preferences::default();
        let recent = vec![std::path::PathBuf::from("/tmp/castle.rox")];
        let mut s = state(&palettes, &prefs);
        s.recent = &recent;
        s.palette_choice = 1;
        let catalog = build_catalog(&s);
        let listed = |a: Action| catalog.iter().any(|e| e.action == a && e.enabled);
        for action in [
            Action::CheckForUpdates,
            Action::ShowOnboarding,
            Action::SetColorSpace(roxel::color_space::ColorSpace::Oklch),
            Action::OpenRecent(0),
            Action::RenamePalette,
            Action::ToggleFocusMode,
            Action::ShowShortcuts,
        ] {
            assert!(listed(action), "{action:?} missing or disabled");
        }
        assert!(catalog.iter().any(|e| e.label == "Open recent: castle.rox"));
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
        let prefs = Preferences::default();
        let cat = build_catalog(&state(&palettes, &prefs));
        let undo = cat
            .iter()
            .find(|e| e.action == Action::Undo)
            .expect("undo entry");
        assert!(!undo.enabled);
    }

    #[test]
    fn catalog_lists_other_palettes_as_switch_entries() {
        let palettes = vec![pal("Alpha", true), pal("Beta", false), pal("Gamma", false)];
        let prefs = Preferences::default();
        let cat = build_catalog(&state(&palettes, &prefs));
        let switches: Vec<_> = cat
            .iter()
            .filter(|e| matches!(e.action, Action::SelectPalette(_)))
            .collect();
        // Two other palettes (active palette is skipped).
        assert_eq!(switches.len(), 2);
        assert!(switches.iter().any(|e| e.label.contains("Beta")));
        assert!(switches.iter().any(|e| e.label.contains("Gamma")));
        assert!(switches.iter().all(|e| !e.label.contains("Alpha")));
    }

    #[test]
    fn catalog_omits_the_command_palette_itself() {
        let palettes = vec![pal("p1", true)];
        let prefs = Preferences::default();
        let cat = build_catalog(&state(&palettes, &prefs));
        assert!(cat.iter().all(|e| e.action != Action::ShowCommandPalette));
    }

    #[test]
    fn catalog_lists_recent_files() {
        let palettes = vec![pal("p1", true)];
        let prefs = Preferences::default();
        let recent = vec![std::path::PathBuf::from("/tmp/castle.rox")];
        let mut s = state(&palettes, &prefs);
        s.recent = &recent;
        let cat = build_catalog(&s);
        let entry = cat
            .iter()
            .find(|e| e.action == Action::OpenRecent(0))
            .expect("recent entry");
        assert_eq!(entry.label, "Open recent: castle.rox");
        assert!(entry.enabled);
    }

    #[test]
    fn rank_orders_better_matches_first() {
        let palettes = vec![pal("p1", true)];
        let prefs = Preferences::default();
        let cat = build_catalog(&state(&palettes, &prefs));
        let order = rank(&cat, "frame");
        let first = &cat[order[0]];
        assert!(first.label.starts_with("Frame view"), "got {}", first.label);
    }

    #[test]
    fn rank_keeps_disabled_in_results_but_below_enabled() {
        let palettes = vec![pal("p1", true)];
        let prefs = Preferences::default();
        let cat = build_catalog(&state(&palettes, &prefs));
        let order = rank(&cat, "undo");
        let undo_pos = order
            .iter()
            .position(|&i| cat[i].action == Action::Undo)
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
    fn shortcut_tokens_map_to_chips() {
        assert_eq!(
            ShortcutToken::from(KeyToken::Control),
            ShortcutToken::Text("Ctrl".into())
        );
        assert_eq!(
            ShortcutToken::from(KeyToken::Key("N".into())),
            ShortcutToken::Text("N".into())
        );
        assert_eq!(ShortcutToken::from(KeyToken::Shift), ShortcutToken::Shift);
        assert_eq!(
            ShortcutToken::from(KeyToken::Command),
            ShortcutToken::Command
        );
    }
}
