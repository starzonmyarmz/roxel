//! First-launch coachmark tour. Floating cards anchored to real widgets,
//! advanced via "Next" link / close button, dismissal persisted in
//! `Preferences`. Relaunchable from the macOS Help submenu and a `?` button
//! in the non-mac top bar.
//!
//! Cards do not highlight their referenced widget or draw arrows — proximity
//! to the anchor is the only spatial cue.
//!
//! Anchor rects are stashed in `OnboardingAnchors` each frame inside
//! `ui_system`; `Viewport` and `GizmoCube` re-use the existing `ViewportRect`
//! and `GizmoRect` resources to avoid duplicate plumbing.

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};

use crate::camera::ViewportRect;
use crate::gizmo::GizmoRect;
use crate::theme::{Preferences, PreferencesWindow, Theme, save_preferences};
use crate::ui::CommandPalette;
use crate::ui::icons;
use crate::ui::tokens::{font, gap, height, icon, pad, radius, space, width};
use roxel::grid::NewProject;

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum AnchorId {
    Viewport,
    ToolRail,
    ColorPalette,
    GizmoCube,
}

pub struct TourStep {
    pub anchor: AnchorId,
    pub title: &'static str,
    pub body: &'static str,
}

pub const TOUR_STEPS: &[TourStep] = &[
    TourStep {
        anchor: AnchorId::Viewport,
        title: "Paint and orbit",
        body: "Left-click to paint voxels. Right-drag to orbit, scroll to zoom.",
    },
    TourStep {
        anchor: AnchorId::ToolRail,
        title: "Tools",
        body: "Brush, Erase, Paint, Pick, and Shape — keys B/E/P/I/S. Long-press Shape for rect, ellipse, line.",
    },
    TourStep {
        anchor: AnchorId::ColorPalette,
        title: "Color and palette",
        body: "Pick a color or save favorites to the palette. Alt-click the eyedropper to keep sampling without leaving your tool.",
    },
    TourStep {
        anchor: AnchorId::GizmoCube,
        title: "Reframe the model",
        body: "Click a face of the gizmo to snap the camera. Cmd+0 (Ctrl+0) fits the whole model.",
    },
];

#[derive(Resource, Default)]
pub struct Onboarding {
    pub active: bool,
    pub step: usize,
    pub anchors_ready: bool,
    pub pending_persist: bool,
    autostart_fired: bool,
}

impl Onboarding {
    pub fn start(&mut self) {
        self.active = true;
        self.step = 0;
    }
    #[allow(dead_code)] // exercised by tests; useful state-machine accessor
    pub fn current(&self) -> Option<&'static TourStep> {
        if self.active {
            TOUR_STEPS.get(self.step)
        } else {
            None
        }
    }
    pub fn next(&mut self) {
        if !self.active {
            return;
        }
        if self.step + 1 >= TOUR_STEPS.len() {
            self.finish();
        } else {
            self.step += 1;
        }
    }
    pub fn skip(&mut self) {
        if !self.active {
            return;
        }
        self.finish();
    }
    #[allow(dead_code)] // exercised by tests; useful state-machine accessor
    pub fn is_done(&self) -> bool {
        !self.active
    }
    fn finish(&mut self) {
        self.active = false;
        self.step = 0;
        self.pending_persist = true;
    }
}

/// Per-frame anchor rects captured from `ui_system`. `Viewport` and
/// `GizmoCube` are sourced from the existing `ViewportRect` / `GizmoRect`
/// resources instead of being duplicated here.
#[derive(Resource, Default)]
pub struct OnboardingAnchors {
    pub tool_rail: Option<egui::Rect>,
    pub color_palette: Option<egui::Rect>,
}

fn rect_from_bevy(r: bevy::math::Rect) -> egui::Rect {
    egui::Rect::from_min_max(egui::pos2(r.min.x, r.min.y), egui::pos2(r.max.x, r.max.y))
}

fn resolve_anchor(
    id: AnchorId,
    anchors: &OnboardingAnchors,
    viewport: &ViewportRect,
    gizmo: &GizmoRect,
) -> Option<egui::Rect> {
    match id {
        AnchorId::Viewport => viewport.avail.map(rect_from_bevy),
        AnchorId::ToolRail => anchors.tool_rail,
        AnchorId::ColorPalette => anchors.color_palette,
        AnchorId::GizmoCube => gizmo.0.map(rect_from_bevy),
    }
}

