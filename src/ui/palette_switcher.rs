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
use super::tokens::{font, gap, height, icon, radius, size, space, stroke, swatch, width};
use super::{icons, widgets};
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

    // Pre-consume nav keys so the search field doesn't move the caret.
    let (mut down, mut up, mut enter, mut esc) = (false, false, false, false);
    ctx.input_mut(|i| {
        down = i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown);
        up = i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp);
        enter = i.consume_key(egui::Modifiers::NONE, egui::Key::Enter);
        esc = i.consume_key(egui::Modifiers::NONE, egui::Key::Escape);
    });

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
        if down {
            switcher.selected = (switcher.selected + 1) % n;
        }
        if up {
            switcher.selected = (switcher.selected + n - 1) % n;
        }
    }

    let prev_search = switcher.search.clone();
    let mut chosen: Option<usize> = None;
    let mut close_after = esc;
    if enter {
        chosen = selectable.get(switcher.selected).copied();
    }

    let window_response = egui::Window::new("Palette switcher")
        .title_bar(false)
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_TOP, [0.0, 60.0])
        .default_width(width::COMMAND_PALETTE)
        .min_width(width::COMMAND_PALETTE)
        .frame(
            egui::Frame::window(&ctx.style())
                .fill(theme.panel)
                .inner_margin(egui::Margin::symmetric(14, 12))
                .stroke(egui::Stroke::NONE)
                .shadow(crate::ui::tokens::shadow::high())
                .corner_radius(egui::CornerRadius::same(radius::LG)),
        )
        .show(ctx, |ui| {
            ui.set_min_width(width::COMMAND_PALETTE - 28.0);

            let margin = egui::Margin::symmetric(space::SM as i8, space::SX as i8);
            let inner = egui::Frame::new()
                .fill(theme.surface)
                .corner_radius(egui::CornerRadius::same(radius::SM))
                .inner_margin(margin)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::Image::new(icons::search())
                                .fit_to_exact_size(icon::md_square())
                                .tint(theme.text_dim),
                        );
                        ui.add_space(space::XS);
                        ui.add(
                            egui::TextEdit::singleline(&mut switcher.search)
                                .desired_width(f32::INFINITY)
                                .hint_text("Search palettes…")
                                .font(egui::TextStyle::Body)
                                .frame(false),
                        )
                    })
                    .inner
                });
            let resp = inner.inner;
            if switcher.just_opened {
                resp.request_focus();
                switcher.just_opened = false;
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

            if selectable.is_empty() {
                ui.add_space(space::SM);
                widgets::hint_label(ui, theme, "No palettes match.");
                ui.add_space(space::SM);
            } else {
                egui::ScrollArea::vertical()
                    .max_height(height::COMMAND_PALETTE_MAX)
                    .auto_shrink([false, true])
                    .scroll_bar_visibility(
                        egui::containers::scroll_area::ScrollBarVisibility::AlwaysHidden,
                    )
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing.y = space::XXS;
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
                    });
            }

            ui.add_space(space::SX);
            ui.painter().hline(
                ui.clip_rect().x_range(),
                ui.cursor().min.y,
                egui::Stroke::new(stroke::HAIR, theme.border),
            );
            ui.add_space(space::SX);
            footer_hint(ui, theme);
        });

    // Click outside closes (matches command palette) — but not the opening click,
    // which is still registered this frame and lands on the `…` menu.
    if let Some(wr) = window_response {
        if !opened_this_frame {
            let rect = wr.response.rect;
            let clicked_outside = ctx.input(|i| {
                i.pointer.any_click()
                    && !rect.contains(i.pointer.interact_pos().unwrap_or(egui::Pos2::ZERO))
            });
            if clicked_outside {
                close_after = true;
            }
        }
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
        egui::vec2(ui.available_width(), size::CMD_PALETTE_ROW * 0.8),
        egui::Sense::hover(),
    );
    ui.painter().text(
        rect.left_center() + egui::vec2(10.0, 0.0),
        egui::Align2::LEFT_CENTER,
        label.to_uppercase(),
        egui::FontId::new(
            font::SECTION,
            egui::FontFamily::Name(crate::theme::INTER_SEMIBOLD_FAMILY.into()),
        ),
        theme.text_muted,
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
            rect.left() + 10.0,
            rect.center().y - name_galley.size().y * 0.5,
        ),
        name_galley,
        theme.text,
    );

    // Swatch preview (right). Tall, narrow cells (12×24); reserve the right
    // ~60% of the row and show as many as land in it.
    let sw = swatch::PREVIEW_SM.x;
    let sh = swatch::PREVIEW_SM.y;
    let step = sw + space::XXS;
    let max_count = ((full * 0.6) / step).floor().max(0.0) as usize;
    let count = pal.colors.len().min(max_count);
    let mut x = rect.right() - 10.0 - count as f32 * step + space::XXS;
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

fn footer_hint(ui: &mut egui::Ui, theme: &Theme) {
    let chip_color = theme.text;
    let label_color = theme.text_dim;
    let chip_h = icon::XS + 8.0;
    let chip_pad_x = 6.0;

    let icon_chip = |ui: &mut egui::Ui, src: egui::ImageSource<'static>| {
        let w = icon::XS + chip_pad_x * 2.0;
        let (rect, _) = ui.allocate_exact_size(egui::vec2(w, chip_h), egui::Sense::hover());
        ui.painter()
            .rect_filled(rect, egui::CornerRadius::same(radius::XS), theme.surface);
        let img_rect = egui::Rect::from_center_size(rect.center(), egui::vec2(icon::XS, icon::XS));
        egui::Image::new(src)
            .tint(chip_color)
            .paint_at(ui, img_rect);
    };
    let text_chip = |ui: &mut egui::Ui, s: &str| {
        let galley = ui.painter().layout_no_wrap(
            s.to_string(),
            egui::FontId::monospace(font::SMALL),
            chip_color,
        );
        let w = galley.size().x + chip_pad_x * 2.0;
        let (rect, _) = ui.allocate_exact_size(egui::vec2(w, chip_h), egui::Sense::hover());
        ui.painter()
            .rect_filled(rect, egui::CornerRadius::same(radius::XS), theme.surface);
        let pos = egui::pos2(
            rect.center().x - galley.size().x * 0.5,
            rect.center().y - galley.size().y * 0.5,
        );
        ui.painter().galley(pos, galley, chip_color);
    };
    let plain = |ui: &mut egui::Ui, s: &str| {
        let galley = ui.painter().layout_no_wrap(
            s.to_string(),
            egui::FontId::proportional(font::SMALL),
            label_color,
        );
        let (rect, _) = ui.allocate_exact_size(galley.size(), egui::Sense::hover());
        ui.painter().galley(rect.min, galley, label_color);
    };

    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = gap::TIGHT.x;
        ui.spacing_mut().interact_size.y = chip_h;
        icon_chip(ui, icons::arrow_up());
        icon_chip(ui, icons::arrow_down());
        ui.add_space(space::XXS);
        plain(ui, "Navigate");
        ui.add_space(space::FOOTER_GROUP);
        icon_chip(ui, icons::corner_down_left());
        ui.add_space(space::XXS);
        plain(ui, "Switch");
        ui.add_space(space::FOOTER_GROUP);
        text_chip(ui, "Esc");
        ui.add_space(space::XXS);
        plain(ui, "Close");
    });
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
