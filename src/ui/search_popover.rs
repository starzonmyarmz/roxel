//! The Cmd+K-style search popover shell shared by the command palette and the
//! palette switcher: centred-top window, surface-framed search field that
//! keeps focus, divider, hidden-scrollbar result list (or an empty-state
//! hint), divider, and the ↑↓ / ↵ / Esc footer.

use super::tokens::{font, gap, height, icon, pad, radius, space, width};
use super::{icons, widgets};
use crate::theme::Theme;
use bevy_egui::egui;

/// Arrow / Enter / Esc presses, consumed before the search field sees them so
/// its caret stays put while the list is navigated.
#[derive(Clone, Copy, Debug, Default)]
pub struct SearchNav {
    pub down: bool,
    pub up: bool,
    pub enter: bool,
    pub esc: bool,
}

impl SearchNav {
    pub fn consume(ctx: &egui::Context) -> Self {
        ctx.input_mut(|i| {
            let mut take = |key| i.consume_key(egui::Modifiers::NONE, key);
            SearchNav {
                down: take(egui::Key::ArrowDown),
                up: take(egui::Key::ArrowUp),
                enter: take(egui::Key::Enter),
                esc: take(egui::Key::Escape),
            }
        })
    }
}

/// Copy for one popover. `enter_label` names what ↵ does in the footer.
pub struct SearchPopover<'a> {
    pub id: &'a str,
    pub hint: &'a str,
    pub empty: &'a str,
    pub enter_label: &'a str,
}

/// Draws the popover around `rows` (or `popover.empty` when `has_rows` is
/// false). Returns `true` when a click landed outside it this frame.
pub fn show(
    ctx: &egui::Context,
    theme: &Theme,
    popover: &SearchPopover,
    search: &mut String,
    just_opened: &mut bool,
    has_rows: bool,
    rows: impl FnOnce(&mut egui::Ui),
) -> bool {
    let window = egui::Window::new(popover.id)
        .title_bar(false)
        .collapsible(false)
        .resizable(false)
        .order(egui::Order::Foreground)
        .anchor(egui::Align2::CENTER_TOP, [0.0, space::POPOVER_TOP])
        .default_width(width::COMMAND_PALETTE)
        .min_width(width::COMMAND_PALETTE)
        .frame(widgets::modal_frame(theme, pad::SEARCH))
        .show(ctx, |ui| {
            ui.set_min_width(width::COMMAND_PALETTE - pad::SEARCH.x * 2.0);

            let resp = search_field(ui, theme, search, popover.hint);
            if *just_opened {
                resp.request_focus();
                *just_opened = false;
            } else if !resp.has_focus() && !resp.lost_focus() {
                resp.request_focus();
            }

            ui.add_space(space::XS);
            widgets::hline_at(ui, theme, ui.cursor().min.y);
            ui.add_space(space::SX);

            if has_rows {
                egui::ScrollArea::vertical()
                    .max_height(height::COMMAND_PALETTE_MAX)
                    .auto_shrink([false, true])
                    .scroll_bar_visibility(
                        egui::containers::scroll_area::ScrollBarVisibility::AlwaysHidden,
                    )
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing.y = space::XXS;
                        rows(ui);
                    });
            } else {
                ui.add_space(space::SM);
                widgets::hint_label(ui, theme, popover.empty);
                ui.add_space(space::SM);
            }

            ui.add_space(space::SX);
            widgets::hline_at(ui, theme, ui.cursor().min.y);
            ui.add_space(space::SX);
            footer_hint(ui, theme, popover.enter_label);
        });

    window.is_some_and(|wr| {
        let rect = wr.response.rect;
        ctx.input(|i| {
            i.pointer.any_click()
                && !rect.contains(i.pointer.interact_pos().unwrap_or(egui::Pos2::ZERO))
        })
    })
}

fn search_field(
    ui: &mut egui::Ui,
    theme: &Theme,
    search: &mut String,
    hint: &str,
) -> egui::Response {
    let margin = egui::Margin::symmetric(space::SM as i8, space::SX as i8);
    egui::Frame::new()
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
                    egui::TextEdit::singleline(search)
                        .desired_width(f32::INFINITY)
                        .hint_text(hint)
                        .font(egui::TextStyle::Body)
                        .frame(egui::Frame::NONE),
                )
            })
            .inner
        })
        .inner
}

fn footer_hint(ui: &mut egui::Ui, theme: &Theme, enter_label: &str) {
    let chip_color = theme.text;
    let label_color = theme.text_dim;
    let chip_h = icon::XS + space::SM;
    let chip_pad_x = space::SX;

    let icon_chip = |ui: &mut egui::Ui, src: egui::ImageSource<'static>| {
        let w = icon::XS + chip_pad_x * 2.0;
        let (rect, _) = ui.allocate_exact_size(egui::vec2(w, chip_h), egui::Sense::hover());
        ui.painter()
            .rect_filled(rect, egui::CornerRadius::same(radius::XS), theme.surface);
        let img_rect = egui::Rect::from_center_size(rect.center(), icon::xs_square());
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
        let pos = rect.center() - galley.size() * 0.5;
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
        plain(ui, enter_label);
        ui.add_space(space::FOOTER_GROUP);
        text_chip(ui, "Esc");
        ui.add_space(space::XXS);
        plain(ui, "Close");
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(key: egui::Key) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }
    }

    #[test]
    fn nav_keys_are_consumed_before_the_field_sees_them() {
        let ctx = egui::Context::default();
        let input = egui::RawInput {
            events: vec![key(egui::Key::ArrowDown), key(egui::Key::Enter)],
            ..Default::default()
        };
        ctx.run_ui(input, |ui| {
            let ctx = ui.ctx();
            let nav = SearchNav::consume(ctx);
            assert!(nav.down && nav.enter);
            assert!(!nav.up && !nav.esc);
            let again = SearchNav::consume(ctx);
            assert!(!again.down && !again.enter);
        })
        .drop_without_applying_deltas();
    }
}
