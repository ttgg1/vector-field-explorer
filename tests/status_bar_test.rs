//! Status bar coverage: the FPS meter, the compact stats formatter, the
//! width-driven mask contract that ticket 53 consumes, and the transient hint's
//! frame-time expiry.

use eframe::egui;
use vector_field_explorer::app::StatusBarState;
use vector_field_explorer::ui::status_bar::{
    FpsMeter, StatsMask, advance_hint, format_status, visible_stats,
};

fn all_stats() -> StatsMask {
    StatsMask {
        sim_time: true,
        density: true,
        trails: true,
        particles: true,
    }
}

#[test]
fn fps_meter_seeds_from_the_first_valid_frame() {
    let mut meter = FpsMeter::default();
    assert_eq!(meter.fps(), 0.0);
    assert_eq!(meter.frame_ms(), 0.0);

    meter.update(1.0 / 60.0);
    assert!((meter.fps() - 60.0).abs() < 1e-3);
    assert!((meter.frame_ms() - 1000.0 / 60.0).abs() < 1e-2);
}

#[test]
fn fps_meter_smooths_later_samples() {
    let mut meter = FpsMeter::default();
    meter.update(1.0 / 60.0);
    meter.update(1.0 / 30.0);
    // 0.9 * 60 + 0.1 * 30
    assert!((meter.fps() - 57.0).abs() < 1e-3);
    assert!((meter.frame_ms() - 1000.0 / 57.0).abs() < 1e-2);
}

#[test]
fn fps_meter_takes_exactly_one_sample_per_update() {
    let mut meter = FpsMeter::default();
    meter.update(0.02); // 50 fps seed
    assert_eq!(meter.fps(), 50.0);

    // One 100 fps sample must move the EMA by exactly 10 points once:
    // 0.9 * 50 + 0.1 * 100 = 55. A second application in the same call would
    // land at 59.5, so this pins one sample per `update`.
    meter.update(0.01);
    assert!((meter.fps() - 55.0).abs() < 1e-3);
}

#[test]
fn fps_meter_ignores_zero_negative_and_non_finite_dt() {
    let mut fresh = FpsMeter::default();
    for dt in [0.0, -0.5, f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        fresh.update(dt);
    }
    assert_eq!(fresh.fps(), 0.0, "no valid sample yet");

    let mut meter = FpsMeter::default();
    meter.update(1.0 / 60.0);
    let before = meter.fps();
    for dt in [0.0, -0.5, f32::NAN, f32::INFINITY] {
        meter.update(dt);
    }
    assert_eq!(meter.fps(), before, "invalid frames keep the last reading");
}

#[test]
fn format_status_prints_zero_counts_with_plurals() {
    let status = StatusBarState::default();
    let text = format_status(&status, all_stats());
    assert_eq!(
        text,
        "0 fps · 0.0 ms | 0 arrows · 0 streamlines · 0 particles · 0 trail segs | n²=0"
    );
}

#[test]
fn format_status_uses_singular_nouns_for_one() {
    let status = StatusBarState {
        arrow_count: 1,
        streamline_count: 1,
        particle_count: 1,
        trail_count: 1,
        ..StatusBarState::default()
    };
    let text = format_status(&status, all_stats());
    assert!(
        text.contains("| 1 arrow · 1 streamline · 1 particle · 1 trail seg |"),
        "singular counts read naturally: {text}"
    );
}

#[test]
fn format_status_compacts_thousands_and_millions() {
    let status = StatusBarState {
        arrow_count: 1_200,
        streamline_count: 12_400,
        particle_count: 1_200_000,
        trail_count: 999,
        ..StatusBarState::default()
    };
    let text = format_status(&status, all_stats());
    assert!(text.contains("1.2k arrows"), "1.2k arrows: {text}");
    assert!(text.contains("12.4k streamlines"), "12.4k streamlines: {text}");
    assert!(text.contains("1.2M particles"), "1.2M particles: {text}");
    assert!(text.contains("999 trail segs"), "small counts stay exact: {text}");
}

