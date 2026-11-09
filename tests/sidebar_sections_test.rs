// Moved out of `src/ui/sidebar.rs`'s inline module (ticket-14 review I-4).

use glam::Vec3;
use vector_field_explorer::colormap::Colormap;
use vector_field_explorer::ui::sidebar::{Controls, SECTIONS, Section};
use vector_field_explorer::viewmodel::ArrowLengthMode;

#[test]
fn sections_are_ordered_display_features_coloring_domain() {
    let titles: Vec<&str> = SECTIONS.iter().map(|section| section.title()).collect();
    assert_eq!(titles, ["Display", "Features", "Coloring", "Domain"]);
}

#[test]
fn display_and_features_open_first_the_others_stay_collapsed() {
    assert!(Section::Display.default_open(), "Display is the primary knob group");
    assert!(Section::Features.default_open(), "Features is the second everyday group");
    assert!(!Section::Coloring.default_open(), "Coloring starts collapsed");
    assert!(!Section::Domain.default_open(), "Domain starts collapsed");
}

#[test]
fn each_reset_restores_only_its_own_section() {
    let defaults = Controls::new();
    let mut controls = Controls::new();
    controls.density = 77;
    controls.arrow_scale = 3.5;
    controls.arrow_length_mode = ArrowLengthMode::Magnitude;
    controls.magnitude_scale = 4.0;
    controls.colormap = Colormap::Turbo;
    controls.auto_fit = false;
    controls.color_min = -4.0;
    controls.color_max = 9.0;
    controls.streamlines = true;
    controls.particles = true;
    controls.domain_min = Vec3::splat(-5.0);
    controls.domain_max = Vec3::splat(5.0);

    controls.reset_display();
    assert_eq!(controls.density, defaults.density);
    assert_eq!(controls.arrow_scale, defaults.arrow_scale);
    assert_eq!(controls.arrow_length_mode, defaults.arrow_length_mode);
    assert_eq!(controls.magnitude_scale, defaults.magnitude_scale);
    assert_eq!(controls.colormap, Colormap::Turbo, "Display reset leaves Coloring alone");
    assert_eq!(controls.domain_max, Vec3::splat(5.0), "Display reset leaves Domain alone");

    controls.reset_features();
    assert!(!controls.streamlines);
    assert!(!controls.particles);
    assert_eq!(controls.colormap, Colormap::Turbo, "Features reset leaves Coloring alone");

    controls.reset_coloring();
    assert_eq!(controls.colormap, defaults.colormap);
    assert_eq!(controls.auto_fit, defaults.auto_fit);
    assert_eq!(controls.color_min, defaults.color_min);
    assert_eq!(controls.color_max, defaults.color_max);
    assert_eq!(controls.domain_min, Vec3::splat(-5.0), "Coloring reset leaves Domain alone");

    controls.reset_domain();
    assert_eq!(controls.domain_min, defaults.domain_min);
    assert_eq!(controls.domain_max, defaults.domain_max);
}
