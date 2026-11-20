//! Issue #82 slice A: the validated viewport style record and its State seam.
//!
//! The record must reproduce today's reference constants exactly, reject
//! non-finite and out-of-range values, sanitize corrupt storage without
//! panicking, and commit atomically through `State`.

use vector_field_explorer::reference::{
    AXIS_WIDTH, AXIS_X_COLOR, AXIS_Y_COLOR, AXIS_Z_COLOR, EDGE_COLOR, EDGE_WIDTH, GRID_COLOR,
    GRID_WIDTH,
};
use vector_field_explorer::viewport_style::{MAX_LINE_WIDTH, MIN_LINE_WIDTH, ViewportStyle};
use vector_field_explorer::viewmodel::State;

#[test]
fn default_matches_existing_reference_constants() {
    let style = ViewportStyle::default();
    assert_eq!(style.grid_color, GRID_COLOR);
    assert_eq!(style.box_color, EDGE_COLOR);
    assert_eq!(style.axis_x_color, AXIS_X_COLOR);
    assert_eq!(style.axis_y_color, AXIS_Y_COLOR);
    assert_eq!(style.axis_z_color, AXIS_Z_COLOR);
    assert_eq!(style.grid_width, GRID_WIDTH);
    assert_eq!(style.box_width, EDGE_WIDTH);
    assert_eq!(style.axis_width, AXIS_WIDTH);

    // The viewport clear colour is `egui::Rgba::from(Visuals::dark().panel_fill)`
    // in `AnimationSnapshot::capture`; the default record stores its linear RGB.
    let expected = eframe::egui::Rgba::from(eframe::egui::Visuals::dark().panel_fill).to_array();
    assert_eq!(style.background, [expected[0], expected[1], expected[2]]);
    assert!(style.validate().is_ok(), "the default is valid");
}

#[test]
fn rejects_nonfinite_width() {
    let default = ViewportStyle::default();

    for width in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        let mut style = default;
        style.grid_width = width;
        assert!(
            style.validate().is_err(),
            "a non-finite grid width is rejected: {width}"
        );
        assert_eq!(
            style.sanitized().grid_width,
            default.grid_width,
            "a non-finite width resets to the default"
        );
    }

    let mut style = default;
    style.axis_width = f32::NAN;
    assert!(style.validate().is_err());
    assert_eq!(style.sanitized().axis_width, default.axis_width);

    let mut style = default;
    style.grid_color = [f32::NAN, 0.2, 0.3, 1.0];
    assert!(style.validate().is_err(), "a non-finite colour is rejected");
    assert_eq!(
        style.sanitized().grid_color,
        default.grid_color,
        "a non-finite colour resets to that field's default"
    );

    let mut style = default;
    style.background = [f32::INFINITY, 0.0, 0.0];
    assert!(
        style.validate().is_err(),
        "a non-finite background is rejected"
    );
    assert_eq!(style.sanitized().background, default.background);
}

#[test]
fn rejects_out_of_range_color_and_width() {
    let default = ViewportStyle::default();

    let mut style = default;
    style.grid_color = [1.5, 0.2, 0.3, 1.0];
    assert!(style.validate().is_err(), "a colour above one is rejected");
    assert_eq!(style.sanitized().grid_color[0], 1.0, "it clamps to one");

    let mut style = default;
    style.axis_z_color = [-0.1, 0.2, 0.3, 1.0];
    assert!(style.validate().is_err(), "a negative colour is rejected");
    assert_eq!(style.sanitized().axis_z_color[0], 0.0, "it clamps to zero");

    let mut style = default;
    style.box_color = [0.2, 0.3, 0.4, 2.0];
    assert!(style.validate().is_err(), "alpha above one is rejected");
    assert_eq!(style.sanitized().box_color[3], 1.0);

    let mut style = default;
    style.grid_width = MAX_LINE_WIDTH + 0.01;
    assert!(style.validate().is_err(), "a width above the max is rejected");
    assert_eq!(style.sanitized().grid_width, MAX_LINE_WIDTH);

    let mut style = default;
    style.axis_width = MIN_LINE_WIDTH / 2.0;
    assert!(style.validate().is_err(), "a width below the min is rejected");
    assert_eq!(style.sanitized().axis_width, MIN_LINE_WIDTH);

    // The bounds are independent: no ordering between grid, box and axis is
    // invented, so a thicker grid than box is still valid.
    let mut style = default;
    style.grid_width = 0.04;
    style.box_width = 0.001;
    style.axis_width = 0.002;
    assert!(
        style.validate().is_ok(),
        "widths are independently bounded, not ordered"
    );
}

