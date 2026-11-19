//! Command-palette-style popover for switching the active palette. Opened from
//! the inspector's `…` menu (`PaletteSwitcher::open_fresh`). Visually mirrors the
//! Cmd+K command palette: centred-top window, surface-framed search, hidden
//! scrollbar, footer hint. Lists user palettes first, built-in templates under a
//! "Built-in" group, with a swatch preview per row.
//!
//! `draw` returns the chosen global palette index (if any); the caller routes it
//! through `palette::request_select` so a dirty built-in still confirms first.

use super::command_palette::fuzzy_match;
use super::palette::{Palette, PaletteSwitcher};
use super::search_popover::{self, SearchNav, SearchPopover};
use super::tokens::{font, radius, size, space, swatch};
use crate::theme::Theme;
use bevy_egui::egui;

/// A row in the rendered list: either a non-selectable group header or a
/// selectable palette at the given global index.
enum Item {
    Header(&'static str),
    Palette(usize),
}

pub fn draw(
    ctx: &egui::Context,
    theme: &Theme,
    switcher: &mut PaletteSwitcher,
    palettes: &[Palette],
) -> Option<usize> {
    if !switcher.open {
        return None;
    }

    // The same click that opened the switcher (on the inspector's `…` menu item)
    // lands outside the switcher window this frame — don't let the click-outside
    // guard below read it as a dismissal. `just_opened` is reset during the
    // window draw, so capture it first.
    let opened_this_frame = switcher.just_opened;

    // Taken before the search field reads them so the caret stays put.
    let nav = SearchNav::consume(ctx);

    // Build the visible item list (headers + filtered palettes). `selectable`
    // holds just the palette global indices, in display order, for nav.
    let (user, builtin) = filter(palettes, &switcher.search);

    let mut items: Vec<Item> = Vec::new();
    let mut selectable: Vec<usize> = Vec::new();
    if !user.is_empty() {
        items.push(Item::Header("Your palettes"));
        for &i in &user {
            items.push(Item::Palette(i));
            selectable.push(i);
        }
    }
    if !builtin.is_empty() {
        items.push(Item::Header("Built-in"));
        for &i in &builtin {
            items.push(Item::Palette(i));
            selectable.push(i);
        }
    }

    if selectable.is_empty() {
        switcher.selected = 0;
    } else if switcher.selected >= selectable.len() {
        switcher.selected = selectable.len() - 1;
    }
    if !selectable.is_empty() {
        let n = selectable.len();
        if nav.down {
            switcher.selected = (switcher.selected + 1) % n;
        }
        if nav.up {
            switcher.selected = (switcher.selected + n - 1) % n;
        }
    }

    let prev_search = switcher.search.clone();
    let mut chosen: Option<usize> = None;
    let mut close_after = nav.esc;
    if nav.enter {
        chosen = selectable.get(switcher.selected).copied();
    }

    let popover = SearchPopover {
        id: "Palette switcher",
        hint: "Search palettes…",
        empty: "No palettes match.",
        enter_label: "Switch",
    };
    let clicked_outside = search_popover::show(
        ctx,
        theme,
        &popover,
        &mut switcher.search,
        &mut switcher.just_opened,
        !selectable.is_empty(),
        |ui| {
            let mut sel_cursor = 0usize;
            for (idx, item) in items.iter().enumerate() {
                match item {
                    Item::Header(label) => {
                        // Breathing room above each group (except the first,
                        // already spaced off the search divider) so the groups
                        // read as distinct bands rather than one run-on list.
                        if idx != 0 {
                            ui.add_space(space::SM);
                        }
                        header_row(ui, theme, label);
                    }
                    Item::Palette(gi) => {
                        let selected = sel_cursor == switcher.selected;
                        sel_cursor += 1;
                        if palette_row(ui, theme, &palettes[*gi], selected) {
                            chosen = Some(*gi);
                        }
                    }
                }
            }
        },
    );
    // The opening click (on the inspector's `…` menu) is still registered this
    // frame and lands outside the popover; don't read it as a dismissal.
    if clicked_outside && !opened_this_frame {
        close_after = true;
    }

    if switcher.search != prev_search {
        switcher.selected = 0;
    }
    if chosen.is_some() || close_after {
        switcher.open = false;
    }
    chosen
}

fn header_row(ui: &mut egui::Ui, theme: &Theme, label: &str) {
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), size::CMD_PALETTE_HEADER_ROW),
        egui::Sense::hover(),
    );
    ui.painter().text(
        rect.left_center() + egui::vec2(space::FIELD_PAD_X, 0.0),
        egui::Align2::LEFT_CENTER,
        label,
        egui::FontId::new(
            font::BODY,
            egui::FontFamily::Name(crate::theme::INTER_SEMIBOLD_FAMILY.into()),
        ),
        theme.text,
    );
}

