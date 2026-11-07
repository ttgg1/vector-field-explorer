// The sidebar owns a plain mirror of the scene knobs (`Controls`): pure data
// that the widgets edit between frames and that `show` hands to the viewmodel
// setters once per frame. The defaults must agree with the seam's or the first
// sidebar frame would change settings the user never touched (and the status
// bar would count a scene the user never asked for).

use vector_field_explorer::ui::sidebar::Controls;
use vector_field_explorer::viewmodel::{State, DEFAULT_DENSITY_2D};

#[test]
fn sidebar_defaults_mirror_the_seam_defaults() {
    let c = Controls::new();
    let s = State::new(false);
    assert_eq!(c.density, s.density, "density starts at the seam default");
    assert_eq!(c.density, DEFAULT_DENSITY_2D, "…and that default is pinned");
    assert_eq!(c.arrow_scale, s.arrow_scale, "arrow scale agrees");
    assert_eq!(c.normalize, s.normalize, "normalize agrees");
    assert_eq!(c.colormap, s.colormap, "colormap agrees");
    assert_eq!(c.auto_fit, s.auto_fit, "auto-fit agrees");
    assert_eq!(c.color_min, s.color_min, "color min agrees");
    assert_eq!(c.color_max, s.color_max, "color max agrees");
    assert_eq!(c.streamlines, s.streamlines, "streamlines start off");
    assert_eq!(c.particles, s.particles, "particles start off");
    assert_eq!(c.speed, s.speed, "speed agrees");
    assert_eq!(c.domain_min, s.domain.min, "domain min agrees");
    assert_eq!(c.domain_max, s.domain.max, "domain max agrees");
}

#[test]
fn synced_controls_never_redirty_the_scene() {
    // Mirrors one sidebar frame with untouched widgets: every setter must
    // report "nothing changed" so the snapshot and the status-bar count stay.
    let c = Controls::new();
    let mut s = State::new(false);
    assert!(s.snapshot_if_dirty().is_some(), "the fresh scene builds once");
    assert!(!s.set_density(c.density), "synced density does not re-dirty");
    assert!(!s.set_arrow_scale(c.arrow_scale));
    assert!(!s.set_normalize(c.normalize));
    assert!(!s.set_colormap(c.colormap));
    assert!(!s.set_auto_fit(c.auto_fit));
    assert!(!s.set_color_range(c.color_min, c.color_max));
    assert!(!s.set_streamlines(c.streamlines));
    assert!(!s.set_particles(c.particles));
    assert!(!s.set_speed(c.speed));
    assert!(!s.set_domain(c.domain_min, c.domain_max), "synced extrema are a no-op");
    assert!(s.snapshot_if_dirty().is_none(), "an untouched frame rebuilds nothing");
}

#[test]
fn reversed_drag_extrema_swap_through_the_seam() {
    // The domain row's drag values may end up reversed (min dragged past max);
    // show() hands them to set_domain as-is and the seam swaps them.
    let mut c = Controls::new();
    let mut s = State::new(false);
    assert!(s.snapshot_if_dirty().is_some());
    c.domain_min.x = 2.0;
    c.domain_max.x = -2.0;
    assert!(s.set_domain(c.domain_min, c.domain_max), "the reversed pair changes the domain");
    assert_eq!(s.domain.min.x, -2.0, "the seam enforces the min<max rule");
    assert_eq!(s.domain.max.x, 2.0, "…by swapping, not by rejecting");
    let scene = s.snapshot_if_dirty();
    assert!(scene.is_some(), "the change asks for a rebuild");
}