#[test]
fn named_looks_are_valid() {
    let dark = ViewportStyle::dark_laboratory();
    let white = ViewportStyle::white_paper();
    let contrast = ViewportStyle::high_contrast();

    for (name, style) in [
        ("dark_laboratory", dark),
        ("white_paper", white),
        ("high_contrast", contrast),
    ] {
        assert!(style.validate().is_ok(), "{name} validates");
    }
    assert_eq!(
        dark,
        ViewportStyle::default(),
        "dark_laboratory is the shipped default"
    );
    assert_ne!(dark, white, "dark and white-paper differ");
    assert_ne!(dark, contrast, "dark and high-contrast differ");
    assert_ne!(white, contrast, "white-paper and high-contrast differ");
}

#[test]
fn sanitized_clamps_finite_and_resets_nonfinite() {
    let style = ViewportStyle {
        grid_color: [1.2, -0.3, 0.5, 0.75],
        background: [2.0, 0.5, -1.0],
        box_width: 0.04,
        axis_width: f32::NAN,
        ..ViewportStyle::default()
    };

    let sanitized = style.sanitized();
    assert_eq!(sanitized.grid_color, [1.0, 0.0, 0.5, 0.75]);
    assert_eq!(sanitized.background, [1.0, 0.5, 0.0]);
    assert_eq!(sanitized.box_width, 0.04);
    assert_eq!(
        sanitized.axis_width,
        ViewportStyle::default().axis_width,
        "a non-finite width resets to the default"
    );
    assert!(sanitized.validate().is_ok(), "sanitized output is valid");
}

#[test]
fn commit_validates_and_changes_atomically() {
    let mut state = State::new(false);
    state.dirty = false;
    let original = state.viewport_style;

    let mut invalid = original;
    invalid.grid_width = f32::NAN;
    let error = state
        .commit_viewport_style(invalid)
        .expect_err("a non-finite width is rejected");
    assert!(!error.is_empty());
    assert_eq!(
        state.viewport_style, original,
        "a rejected commit changes nothing"
    );
    assert!(!state.dirty, "a rejected commit does not dirty the scene");

    let next = ViewportStyle::white_paper();
    state
        .commit_viewport_style(next)
        .expect("a valid style commits");
    assert_eq!(state.viewport_style, next);
    assert!(state.dirty, "a real change dirties the scene");
}

#[test]
fn noop_commit_does_not_dirty_and_reset_dirties_only_on_change() {
    let mut state = State::new(false);
    state.dirty = false;

    state
        .commit_viewport_style(ViewportStyle::default())
        .expect("the default is valid");
    assert!(!state.dirty, "a validated no-op does not dirty");

    let white = ViewportStyle::white_paper();
    state
        .commit_viewport_style(white)
        .expect("a valid style commits");
    assert!(state.dirty);

    state.dirty = false;
    state
        .commit_viewport_style(white)
        .expect("the same style is a validated no-op");
    assert!(!state.dirty, "re-committing the same style does not dirty");

    state.reset_viewport_style();
    assert_eq!(state.viewport_style, ViewportStyle::default());
    assert!(state.dirty, "a real reset dirties");

    state.dirty = false;
    state.reset_viewport_style();
    assert!(!state.dirty, "resetting an already-default style does not dirty");
}
