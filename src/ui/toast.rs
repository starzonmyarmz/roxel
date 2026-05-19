use crate::theme::Theme;
use crate::ui::tokens::{font, radius, size, stroke, width};
use bevy::prelude::*;
use bevy_egui::egui;
use std::collections::VecDeque;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ToastKind {
    Success,
    Error,
    #[allow(dead_code)] // reserved for future neutral notifications
    Info,
}

#[derive(Clone, Debug)]
pub struct Toast {
    pub message: String,
    pub kind: ToastKind,
    pub remaining: f32,
}

pub const SUCCESS_TTL: f32 = 3.5;
pub const ERROR_TTL: f32 = 6.0;
#[allow(dead_code)]
pub const INFO_TTL: f32 = 3.5;
pub const MAX_TOASTS: usize = 4;

const FADE_WINDOW: f32 = 0.5;

#[derive(Resource, Default)]
pub struct Toasts(pub VecDeque<Toast>);

impl Toasts {
    pub fn success(&mut self, message: impl Into<String>) {
        self.push(ToastKind::Success, message.into(), SUCCESS_TTL);
    }
    pub fn error(&mut self, message: impl Into<String>) {
        self.push(ToastKind::Error, message.into(), ERROR_TTL);
    }
    #[allow(dead_code)]
    pub fn info(&mut self, message: impl Into<String>) {
        self.push(ToastKind::Info, message.into(), INFO_TTL);
    }
    fn push(&mut self, kind: ToastKind, message: String, remaining: f32) {
        if self.0.len() >= MAX_TOASTS {
            self.0.pop_front();
        }
        self.0.push_back(Toast {
            message,
            kind,
            remaining,
        });
    }
}

pub fn tick(toasts: &mut Toasts, dt: f32) {
    for t in toasts.0.iter_mut() {
        t.remaining -= dt;
    }
    toasts.0.retain(|t| t.remaining > 0.0);
}

pub fn toast_lifetime_system(mut toasts: ResMut<Toasts>, time: Res<Time>) {
    tick(&mut toasts, time.delta_secs());
}

pub fn draw_toasts(ctx: &egui::Context, theme: &Theme, toasts: &Toasts) {
    if toasts.0.is_empty() {
        return;
    }
    // Anchor at bottom-center of the canvas (the region left after every panel
    // is registered). `available_rect` reflects that area at this point in the
    // frame since `draw_toasts` is called last in `ui_system`.
    let canvas = ctx.available_rect();
    let center_x = (canvas.min.x + canvas.max.x) * 0.5;
    let bottom_y = canvas.max.y - 16.0;
    egui::Area::new(egui::Id::new("toast-stack"))
        .order(egui::Order::Foreground)
        .fixed_pos(egui::pos2(center_x, bottom_y))
        .pivot(egui::Align2::CENTER_BOTTOM)
        .show(ctx, |ui| {
            ui.set_max_width(width::TOAST);
            ui.spacing_mut().item_spacing.y = 6.0;
            for t in toasts.0.iter() {
                let accent = match t.kind {
                    ToastKind::Success => egui::Color32::from_rgb(76, 175, 102),
                    ToastKind::Error => egui::Color32::from_rgb(220, 90, 90),
                    ToastKind::Info => theme.accent,
                };
                let fade = (t.remaining / FADE_WINDOW).clamp(0.0, 1.0);
                let fill = lerp_color(theme.surface, theme.panel, fade);
                egui::Frame::default()
                    .fill(fill)
                    .stroke(egui::Stroke::new(stroke::HAIR, theme.border))
                    .corner_radius(egui::CornerRadius::same(radius::MD))
                    .inner_margin(egui::Margin::symmetric(12, 8))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            let (rect, _) =
                                ui.allocate_exact_size(size::TOAST_ACCENT, egui::Sense::hover());
                            ui.painter().rect_filled(
                                rect,
                                egui::CornerRadius::same(radius::XS),
                                accent,
                            );
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(&t.message)
                                        .color(theme.text)
                                        .size(font::BODY),
                                )
                                .wrap(),
                            );
                        });
                    });
            }
        });
}

fn lerp_color(from: egui::Color32, to: egui::Color32, t: f32) -> egui::Color32 {
    let lerp = |a: u8, b: u8| -> u8 {
        let af = a as f32;
        let bf = b as f32;
        (af + (bf - af) * (1.0 - t)).round().clamp(0.0, 255.0) as u8
    };
    egui::Color32::from_rgba_unmultiplied(
        lerp(from.r(), to.r()),
        lerp(from.g(), to.g()),
        lerp(from.b(), to.b()),
        lerp(from.a(), to.a()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_success_appends_with_ttl() {
        let mut t = Toasts::default();
        t.success("ok");
        assert_eq!(t.0.len(), 1);
        assert_eq!(t.0[0].kind, ToastKind::Success);
        assert!((t.0[0].remaining - SUCCESS_TTL).abs() < f32::EPSILON);
        assert_eq!(t.0[0].message, "ok");
    }

    #[test]
    fn error_uses_longer_ttl_than_success() {
        let mut t = Toasts::default();
        t.success("a");
        t.error("b");
        assert!(t.0[1].remaining > t.0[0].remaining);
        assert_eq!(t.0[1].kind, ToastKind::Error);
    }

    #[test]
    fn cap_evicts_oldest() {
        let mut t = Toasts::default();
        for i in 0..(MAX_TOASTS + 1) {
            t.info(format!("m{i}"));
        }
        assert_eq!(t.0.len(), MAX_TOASTS);
        assert_eq!(t.0.front().unwrap().message, "m1");
        assert_eq!(t.0.back().unwrap().message, format!("m{}", MAX_TOASTS));
    }

    #[test]
    fn tick_decrements_and_drops_expired() {
        let mut t = Toasts::default();
        t.success("keep");
        t.success("drop");
        t.0[1].remaining = 0.1;
        tick(&mut t, 0.2);
        assert_eq!(t.0.len(), 1);
        assert_eq!(t.0[0].message, "keep");
        assert!(t.0[0].remaining < SUCCESS_TTL);
    }
}