#[test]
fn format_status_labels_density_for_the_sampling_dimension() {
    let flat = StatusBarState {
        density: 32,
        is_3d: false,
        ..StatusBarState::default()
    };
    assert!(format_status(&flat, all_stats()).contains("n²=32"));

    let volume = StatusBarState {
        density: 24,
        is_3d: true,
        ..StatusBarState::default()
    };
    assert!(format_status(&volume, all_stats()).contains("n³=24"));
}

#[test]
fn format_status_shows_sim_time_only_when_some() {
    let status = StatusBarState {
        sim_time: Some(12.4),
        ..StatusBarState::default()
    };
    assert!(format_status(&status, all_stats()).contains("| t 12.4 s"));

    let hidden = format_status(&StatusBarState::default(), all_stats());
    assert!(!hidden.contains("| t "), "no clock, no time: {hidden}");

    let masked = StatusBarState {
        sim_time: Some(12.4),
        ..StatusBarState::default()
    };
    let mask = StatsMask {
        sim_time: false,
        ..all_stats()
    };
    assert!(!format_status(&masked, mask).contains("| t "));
}

#[test]
fn format_status_prefixes_fly_when_flying() {
    let status = StatusBarState {
        flying: true,
        ..StatusBarState::default()
    };
    let text = format_status(&status, all_stats());
    assert!(text.starts_with("fly | "), "flying readout: {text}");
}

#[test]
fn visible_stats_is_full_at_960_and_drops_in_priority_order() {
    assert_eq!(visible_stats(960.0), StatsMask::FULL);
    assert_eq!(visible_stats(1280.0), StatsMask::FULL);

    let below_960 = visible_stats(959.0);
    assert!(!below_960.sim_time, "simulation time drops first");
    assert!(below_960.density);
    assert!(below_960.trails);
    assert!(below_960.particles);

    assert!(visible_stats(700.0).density, "700 keeps density");
    let below_700 = visible_stats(699.0);
    assert!(!below_700.density);
    assert!(below_700.trails);
    assert!(below_700.particles);

    assert!(visible_stats(520.0).trails, "520 keeps trails");
    let below_520 = visible_stats(519.0);
    assert!(!below_520.trails);
    assert!(below_520.particles);

    assert!(visible_stats(360.0).particles, "360 keeps particles");
    let below_360 = visible_stats(359.0);
    assert!(!below_360.particles);
}

#[test]
fn visible_stats_is_a_strict_subset_and_never_hides_fps() {
    let full = StatsMask::FULL;
    let narrow = visible_stats(0.0);
    assert!(!narrow.sim_time && !narrow.density && !narrow.trails && !narrow.particles);

    let status = StatusBarState {
        fps: 60.0,
        frame_ms: 16.7,
        arrow_count: 12,
        streamline_count: 8,
        ..StatusBarState::default()
    };
    let text = format_status(&status, narrow);
    assert!(text.contains("60 fps"), "FPS is always present: {text}");
    assert!(text.contains("16.7 ms"), "frame time is always present: {text}");
    assert!(text.contains("12 arrows") && text.contains("8 streamlines"));
    assert!(!text.contains("particles"));
    assert!(!text.contains("trail segs"));
    assert!(!text.contains("n²"));
    assert!(!text.contains("| t "));

    for width in [960.0, 700.0, 520.0, 360.0, 0.0] {
        let mask = visible_stats(width);
        assert!(
            (!mask.sim_time || full.sim_time)
                && (!mask.density || full.density)
                && (!mask.trails || full.trails)
                && (!mask.particles || full.particles),
            "the mask at {width} is a subset of the full readout"
        );
    }
}

#[test]
fn advance_hint_expires_after_the_frame_time_lifetime() {
    let mut hint = Some("particles are off".to_owned());
    advance_hint(&mut hint, 0.0);
    assert_eq!(hint.as_deref(), Some("particles are off"));

    advance_hint(&mut hint, 2.49);
    assert!(hint.is_some(), "still live just before the lifetime");

    advance_hint(&mut hint, 2.5);
    assert!(hint.is_none(), "expires at exactly 2.5 s of frame time");

    // Accumulating frame time the way the app does clears at the same point.
    let mut hint = Some("2D view".to_owned());
    let mut elapsed = 0.0;
    for _ in 0..149 {
        elapsed += 1.0 / 60.0;
        advance_hint(&mut hint, elapsed);
    }
    assert!(hint.is_some());
    for _ in 0..2 {
        elapsed += 1.0 / 60.0;
        advance_hint(&mut hint, elapsed);
    }
    assert!(hint.is_none());
}

