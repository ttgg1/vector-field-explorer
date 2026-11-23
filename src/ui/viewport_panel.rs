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

/// Pure slide-in position for a panel: at `progress = 0` the panel is `distance`
/// points below its resting spot, at `progress = 1` exactly at rest. Only the
/// painted position changes; callers keep the unshifted rect for layout and
/// interaction so hit targets and accessible bounds never move.
#[doc(hidden)]
pub fn slide_pos(rest: Pos2, progress: f32, distance: f32) -> Pos2 {
    let p = progress.clamp(0.0, 1.0);
    rest - Vec2::new(0.0, distance * (1.0 - p))
}

/// The painted rectangle for a slide-in; same size as `rest`, translated only
/// by the slide offset. Interaction continues to use the original rect.
#[doc(hidden)]
pub fn slide_rect(rest: Rect, progress: f32, distance: f32) -> Rect {
    rest.translate(slide_pos(rest.min, progress, distance) - rest.min)
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
    show_slide(
        ctx,
        viewport,
        id,
        default_pos,
        state,
        Vec2::ZERO,
        add_contents,
    )
}

/// As [`show`], but paints the panel at `rest + slide` while keeping the stored
/// placement at `rest`. The slide is a purely visual offset: hit targets, the
/// stored position and the panel size are the same as at rest, so accessibility
/// bounds and layout geometry never move. Callers request a repaint only while
/// their transition is active.
pub fn show_slide<R>(
    ctx: &Context,
    viewport: Rect,
    id: Id,
    default_pos: Pos2,
    state: &mut ViewportPanelState,
    slide: Vec2,
    add_contents: impl FnOnce(&mut Ui) -> R,
) -> InnerResponse<R> {
    let rest = clamp_position(viewport, state.pos.unwrap_or(default_pos), state.size);
    let frame = Area::new(id)
        .order(Order::Foreground)
        .pivot(Align2::LEFT_TOP)
        .movable(true)
        // Ignore egui's 600×400 sizing-pass estimate when placing the panel.
        .default_size(Vec2::ZERO)
        .constrain_to(viewport)
        .current_pos(rest + slide)
        .show(ctx, add_contents);
    let rect = frame.response.rect;
    // Persist the resting position, not the transient slid one, so dragging
    // still writes the user's placement while the visual slide settles.
    //
    // `constrain_to` clamps the *painted* position (`rest + slide`) to the
    // viewport. At a bottom edge that shortens the effective slide; subtracting
    // the unchanged `slide` would then shift the saved rest upward on reopen.
    // Recover the slide egui actually applied and subtract that instead.
    //
    // egui's `Area::begin` constrains using the size measured on the *previous*
    // frame, while `Area::end` replaces it with the freshly measured content
    // size before returning `rect`. Use the entry size (`state.size`, still
    // untouched here) for the inverse; using `rect.size()` would invert the
    // wrong constraint whenever the contents reflow mid-slide.
    let applied_pos = clamp_position(viewport, rest + slide, state.size);
    let applied_slide = applied_pos - rest;
    state.pos = Some(rect.min - applied_slide);
    state.size = rect.size();
    let rest_now = clamp_position(viewport, state.pos.unwrap_or(default_pos), state.size);
    if rest_now.distance(state.pos.unwrap_or(default_pos)) > 0.5 {
        // egui constrains before laying out the contents, using the last size.
        ctx.request_repaint();
    }
    frame
}