/// Computes card position relative to the anchor, clamped inside `screen`.
/// Card extends downward and rightward from the returned `pos2`.
fn card_position(
    anchor: AnchorId,
    anchor_rect: egui::Rect,
    card_size: egui::Vec2,
    screen: egui::Rect,
) -> egui::Pos2 {
    let pad = space::MD;
    let raw = match anchor {
        AnchorId::ToolRail => egui::pos2(
            anchor_rect.left() - card_size.x - pad,
            anchor_rect.center().y - card_size.y * 0.5,
        ),
        AnchorId::ColorPalette => egui::pos2(
            anchor_rect.right() + pad,
            anchor_rect.center().y - card_size.y * 0.5,
        ),
        AnchorId::GizmoCube => egui::pos2(
            anchor_rect.center().x - card_size.x * 0.5,
            anchor_rect.bottom() + pad,
        ),
        AnchorId::Viewport => egui::pos2(
            anchor_rect.center().x - card_size.x * 0.5,
            anchor_rect.center().y - card_size.y * 0.5,
        ),
    };
    let min_x = raw.x.clamp(
        screen.min.x + pad,
        (screen.max.x - card_size.x - pad).max(screen.min.x + pad),
    );
    let min_y = raw.y.clamp(
        screen.min.y + pad,
        (screen.max.y - card_size.y - pad).max(screen.min.y + pad),
    );
    egui::pos2(min_x, min_y)
}

/// Returns true when a blocking modal/palette is open and the tour should
/// pause without losing state.
fn modal_blocks_tour(
    prefs_window: &PreferencesWindow,
    new_project: &NewProject,
    cmd_palette: &CommandPalette,
) -> bool {
    prefs_window.open || new_project.dialog_open || cmd_palette.open
}

fn hero_icon(anchor: AnchorId) -> egui::ImageSource<'static> {
    match anchor {
        AnchorId::Viewport => icons::square(),
        AnchorId::ToolRail => icons::brush(),
        AnchorId::ColorPalette => icons::pipette(),
        AnchorId::GizmoCube => icons::box_select(),
    }
}

#[derive(SystemParam)]
pub struct TourAnchors<'w> {
    anchors: Res<'w, OnboardingAnchors>,
    viewport: Res<'w, ViewportRect>,
    gizmo: Res<'w, GizmoRect>,
}

#[derive(SystemParam)]
pub struct TourModals<'w> {
    prefs_window: Res<'w, PreferencesWindow>,
    new_project: Res<'w, NewProject>,
    cmd_palette: Res<'w, CommandPalette>,
}

