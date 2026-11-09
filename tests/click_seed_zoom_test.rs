// Review M-4: the 2D click seed must honour the ortho zoom. A click at NDC 0.5
// with 2x zoom lands on the same world point as a click at NDC 0.25 unzoomed.

use glam::Vec2;
use vector_field_explorer::camera::{OrbitCamera, ViewCamera};
use vector_field_explorer::domain::Domain;
use vector_field_explorer::streamlines::click_seed;

#[test]
fn click_seed_honours_the_2d_ortho_zoom() {
    let domain = Domain::default();
    let aspect = 1.4;
    let plane_size = 2.7;
    let mut zoomed = OrbitCamera::default();
    zoomed.zoom_2d(2.0);

    let zoomed_seed = click_seed(ViewCamera::Orbit(&zoomed), &domain, aspect, Vec2::new(0.5, 0.0), Some(plane_size))
        .expect("a zoomed 2D click describes a seed");
    let plain_seed = click_seed(
        ViewCamera::Orbit(&OrbitCamera::default()),
        &domain,
        aspect,
        Vec2::new(0.25, 0.0),
        Some(plane_size),
    )
    .expect("an unzoomed 2D click describes a seed");

    assert!(
        (zoomed_seed - plain_seed).length() < 1e-4,
        "2x zoom at NDC 0.5 is the same world point as NDC 0.25 at 1x: {zoomed_seed:?} vs {plain_seed:?}",
    );
    assert!(
        (zoomed_seed.z - (domain.min.z + domain.max.z) * 0.5).abs() < 1e-4,
        "the seed stays on the domain's z midpoint",
    );
}
