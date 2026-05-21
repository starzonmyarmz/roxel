//! First-launch coachmark tour. Five floating tooltips anchored to real
//! widgets, advanced via Next/Skip, dismissal persisted in `Preferences`.
//! Relaunchable from the macOS Help submenu and a `?` button in the non-mac
//! top bar.
//!
//! Anchor rects are stashed in `OnboardingAnchors` each frame inside
//! `ui_system`; `Viewport` and `GizmoCube` re-use the existing `ViewportRect`
//! and `GizmoRect` resources to avoid duplicate plumbing.

use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};

use crate::camera::ViewportRect;
use crate::gizmo::GizmoRect;
use crate::grid::NewProject;
use crate::theme::{Preferences, PreferencesWindow, Theme, save_preferences};
use crate::ui::CommandPalette;
use crate::ui::tokens::{font, gap, pad, radius, space, stroke, width};

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum AnchorId {
    Viewport,
    ToolRail,
    ColorPalette,
    GizmoCube,
    SaveButton,
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
    TourStep {
        anchor: AnchorId::SaveButton,
        title: "Save your work",
        body: "Cmd+S (Ctrl+S) saves a .rox project. You're set — start building.",
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
    pub save_button: Option<egui::Rect>,
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
        AnchorId::SaveButton => anchors.save_button.or_else(|| {
            // macOS: no Save button widget — fall back to the top-right of the
            // viewport so the keyboard-shortcut copy still has somewhere
            // visually anchored to read from.
            let v = viewport.avail.map(rect_from_bevy)?;
            let size = egui::vec2(120.0, 32.0);
            let top_right = egui::pos2(v.max.x - 12.0, v.min.y + 12.0);
            Some(egui::Rect::from_min_size(
                top_right - egui::vec2(size.x, 0.0),
                size,
            ))
        }),
    }
}

/// Computes bubble position relative to the anchor, clamped inside `screen`.
/// Bubble extends downward and rightward from the returned `pos2`.
fn bubble_position(
    anchor: AnchorId,
    anchor_rect: egui::Rect,
    bubble_size: egui::Vec2,
    screen: egui::Rect,
) -> egui::Pos2 {
    let pad = 12.0;
    let raw = match anchor {
        AnchorId::ToolRail => egui::pos2(
            anchor_rect.right() + pad,
            anchor_rect.center().y - bubble_size.y * 0.5,
        ),
        AnchorId::ColorPalette => egui::pos2(
            anchor_rect.left() - bubble_size.x - pad,
            anchor_rect.center().y - bubble_size.y * 0.5,
        ),
        AnchorId::GizmoCube => egui::pos2(
            anchor_rect.center().x - bubble_size.x * 0.5,
            anchor_rect.bottom() + pad,
        ),
        AnchorId::SaveButton => egui::pos2(
            anchor_rect.center().x - bubble_size.x * 0.5,
            anchor_rect.bottom() + pad,
        ),
        AnchorId::Viewport => egui::pos2(
            anchor_rect.center().x - bubble_size.x * 0.5,
            anchor_rect.center().y - bubble_size.y * 0.5,
        ),
    };
    let min_x = raw.x.clamp(
        screen.min.x + pad,
        (screen.max.x - bubble_size.x - pad).max(screen.min.x + pad),
    );
    let min_y = raw.y.clamp(
        screen.min.y + pad,
        (screen.max.y - bubble_size.y - pad).max(screen.min.y + pad),
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

pub fn onboarding_overlay_system(
    mut contexts: EguiContexts,
    mut onboarding: ResMut<Onboarding>,
    anchors: Res<OnboardingAnchors>,
    viewport: Res<ViewportRect>,
    gizmo: Res<GizmoRect>,
    theme: Res<Theme>,
    mut prefs: ResMut<Preferences>,
    prefs_window: Res<PreferencesWindow>,
    new_project: Res<NewProject>,
    cmd_palette: Res<CommandPalette>,
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
    if modal_blocks_tour(&prefs_window, &new_project, &cmd_palette) {
        return;
    }

    let step_idx = onboarding.step;
    let Some(step) = TOUR_STEPS.get(step_idx) else {
        onboarding.active = false;
        onboarding.step = 0;
        onboarding.pending_persist = true;
        return;
    };
    let anchor_rect = match resolve_anchor(step.anchor, &anchors, &viewport, &gizmo) {
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

    // First-pass: measure the bubble with an offscreen layout so we can place
    // the real one. Egui doesn't expose an explicit measure pass, so we use a
    // generous fixed width and a height estimate.
    let est_height = 96.0;
    let bubble_size = egui::vec2(width::COACHMARK + pad::BUTTON.x * 2.0, est_height);
    let bubble_pos = bubble_position(step.anchor, anchor_rect, bubble_size, screen);

    let mut next_clicked = false;
    let mut skip_clicked = false;
    let area_id = egui::Id::new("onboarding_bubble");
    let area_resp = egui::Area::new(area_id)
        .order(egui::Order::Foreground)
        .fade_in(false)
        .fixed_pos(bubble_pos)
        .show(ctx, |ui| {
            egui::Frame::popup(ui.style())
                .fill(theme.panel)
                .stroke(egui::Stroke::new(stroke::ACCENT, theme.accent))
                .corner_radius(egui::CornerRadius::same(radius::MD))
                .inner_margin(egui::Margin::symmetric(
                    pad::BUTTON.x as i8,
                    pad::BUTTON.y as i8 + 4,
                ))
                .show(ui, |ui| {
                    ui.set_max_width(width::COACHMARK);
                    ui.label(
                        egui::RichText::new(step.title)
                            .size(font::BODY)
                            .strong()
                            .color(theme.text),
                    );
                    ui.add_space(space::XS);
                    ui.label(
                        egui::RichText::new(step.body)
                            .size(font::SMALL)
                            .color(theme.text_dim),
                    );
                    ui.add_space(space::SM);
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing = gap::DEFAULT;
                        ui.label(
                            egui::RichText::new(format!("{}/{}", step_idx + 1, total))
                                .size(font::SMALL)
                                .color(theme.text_dim),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let next_label = if is_last { "Finish" } else { "Next" };
                            if ui.button(next_label).clicked() {
                                next_clicked = true;
                            }
                            if !is_last && ui.button("Skip tour").clicked() {
                                skip_clicked = true;
                            }
                        });
                    });
                });
        });

    // Connector overlay: highlight ring + short line + arrowhead. Drawn on
    // the Foreground layer so it sits above panels but below the bubble area
    // (which is also Foreground but added last).
    let bubble_rect = area_resp.response.rect;
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("onboarding_connector"),
    ));
    painter.rect_stroke(
        anchor_rect.expand(2.0),
        egui::CornerRadius::same(radius::MD),
        egui::Stroke::new(stroke::ACCENT, theme.accent),
        egui::StrokeKind::Outside,
    );
    if let Some((from, to)) = nearest_edge_segment(bubble_rect, anchor_rect) {
        painter.line_segment(
            [from, to],
            egui::Stroke::new(stroke::HAIR + 1.0, theme.accent),
        );
        // Tiny arrowhead at the anchor end.
        let dir = (to - from).normalized();
        let perp = egui::vec2(-dir.y, dir.x);
        let tip = to;
        let base = to - dir * 6.0;
        let a = base + perp * 4.0;
        let b = base - perp * 4.0;
        painter.add(egui::Shape::convex_polygon(
            vec![tip, a, b],
            theme.accent,
            egui::Stroke::NONE,
        ));
    }

    if next_clicked {
        onboarding.next();
    } else if skip_clicked {
        onboarding.skip();
    }
}