pub fn onboarding_overlay_system(
    mut contexts: EguiContexts,
    mut onboarding: ResMut<Onboarding>,
    anchor_params: TourAnchors,
    theme: Res<Theme>,
    mut prefs: ResMut<Preferences>,
    modals: TourModals,
) {
    let Ok(ctx) = contexts.ctx_mut() else { return };
    onboarding.anchors_ready = true;

    if onboarding.pending_persist {
        if !prefs.onboarding_seen {
            prefs.onboarding_seen = true;
            save_preferences(&prefs);
        }
        onboarding.pending_persist = false;
    }

    if !onboarding.active {
        return;
    }
    if modal_blocks_tour(
        &modals.prefs_window,
        &modals.new_project,
        &modals.cmd_palette,
    ) {
        return;
    }

    let step_idx = onboarding.step;
    let Some(step) = TOUR_STEPS.get(step_idx) else {
        onboarding.active = false;
        onboarding.step = 0;
        onboarding.pending_persist = true;
        return;
    };
    let anchor_rect = match resolve_anchor(
        step.anchor,
        &anchor_params.anchors,
        &anchor_params.viewport,
        &anchor_params.gizmo,
    ) {
        Some(r) => r,
        None => {
            if step_idx + 1 >= TOUR_STEPS.len() {
                onboarding.active = false;
                onboarding.step = 0;
                onboarding.pending_persist = true;
            } else {
                onboarding.step = step_idx + 1;
            }
            return;
        }
    };

    let screen = ctx.content_rect();
    let total = TOUR_STEPS.len();
    let is_last = step_idx + 1 == total;

    let card_w = width::COACHMARK;
    let est_body_h = 140.0;
    let card_size = egui::vec2(card_w, height::COACHMARK_HERO + est_body_h);
    let card_pos = card_position(step.anchor, anchor_rect, card_size, screen);

    let mut next_clicked = false;
    let mut close_clicked = false;
    let area_id = egui::Id::new("onboarding_card");
    egui::Area::new(area_id)
        .order(egui::Order::Foreground)
        .fade_in(false)
        .fixed_pos(card_pos)
        .show(ctx, |ui| {
            egui::Frame::popup(ui.style())
                .fill(theme.panel)
                .stroke(egui::Stroke::NONE)
                .shadow(crate::ui::tokens::shadow::mid())
                .corner_radius(egui::CornerRadius::same(radius::LG))
                .inner_margin(egui::Margin::ZERO)
                .show(ui, |ui| {
                    ui.set_width(card_w);

                    // Hero band — accent-tinted fill + large step icon.
                    let (hero_rect, _) = ui.allocate_exact_size(
                        egui::vec2(card_w, height::COACHMARK_HERO),
                        egui::Sense::hover(),
                    );
                    let painter = ui.painter_at(hero_rect);
                    // Top-rounded fill only — bottom corners square so the
                    // hero meets the body cleanly.
                    let hero_radius = egui::CornerRadius {
                        nw: radius::LG,
                        ne: radius::LG,
                        sw: 0,
                        se: 0,
                    };
                    painter.rect_filled(hero_rect, hero_radius, theme.accent_dim);
                    // Soft inner highlight: top-half slightly lighter to give
                    // depth without a literal gradient (egui has no gradient
                    // primitive — two stacked rects approximate it).
                    let top_half = egui::Rect::from_min_max(
                        hero_rect.min,
                        egui::pos2(hero_rect.max.x, hero_rect.center().y),
                    );
                    painter.rect_filled(
                        top_half,
                        egui::CornerRadius {
                            nw: radius::LG,
                            ne: radius::LG,
                            sw: 0,
                            se: 0,
                        },
                        egui::Color32::from_rgba_unmultiplied(255, 255, 255, 14),
                    );

                    // Centered hero icon, tinted accent.
                    let icon_size = icon::hero_square();
                    let icon_rect = egui::Rect::from_center_size(hero_rect.center(), icon_size);
                    egui::Image::new(hero_icon(step.anchor))
                        .tint(theme.accent)
                        .paint_at(ui, icon_rect);

                    // Close X overlay — top-right of hero.
                    let close_size = egui::vec2(28.0, 28.0);
                    let close_pos = egui::pos2(
                        hero_rect.max.x - close_size.x - space::SM,
                        hero_rect.min.y + space::SM,
                    );
                    let close_rect = egui::Rect::from_min_size(close_pos, close_size);
                    let close_resp =
                        ui.interact(close_rect, area_id.with("close"), egui::Sense::click());
                    let close_bg = if close_resp.hovered() {
                        egui::Color32::from_rgba_unmultiplied(0, 0, 0, 96)
                    } else {
                        egui::Color32::from_rgba_unmultiplied(0, 0, 0, 56)
                    };
                    ui.painter()
                        .circle_filled(close_rect.center(), close_size.x * 0.5, close_bg);
                    let x_size = egui::vec2(icon::SM, icon::SM);
                    let x_rect = egui::Rect::from_center_size(close_rect.center(), x_size);
                    egui::Image::new(icons::x())
                        .tint(theme.text)
                        .paint_at(ui, x_rect);
                    if close_resp.clicked() {
                        close_clicked = true;
                    }

                    // Body region.
                    let body_margin =
                        egui::Margin::symmetric(pad::DIALOG.x as i8, pad::DIALOG.y as i8 + 4);
                    egui::Frame::NONE.inner_margin(body_margin).show(ui, |ui| {
                        ui.spacing_mut().item_spacing = gap::DEFAULT;
                        ui.label(
                            egui::RichText::new(step.title)
                                .size(font::HEADING)
                                .strong()
                                .color(theme.text),
                        );
                        ui.add_space(space::XS);
                        ui.label(
                            egui::RichText::new(step.body)
                                .size(font::BODY)
                                .color(theme.text_dim),
                        );
                        ui.add_space(space::MD);

                        // Footer: pagination dots (left) + Next/Done link (right).
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing = gap::TIGHT;
                            let dot_d = 6.0;
                            let dot_size = egui::vec2(dot_d, dot_d);
                            for i in 0..total {
                                let (rect, _) =
                                    ui.allocate_exact_size(dot_size, egui::Sense::hover());
                                let fill = if i == step_idx {
                                    theme.accent
                                } else {
                                    theme.border
                                };
                                ui.painter().circle_filled(rect.center(), dot_d * 0.5, fill);
                            }
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    let label = if is_last { "Done" } else { "Next" };
                                    let link = egui::RichText::new(label)
                                        .size(font::BODY)
                                        .strong()
                                        .color(theme.accent);
                                    if ui
                                        .add(egui::Label::new(link).sense(egui::Sense::click()))
                                        .on_hover_cursor(egui::CursorIcon::PointingHand)
                                        .clicked()
                                    {
                                        next_clicked = true;
                                    }
                                },
                            );
                        });
                    });
                });
        });

    if next_clicked {
        onboarding.next();
    } else if close_clicked {
        onboarding.skip();
    }
}

