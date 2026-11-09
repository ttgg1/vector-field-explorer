// Moved out of `src/ui/top_bar.rs`'s inline module (ticket-14 review I-4).

use eframe::egui;
use vector_field_explorer::ui::top_bar::{dismisses_appearance_popover, wheel_zoom_factor};

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

#[test]
fn clicking_the_popover_or_its_open_picker_keeps_the_popover() {
    // Ticket 41: the nested color picker is a separate foreground area, often
    // drawn beyond the popover rect. Its clicks must not count as outside.
    let popover_rect = egui::Rect::from_min_size(egui::pos2(100.0, 20.0), egui::vec2(260.0, 160.0));
    let popover_layer =
        egui::LayerId::new(egui::Order::Foreground, egui::Id::unique("appearance_popover"));
    let picker_layer = egui::LayerId::new(egui::Order::Foreground, egui::Id::unique("popup"));
    let inside = egui::pos2(120.0, 80.0);
    let picker = egui::pos2(320.0, 300.0);
    let background = egui::pos2(900.0, 400.0);
    let background_layer = egui::LayerId::new(egui::Order::Background, egui::Id::unique("background"));

    assert!(
        !dismisses_appearance_popover(
            true,
            Some(inside),
            popover_rect,
            Some(popover_layer),
            Some(picker_layer)
        ),
        "a click inside the popover is not outside"
    );
    assert!(
        !dismisses_appearance_popover(
            true,
            Some(picker),
            popover_rect,
            Some(picker_layer),
            Some(picker_layer)
        ),
        "a click on the open picker belongs to the picker"
    );
    assert!(
        dismisses_appearance_popover(
            true,
            Some(background),
            popover_rect,
            Some(background_layer),
            Some(picker_layer)
        ),
        "a click on the app behind the popover still dismisses"
    );
    assert!(
        !dismisses_appearance_popover(
            false,
            Some(background),
            popover_rect,
            Some(background_layer),
            None
        ),
        "a frame with no click never dismisses"
    );
}
