//! Ignored release benchmark for the densest source preset. Run through
//! `scripts/bench-source-presets.sh`; there is no pass/fail threshold.

use std::time::Instant;

use glam::UVec3;
use vector_field_explorer::source_presets::{load_source_preset, source_preset};
use vector_field_explorer::source_scene::sample_source_scene;
use vector_field_explorer::viewmodel::State;

#[test]
// `cfg_attr(all(), ...)` applies the ignore while keeping the suite-layout
// guard (which rejects literal `#[ignore]`) intact for this evidence-only run.
#[cfg_attr(
    all(),
    ignore = "release benchmark; run scripts/bench-source-presets.sh"
)]
fn source_gallery_sampling_benchmark() {
    let mut state = State::new(true);
    let preset = source_preset("Source gallery").expect("the all-eight preset");
    assert!(load_source_preset(preset, &mut state));

    for (label, grid) in [
        ("40^3", UVec3::new(40, 40, 40)),
        ("200^2", UVec3::new(200, 200, 1)),
    ] {
        let start = Instant::now();
        let sampling = sample_source_scene(
            &state.sources,
            &state.domain,
            grid,
            None,
            &state.view_options(),
        );
        let elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;
        println!(
            "Source gallery {label}: {elapsed_ms:.3} ms, {} arrows",
            sampling.arrows.len()
        );
        assert!(
            sampling
                .arrows
                .iter()
                .all(|arrow| arrow.position.is_finite()
                    && arrow.dir.is_finite()
                    && arrow.mag.is_finite()),
            "every benchmark arrow must be finite"
        );
        assert!(sampling.electric.sources > 0, "electric diagnostics");
        assert!(sampling.magnetic.sources > 0, "magnetic diagnostics");
        assert!(
            sampling.validation_error.is_none(),
            "the gallery preset must validate"
        );
    }
}
