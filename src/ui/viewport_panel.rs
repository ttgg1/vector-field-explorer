//! Session-only placement for floating vector-viewport panels.

use eframe::egui::{Align2, Area, Context, Id, InnerResponse, Order, Pos2, Rect, Ui, Vec2};

/// The last painted panel position, kept outside scene and settings records.
#[derive(Debug, Clone, Copy, Default)]
pub struct ViewportPanelState {
    pos: Option<Pos2>,
    size: Vec2,
}

impl ViewportPanelState {
    /// Restore the caller's default position on the next frame.
    pub fn reset(&mut self) {
        self.pos = None;
    }
}

/// Clamp the top-left using the panel's measured size. Oversized dimensions
/// start at the viewport origin rather than hiding the header beyond an edge.
#[doc(hidden)]
pub fn clamp_position(viewport: Rect, pos: Pos2, size: Vec2) -> Pos2 {
    pos.clamp(viewport.min, (viewport.max - size).max(viewport.min))
}

/// Show an interactive foreground panel. The caller supplies a default
/// top-left and lays out contents to fit the viewport, using scrolling where
/// necessary. Child widgets retain their ordinary egui interaction senses.
pub fn show<R>(
    ctx: &Context,
    viewport: Rect,
    id: Id,
    default_pos: Pos2,
    state: &mut ViewportPanelState,
    add_contents: impl FnOnce(&mut Ui) -> R,
) -> InnerResponse<R> {
    let pos = clamp_position(viewport, state.pos.unwrap_or(default_pos), state.size);
    let frame = Area::new(id)
        .order(Order::Foreground)
        .pivot(Align2::LEFT_TOP)
        .movable(true)
        // Ignore egui's 600×400 sizing-pass estimate when placing the panel.
        .default_size(Vec2::ZERO)
        .constrain_to(viewport)
        .current_pos(pos)
        .show(ctx, add_contents);
    let rect = frame.response.rect;
    state.pos = Some(rect.min);
    state.size = rect.size();
    if clamp_position(viewport, rect.min, state.size).distance(rect.min) > 0.5 {
        // egui constrains before laying out the contents, using the last size.
        ctx.request_repaint();
    }
    frame
}