#[test]
fn advance_hint_ignores_invalid_elapsed_and_none() {
    for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -1.0] {
        let mut hint = Some("live".to_owned());
        advance_hint(&mut hint, bad);
        assert_eq!(hint.as_deref(), Some("live"), "invalid {bad} is ignored");
    }

    let mut none: Option<String> = None;
    advance_hint(&mut none, 10.0);
    assert_eq!(none, None);
}

/// Headless status bar surface: accesskit publishes the label texts, so the
/// left slot can be asserted without hard-coded geometry.
struct Harness {
    ctx: egui::Context,
    status: StatusBarState,
}

impl Harness {
    fn new(status: StatusBarState) -> Self {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        Self { ctx, status }
    }

    fn frame(&mut self) -> egui::FullOutput {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::pos2(0.0, 0.0),
                egui::vec2(960.0, 600.0),
            )),
            ..Default::default()
        };
        let status = &self.status;
        let mut output = self.ctx.run_ui(input, |ui| {
            egui::Panel::bottom("status_bar").show(ui, |ui| {
                vector_field_explorer::ui::status_bar::show(ui, status);
            });
        });
        output.textures_delta.clear();
        output
    }

    fn labels(&mut self) -> Vec<String> {
        // The first frame publishes nodes; the second settles label updates.
        self.frame();
        let output = self.frame();
        let Some(update) = output.platform_output.accesskit_update.as_ref() else {
            return Vec::new();
        };
        update
            .nodes
            .iter()
            .flat_map(|(_, node)| {
                [node.label().map(str::to_owned), node.value().map(str::to_owned)]
            })
            .flatten()
            .collect()
    }
}

#[test]
fn status_bar_uses_the_window_width_for_the_full_mask() {
    // The harness window is exactly the 960 pt minimum; `visible_stats(960)`
    // is the full mask, so every optional readout must be on screen. Feeding
    // the panel's inner width (~944) instead would drop simulation time here.
    let mut harness = Harness::new(StatusBarState {
        sim_time: Some(1.0),
        density: 32,
        particle_count: 5,
        trail_count: 2,
        ..StatusBarState::default()
    });
    let labels = harness.labels();
    let stats = labels
        .iter()
        .find(|label| label.contains("fps"))
        .unwrap_or_else(|| panic!("the stats readout is missing: {labels:?}"));
    assert!(stats.contains("5 particles"), "particles stay: {stats}");
    assert!(stats.contains("2 trail segs"), "trails stay: {stats}");
    assert!(stats.contains("n²=32"), "density stays: {stats}");
    assert!(stats.contains("t 1.0 s"), "sim time stays at 960 pt: {stats}");
}

#[test]
fn status_bar_renders_hint_until_the_parse_error_wins() {
    let mut hint = Harness::new(StatusBarState {
        hint: Some("particles are off".to_owned()),
        ..StatusBarState::default()
    });
    let labels = hint.labels();
    assert!(
        labels.iter().any(|label| label.contains("particles are off")),
        "the live hint takes the ready slot: {labels:?}"
    );
    assert!(
        labels.iter().any(|label| label.contains("fps")),
        "the stats readout is always present: {labels:?}"
    );

    let mut error = Harness::new(StatusBarState {
        parse_message: "bad formula".to_owned(),
        hint: Some("particles are off".to_owned()),
        ..StatusBarState::default()
    });
    let labels = error.labels();
    assert!(
        labels.iter().any(|label| label.contains("bad formula")),
        "the parse error wins: {labels:?}"
    );
    assert!(
        !labels.iter().any(|label| label.contains("particles are off")),
        "the hint is suppressed by a parse error: {labels:?}"
    );
}