/// Runs in `Update` and starts the tour exactly once per launch when prefs
/// say it hasn't been seen yet. Waits on `anchors_ready` so the first
/// `ui_system` pass has run (fonts loaded, anchors populated).
pub fn onboarding_autostart_system(
    mut onboarding: ResMut<Onboarding>,
    prefs: Res<Preferences>,
    prefs_window: Res<PreferencesWindow>,
    new_project: Res<NewProject>,
) {
    if onboarding.autostart_fired {
        return;
    }
    if !onboarding.anchors_ready {
        return;
    }
    onboarding.autostart_fired = true;
    if prefs.onboarding_seen {
        return;
    }
    if prefs_window.open || new_project.dialog_open {
        return;
    }
    onboarding.start();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tour_starts_at_first_step() {
        let mut o = Onboarding::default();
        o.start();
        assert!(o.active);
        assert_eq!(o.step, 0);
        assert_eq!(o.current().unwrap().anchor, AnchorId::Viewport);
    }

    #[test]
    fn next_advances_through_each_step() {
        let mut o = Onboarding::default();
        o.start();
        for expected in 1..TOUR_STEPS.len() {
            o.next();
            assert!(o.active, "tour ended early at step {expected}");
            assert_eq!(o.step, expected);
            assert!(o.current().is_some());
        }
    }

    #[test]
    fn next_on_last_step_finishes_and_sets_persist() {
        let mut o = Onboarding::default();
        o.start();
        for _ in 0..TOUR_STEPS.len() - 1 {
            o.next();
        }
        o.next();
        assert!(!o.active);
        assert!(o.pending_persist);
        assert!(o.current().is_none());
        assert!(o.is_done());
    }

    #[test]
    fn skip_finishes_immediately() {
        let mut o = Onboarding::default();
        o.start();
        o.skip();
        assert!(!o.active);
        assert!(o.pending_persist);
    }

    #[test]
    fn restart_resets_step_to_zero() {
        let mut o = Onboarding::default();
        o.start();
        o.next();
        o.next();
        o.skip();
        o.start();
        assert!(o.active);
        assert_eq!(o.step, 0);
    }

    #[test]
    fn next_when_inactive_is_noop() {
        let mut o = Onboarding::default();
        o.next();
        assert!(!o.active);
        assert_eq!(o.step, 0);
        assert!(!o.pending_persist);
    }

    #[test]
    fn skip_when_inactive_is_noop() {
        let mut o = Onboarding::default();
        o.skip();
        assert!(!o.pending_persist);
    }

    #[test]
    fn tour_steps_count_is_four() {
        assert_eq!(TOUR_STEPS.len(), 4);
    }

    #[test]
    fn every_step_has_nonempty_copy_under_140_chars() {
        for (i, s) in TOUR_STEPS.iter().enumerate() {
            assert!(!s.title.is_empty(), "step {i} title empty");
            assert!(!s.body.is_empty(), "step {i} body empty");
            assert!(
                s.body.chars().count() <= 140,
                "step {i} body too long: {} chars",
                s.body.chars().count()
            );
            assert!(
                s.title.chars().count() <= 40,
                "step {i} title too long: {} chars",
                s.title.chars().count()
            );
        }
    }

    #[test]
    fn anchor_ids_are_distinct_across_steps() {
        // Each step targets a different widget — guards against accidental
        // duplicate anchors that would feel redundant to the user.
        let ids: Vec<AnchorId> = TOUR_STEPS.iter().map(|s| s.anchor).collect();
        for i in 0..ids.len() {
            for j in (i + 1)..ids.len() {
                assert_ne!(ids[i], ids[j], "duplicate anchor at steps {i} and {j}");
            }
        }
    }
}