fn palette_row(ui: &mut egui::Ui, theme: &Theme, pal: &Palette, selected: bool) -> bool {
    let row_h = size::CMD_PALETTE_ROW;
    let full = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(full, row_h), egui::Sense::click());
    let bg = if selected {
        theme.surface_hover
    } else if resp.hovered() {
        theme.surface
    } else {
        egui::Color32::TRANSPARENT
    };
    ui.painter()
        .rect_filled(rect, egui::CornerRadius::same(radius::SM), bg);

    // Name (left).
    let name_galley = ui.painter().layout_no_wrap(
        pal.name.clone(),
        egui::FontId::proportional(font::BODY),
        theme.text,
    );
    ui.painter().galley(
        egui::pos2(
            rect.left() + space::FIELD_PAD_X,
            rect.center().y - name_galley.size().y * 0.5,
        ),
        name_galley,
        theme.text,
    );

    // Swatch preview (right). Square cells (12×12); reserve the right
    // ~60% of the row and show as many as land in it.
    let sw = swatch::PREVIEW_SM.x;
    let sh = swatch::PREVIEW_SM.y;
    let step = sw + space::XXS;
    let max_count = ((full * 0.6) / step).floor().max(0.0) as usize;
    let count = pal.colors.len().min(max_count);
    let mut x = rect.right() - space::FIELD_PAD_X - count as f32 * step + space::XXS;
    let y = rect.center().y - sh * 0.5;
    for c in pal.colors.iter().take(count) {
        let col = egui::Color32::from_rgba_unmultiplied(c[0], c[1], c[2], 255);
        let r = egui::Rect::from_min_size(egui::pos2(x, y), egui::vec2(sw, sh));
        ui.painter()
            .rect_filled(r, egui::CornerRadius::same(radius::XXS), col);
        x += step;
    }

    resp.clicked()
}

/// Split palettes into `(user, builtin)` global-index lists, each filtered by
/// `query` via the command-palette fuzzy matcher and kept in source order. The
/// user group renders first, built-ins under their own header.
fn filter(palettes: &[Palette], query: &str) -> (Vec<usize>, Vec<usize>) {
    let matches = |p: &Palette| fuzzy_match(&p.name, query).is_some();
    let pick = |want_builtin: bool| -> Vec<usize> {
        palettes
            .iter()
            .enumerate()
            .filter(|(_, p)| p.builtin == want_builtin && matches(p))
            .map(|(i, _)| i)
            .collect()
    };
    (pick(false), pick(true))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pal(name: &str, builtin: bool) -> Palette {
        Palette {
            name: name.into(),
            colors: Vec::new(),
            builtin,
        }
    }

    #[test]
    fn filter_groups_user_first_then_builtin_in_source_order() {
        let palettes = vec![
            pal("Sweetie 16", true),
            pal("PICO-8", true),
            pal("My Reds", false),
            pal("Greens", false),
        ];
        let (user, builtin) = filter(&palettes, "");
        assert_eq!(user, vec![2, 3]);
        assert_eq!(builtin, vec![0, 1]);
    }

    #[test]
    fn filter_matches_fuzzy_across_both_groups() {
        let palettes = vec![
            pal("Sweetie 16", true),
            pal("PICO-8", true),
            pal("Sweet User", false),
        ];
        let (user, builtin) = filter(&palettes, "swe");
        assert_eq!(user, vec![2]);
        assert_eq!(builtin, vec![0]);
    }

    #[test]
    fn filter_empty_when_nothing_matches() {
        let palettes = vec![pal("Sweetie 16", true), pal("Mine", false)];
        let (user, builtin) = filter(&palettes, "zzzz");
        assert!(user.is_empty());
        assert!(builtin.is_empty());
    }
}
