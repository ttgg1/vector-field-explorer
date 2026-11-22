//! Guided-tour overlay, anchoring and advance helper (#99 slice B).
//!
//! The pure model lives in [`crate::tour`]. This module owns the live-frame
//! half: it records the real panel and viewport rectangles the app painted,
//! resolves a stop's semantic target against them, draws the anchored card and
//! the highlight, and evaluates the `Changed` advance rule.
//!
//! Nothing here mutates the document, history, renderer or persistence. The
//! recorded rectangles are transient frame data, cleared and rebuilt by the app
//! every frame; an anchor that is not visible this frame simply has no
//! highlight instead of a guessed position.

use eframe::egui;

use crate::tour::{
    AdvanceRule, ControlId, PanelId, Region, TOUR, TourAnchor, TourProgress, TourTarget, resolve,
};

/// The card width used for placement and painting, before compact clamping.
pub const CARD_WIDTH: f32 = 340.0;
/// Gap between the highlighted target and the card.
const CARD_GAP: f32 = 12.0;
/// Keep the card off the physical screen edges.
const SCREEN_MARGIN: f32 = 8.0;
/// Estimated card height for placement. The painted card may be shorter; the
/// reserve is deliberately generous so placement never lands on the target.
const CARD_RESERVE_HEIGHT: f32 = 230.0;

/// Pure `AdvanceRule` evaluation. `captured` is the value snapshotted when the
/// stop became current; `current` is this frame's value (both `None` when the
/// control is unavailable).
pub fn advance_ready(rule: AdvanceRule, captured: Option<&str>, current: Option<&str>) -> bool {
    match rule {
        AdvanceRule::Next => true,
        AdvanceRule::Changed(_) => match (captured, current) {
            (Some(captured), Some(current)) => captured != current,
            _ => false,
        },
    }
}

/// The panel whose live rectangle best represents a control. Controls do not
/// expose independent stable ids through the app's panel code, so the
/// containing panel is the recorded real rectangle the tour highlights; the
/// card names the control itself.
pub const fn control_panel(target: TourTarget) -> Option<PanelId> {
    match resolve(target) {
        TourAnchor::Control { panel, .. } | TourAnchor::Panel(panel) => Some(panel),
        TourAnchor::Viewport(_) | TourAnchor::Maximize => None,
    }
}

/// Rectangles recorded from real widgets during one frame.
#[derive(Debug, Clone, Default)]
pub struct TourFrameRects {
    panels: Vec<(PanelId, egui::Rect)>,
    controls: Vec<(ControlId, egui::Rect)>,
    viewport: Option<egui::Rect>,
}

impl TourFrameRects {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn clear(&mut self) {
        self.panels.clear();
        self.controls.clear();
        self.viewport = None;
    }

    /// Record the response rect of a panel surface. A later record in the same
    /// frame replaces an earlier one.
    pub fn record_panel(&mut self, panel: PanelId, rect: egui::Rect) {
        if let Some(entry) = self.panels.iter_mut().find(|(known, _)| *known == panel) {
            entry.1 = rect;
        } else {
            self.panels.push((panel, rect));
        }
    }

    /// Record the response rect of one live control. Controls that are not
    /// painted this frame stay absent, never guessed.
    pub fn record_control(&mut self, control: ControlId, rect: egui::Rect) {
        if let Some(entry) = self
            .controls
            .iter_mut()
            .find(|(known, _)| *known == control)
        {
            entry.1 = rect;
        } else {
            self.controls.push((control, rect));
        }
    }

    pub fn record_viewport(&mut self, rect: egui::Rect) {
        self.viewport = Some(rect);
    }

    pub fn panel_rect(&self, panel: PanelId) -> Option<egui::Rect> {
        self.panels
            .iter()
            .find_map(|(known, rect)| (*known == panel).then_some(*rect))
    }

    pub fn control_rect(&self, control: ControlId) -> Option<egui::Rect> {
        self.controls
            .iter()
            .find_map(|(known, rect)| (*known == control).then_some(*rect))
    }

    /// Resolve a stop target to the real rectangle recorded this frame. A
    /// control with its own live rect uses it; controls without one keep the
    /// historical panel fallback. [`ControlId::requires_live_rect`] controls
    /// yield no anchor when absent rather than highlighting the whole panel.
    pub fn anchor_rect(&self, target: TourTarget) -> Option<egui::Rect> {
        match resolve(target) {
            TourAnchor::Control { panel, control } => {
                if let Some(rect) = self.control_rect(control) {
                    Some(rect)
                } else if control.requires_live_rect() {
                    None
                } else {
                    self.panel_rect(panel)
                }
            }
            TourAnchor::Panel(panel) => self.panel_rect(panel),
            TourAnchor::Viewport(region) => {
                let viewport = self.viewport?;
                Some(region_rect(viewport, region))
            }
            TourAnchor::Maximize => self.viewport,
        }
    }
}