/// Returns the shortest segment between the closest edge midpoints of two
/// non-overlapping rects. Returns `None` when the rects overlap (no useful
/// connector to draw).
fn nearest_edge_segment(
    bubble: egui::Rect,
    anchor: egui::Rect,
) -> Option<(egui::Pos2, egui::Pos2)> {
    if bubble.intersects(anchor) {
        return None;
    }
    let bubble_pts = [
        egui::pos2(bubble.center().x, bubble.top()),
        egui::pos2(bubble.center().x, bubble.bottom()),
        egui::pos2(bubble.left(), bubble.center().y),
        egui::pos2(bubble.right(), bubble.center().y),
    ];
    let anchor_pts = [
        egui::pos2(anchor.center().x, anchor.top()),
        egui::pos2(anchor.center().x, anchor.bottom()),
        egui::pos2(anchor.left(), anchor.center().y),
        egui::pos2(anchor.right(), anchor.center().y),
    ];
    let mut best = None;
    let mut best_d = f32::INFINITY;
    for &b in &bubble_pts {
        for &a in &anchor_pts {
            let d = (b - a).length();
            if d < best_d {
                best_d = d;
                best = Some((b, a));
            }
        }
    }
    best
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
    fn tour_steps_count_is_five() {
        assert_eq!(TOUR_STEPS.len(), 5);
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

    #[test]
    fn nearest_edge_segment_returns_none_when_overlapping() {
        let a = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(100.0, 100.0));
        let b = egui::Rect::from_min_size(egui::pos2(50.0, 50.0), egui::vec2(100.0, 100.0));
        assert!(nearest_edge_segment(a, b).is_none());
    }

    #[test]
    fn nearest_edge_segment_picks_facing_edges() {
        // Bubble to the right of the anchor — connector should run from the
        // bubble's left edge to the anchor's right edge.
        let anchor = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(40.0, 40.0));
        let bubble = egui::Rect::from_min_size(egui::pos2(80.0, 0.0), egui::vec2(40.0, 40.0));
        let (from, to) = nearest_edge_segment(bubble, anchor).expect("non-overlapping");
        assert_eq!(from, egui::pos2(bubble.left(), bubble.center().y));
        assert_eq!(to, egui::pos2(anchor.right(), anchor.center().y));
    }
}
