// Moved out of `src/ui/top_bar.rs`'s inline module (ticket-14 review I-4).

use vector_field_explorer::ui::top_bar::wheel_zoom_factor;

#[test]
fn no_scroll_is_no_zoom() {
    assert_eq!(wheel_zoom_factor(0.0), 1.0);
}

#[test]
fn scrolling_up_zooms_in_and_down_zooms_out() {
    let up = wheel_zoom_factor(50.0);
    let down = wheel_zoom_factor(-50.0);
    assert!(up > 1.0, "scroll up comes closer: {up}");
    assert!(down < 1.0, "scroll down pulls back: {down}");
    assert!((up * down - 1.0).abs() < 1e-6, "the directions are reciprocal");
    assert!(up > 1.05, "a wheel notch is a visible step, not a nudge: {up}");
}

#[test]
fn nonsense_scroll_leaves_the_zoom_alone() {
    for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        assert_eq!(wheel_zoom_factor(bad), 1.0, "{bad} must not move the view");
    }
    assert_eq!(wheel_zoom_factor(1e30), 1.0, "an overflowing factor falls back to 1");
}