/// Map a semantic viewport region onto a sub-rectangle of the real viewport.
/// The centre uses the middle 40% so the card always has room outside the
/// highlighted region.
fn region_rect(viewport: egui::Rect, region: Region) -> egui::Rect {
    let half = viewport.size() * 0.5;
    match region {
        Region::Center => egui::Rect::from_center_size(viewport.center(), viewport.size() * 0.4),
        Region::TopLeft => egui::Rect::from_min_size(viewport.min, half),
        Region::TopRight => {
            egui::Rect::from_min_size(egui::pos2(viewport.center().x, viewport.top()), half)
        }
        Region::BottomLeft => {
            egui::Rect::from_min_size(egui::pos2(viewport.left(), viewport.center().y), half)
        }
        Region::BottomRight => egui::Rect::from_min_size(viewport.center(), half),
    }
}

/// Place a card of `size` near `target` without covering it, kept inside
/// `screen`. With no target (an unresolved anchor) the card is centred at the
/// top of the screen.
pub fn place_card(target: Option<egui::Rect>, screen: egui::Rect, size: egui::Vec2) -> egui::Rect {
    let width = size.x.min((screen.width() - 2.0 * SCREEN_MARGIN).max(1.0));
    let height = size.y.min((screen.height() - 2.0 * SCREEN_MARGIN).max(1.0));
    let size = egui::vec2(width, height);
    let Some(target) = target else {
        let x = (screen.center().x - width * 0.5).clamp(
            screen.left() + SCREEN_MARGIN,
            (screen.right() - width - SCREEN_MARGIN).max(screen.left() + SCREEN_MARGIN),
        );
        let y = (screen.top() + 16.0).clamp(
            screen.top() + SCREEN_MARGIN,
            (screen.bottom() - height - SCREEN_MARGIN).max(screen.top() + SCREEN_MARGIN),
        );
        return egui::Rect::from_min_size(egui::pos2(x, y), size);
    };

    let clamp_x = |x: f32| {
        x.clamp(
            screen.left() + SCREEN_MARGIN,
            (screen.right() - width - SCREEN_MARGIN).max(screen.left() + SCREEN_MARGIN),
        )
    };
    let clamp_y = |y: f32| {
        y.clamp(
            screen.top() + SCREEN_MARGIN,
            (screen.bottom() - height - SCREEN_MARGIN).max(screen.top() + SCREEN_MARGIN),
        )
    };
    let candidates = [
        // Right, then left, then below, then above. Each is clamped into the
        // screen before it is tested, so a tiny viewport still gets a card.
        egui::pos2(
            clamp_x(target.right() + CARD_GAP),
            clamp_y(target.center().y - height * 0.5),
        ),
        egui::pos2(
            clamp_x(target.left() - CARD_GAP - width),
            clamp_y(target.center().y - height * 0.5),
        ),
        egui::pos2(
            clamp_x(target.center().x - width * 0.5),
            clamp_y(target.bottom() + CARD_GAP),
        ),
        egui::pos2(
            clamp_x(target.center().x - width * 0.5),
            clamp_y(target.top() - CARD_GAP - height),
        ),
    ];
    for min in candidates {
        let rect = egui::Rect::from_min_size(min, size);
        if screen.contains_rect(rect) && !rect.intersects(target) {
            return rect;
        }
    }

    // No side fits (for example a nearly full-screen target). Put the card in
    // the screen corner farthest from the target centre so it covers as little
    // of the highlight as possible; the card still never leaves the screen.
    let far_left = target.center().x > screen.center().x;
    let far_top = target.center().y > screen.center().y;
    let x = if far_left {
        screen.left() + SCREEN_MARGIN
    } else {
        screen.right() - width - SCREEN_MARGIN
    };
    let y = if far_top {
        screen.top() + SCREEN_MARGIN
    } else {
        screen.bottom() - height - SCREEN_MARGIN
    };
    egui::Rect::from_min_size(egui::pos2(clamp_x(x), clamp_y(y)), size)
}

/// What the user asked for this frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TourAction {
    None,
    Back,
    Next,
    Dismiss,
}

/// One overlay pass's painted geometry and requested action.
#[derive(Debug, Clone, Copy)]
pub struct TourOutcome {
    pub action: TourAction,
    /// The card rectangle actually painted, for compact-layout tests.
    pub card_rect: Option<egui::Rect>,
    /// The real target rectangle, or `None` when the anchor is unresolved this
    /// frame.
    pub target_rect: Option<egui::Rect>,
}

