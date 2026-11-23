use eframe::egui::{Rect, pos2, vec2};
use vector_field_explorer::ui::viewport_panel::clamp_position;

#[test]
fn clamps_panel_origin_without_hiding_the_header() {
    let viewport = Rect::from_min_max(pos2(20.0, 40.0), pos2(420.0, 280.0));
    let size = vec2(100.0, 60.0);
    let default = pos2(308.0, 52.0);
    assert_eq!(clamp_position(viewport, default, size), default);
    let moved = pos2(120.0, 90.0);
    assert_eq!(clamp_position(viewport, moved, size), moved);
    assert_eq!(
        clamp_position(viewport, pos2(500.0, -30.0), size),
        pos2(320.0, 40.0)
    );
    assert_eq!(
        clamp_position(viewport, moved, vec2(500.0, 300.0)),
        viewport.min
    );
}