impl Default for TourOutcome {
    fn default() -> Self {
        Self {
            action: TourAction::None,
            card_rect: None,
            target_rect: None,
        }
    }
}

/// Draw the tour card and highlight for the current stop. The caller applies
/// the returned action after the overlay pass.
pub fn show(
    ctx: &egui::Context,
    rects: &TourFrameRects,
    progress: &TourProgress,
    next_ready: bool,
) -> TourOutcome {
    let Some(stop) = progress.current() else {
        return TourOutcome::default();
    };
    let screen = ctx.content_rect();
    let target = rects.anchor_rect(stop.target);
    let card_size = egui::vec2(
        CARD_WIDTH.min((screen.width() - 2.0 * SCREEN_MARGIN).max(1.0)),
        CARD_RESERVE_HEIGHT,
    );
    let card_rect = place_card(target, screen, card_size);

    if let Some(rect) = target {
        paint_highlight(ctx, rect);
    }

    let mut action = TourAction::None;
    let mut painted_rect = None;
    egui::Area::new(egui::Id::unique("guided_tour_card"))
        .order(egui::Order::Foreground)
        .fixed_pos(card_rect.min)
        .interactable(true)
        .show(ctx, |ui| {
            // A Modal stack kind gives the card an accessible dialog role
            // without making the Area modal: the viewport keeps its pointer
            // input while the tour is up.
            let response = ui.scope_builder(
                egui::UiBuilder::new().ui_stack_info(egui::UiStackInfo::new(egui::UiKind::Modal)),
                |ui| {
                    egui::Frame::new()
                        .fill(ui.visuals().window_fill)
                        .stroke(egui::Stroke::new(1.0, ui.visuals().window_stroke.color))
                        .corner_radius(egui::CornerRadius::same(8))
                        .inner_margin(egui::Margin::same(12))
                        .show(ui, |ui| {
                            ui.set_max_width((card_size.x - 24.0).max(1.0));
                            ui.label(egui::RichText::new(stop.title).strong().size(16.0));
                            ui.add_space(4.0);
                            ui.label(stop.body);
                            ui.add_space(6.0);
                            ui.weak(format!("Step {} of {}", progress.index() + 1, TOUR.len()));
                            ui.horizontal_wrapped(|ui| {
                                if ui
                                    .add_enabled(progress.index() > 0, egui::Button::new("Back"))
                                    .clicked()
                                {
                                    action = TourAction::Back;
                                }
                                let next = ui.add_enabled(next_ready, egui::Button::new("Next"));
                                if next.clicked() {
                                    action = TourAction::Next;
                                }
                                if !next_ready {
                                    next.on_hover_text(match stop.advance {
                                        AdvanceRule::Changed(key) => {
                                            format!("Change {key} to continue")
                                        }
                                        AdvanceRule::Next => "Continue".to_owned(),
                                    });
                                }
                                if ui.button("Skip tour").clicked() {
                                    action = TourAction::Dismiss;
                                }
                            });
                            ui.weak("Esc dismisses · ← back · → next");
                        });
                },
            );
            painted_rect = Some(response.response.rect);
        });

    TourOutcome {
        action,
        card_rect: painted_rect.or(Some(card_rect)),
        target_rect: target,
    }
}

/// A two-line highlight: a dark outer stroke and a light inner stroke. The
/// double edge carries meaning without relying on colour alone.
fn paint_highlight(ctx: &egui::Context, rect: egui::Rect) {
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Middle,
        egui::Id::unique("guided_tour_highlight"),
    ));
    painter.rect_stroke(
        rect.expand(3.0),
        egui::CornerRadius::same(6),
        egui::Stroke::new(3.0, egui::Color32::from_black_alpha(200)),
        egui::StrokeKind::Outside,
    );
    painter.rect_stroke(
        rect.expand(1.0),
        egui::CornerRadius::same(6),
        egui::Stroke::new(2.0, egui::Color32::from_rgb(255, 214, 10)),
        egui::StrokeKind::Outside,
    );
    // A text label beside the highlight keeps the meaning available to anyone
    // who cannot distinguish the accent colour.
    let label_pos = egui::pos2(rect.left(), (rect.top() - 18.0).max(0.0));
    painter.text(
        label_pos,
        egui::Align2::LEFT_BOTTOM,
        "Tour target",
        egui::FontId::proportional(12.0),
        egui::Color32::from_rgb(255, 214, 10),
    );
}
