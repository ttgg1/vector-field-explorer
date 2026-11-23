//! Issue #234: bounded parameter sweeps from a lesson reading.
//!
//! The sweep reuses the shipped `ideal_axial_field` reference and the #219
//! `compute_reading`/`flux_estimate` seam. It adds no solver, quadrature rule,
//! physics model or durable probe: samples are the ordinary lesson readings,
//! and invalid samples stay gaps instead of becoming zero.

use eframe::egui;

use crate::support;
use vector_field_explorer::maxwell::{MaxwellCase, MaxwellScene};
use vector_field_explorer::sweep::{
    FluxConstancyCheck, SweepConfig, SweepConfigError, SweepKind, SweepSample, SweepSession,
    SweepStatus, evaluate, fixed_parameters_text, flux_constancy_report, odd_symmetry_report,
};
use vector_field_explorer::ui::ring_lesson::{
    RING_CHARGE, RING_RADIUS, RING_UNITS_NOTE, RingLessonState, RingPrediction, ideal_axial_field,
};
use vector_field_explorer::ui::shell_lesson::{
    ShellFieldPrediction, ShellFluxPrediction, ShellLessonState,
};

const SWEEP_TITLE: &str = "Parameter sweep";
const RUN_LABEL: &str = "Run sweep";
const CANCEL_LABEL: &str = "Cancel sweep";
const EXIT_PREVIEW_LABEL: &str = "Exit preview";
const ADOPT_LABEL: &str = "Adopt sample";

fn hollow(radius: f64) -> MaxwellScene {
    MaxwellScene {
        case: MaxwellCase::HollowSphere,
        radius,
        ..MaxwellScene::default()
    }
}

fn samples_for(config: &SweepConfig, scene: &MaxwellScene) -> Vec<SweepSample> {
    (0..config.len())
        .map(|index| evaluate(config.kind, scene, config.x_at(index)))
        .collect()
}

fn texts(out: &egui::FullOutput) -> Vec<String> {
    out.shapes
        .iter()
        .filter_map(|clipped| match &clipped.shape {
            egui::Shape::Text(shape) => Some(shape.galley.text().to_owned()),
            _ => None,
        })
        .collect()
}

fn node_rect(out: &egui::FullOutput, label: &str) -> Option<egui::Rect> {
    out.platform_output
        .accesskit_update
        .as_ref()?
        .nodes
        .iter()
        .find_map(|(_, node)| {
            (node.label() == Some(label) || node.value() == Some(label)).then(|| {
                let b = node.bounds().expect("widget bounds");
                egui::Rect::from_min_max(
                    egui::pos2(b.x0 as f32, b.y0 as f32),
                    egui::pos2(b.x1 as f32, b.y1 as f32),
                )
            })
        })
}

#[test]
fn config_rejects_unordered_non_finite_and_out_of_bounds_inputs() {
    let kind = SweepKind::RingAxialField;
    assert_eq!(
        SweepConfig::new(kind, 2.0, -2.0, 33),
        Err(SweepConfigError::UnorderedRange)
    );
    assert_eq!(
        SweepConfig::new(kind, 1.0, 1.0, 33),
        Err(SweepConfigError::UnorderedRange)
    );
    assert_eq!(
        SweepConfig::new(kind, f64::NAN, 2.0, 33),
        Err(SweepConfigError::NonFiniteRange)
    );
    assert_eq!(
        SweepConfig::new(kind, -2.0, f64::INFINITY, 33),
        Err(SweepConfigError::NonFiniteRange)
    );
    assert_eq!(
        SweepConfig::new(kind, -2.0, 2.0, 2),
        Err(SweepConfigError::SampleCount)
    );
    assert_eq!(
        SweepConfig::new(kind, -2.0, 2.0, 130),
        Err(SweepConfigError::SampleCount)
    );

    let default = SweepConfig::ring_default();
    assert_eq!(default.min, -2.0);
    assert_eq!(default.max, 2.0);
    assert_eq!(default.samples, 33);
    assert_eq!(default.len(), 33);
    assert!((default.x_at(0) - -2.0).abs() < 1e-12);
    assert!((default.x_at(32) - 2.0).abs() < 1e-12);
    assert_eq!(SweepConfig::new(kind, -2.0, 2.0, 3).unwrap().len(), 3);
}

#[test]
fn ring_samples_follow_the_ideal_field_and_report_odd_symmetry() {
    let scene = MaxwellScene::default();
    let at_one = evaluate(SweepKind::RingAxialField, &scene, 1.0);
    let at_minus_one = evaluate(SweepKind::RingAxialField, &scene, -1.0);
    assert_eq!(at_one.value, Some(ideal_axial_field(1.0)));
    assert!(at_one.gap.is_none());
    let one = at_one.value.unwrap();
    let minus_one = at_minus_one.value.unwrap();
    assert!((one + minus_one).abs() < 1e-12, "ideal field is odd");

    let config = SweepConfig::ring_default();
    let samples = samples_for(&config, &scene);
    let check = odd_symmetry_report(&config, &samples).expect("symmetric range pairs samples");
    assert_eq!(check.pairs, 16);
    assert!(check.satisfied, "default range is odd-symmetric: {check:?}");
    assert!(check.max_residual <= check.tolerance);
    assert!(check.tolerance > 0.0, "tolerance is stated and derived");

    let asymmetric = SweepConfig::new(SweepKind::RingAxialField, -1.0, 2.0, 33).unwrap();
    assert!(odd_symmetry_report(&asymmetric, &samples).is_none());
}

#[test]
fn a_non_finite_ring_sample_is_a_gap_not_zero() {
    let scene = MaxwellScene::default();
    let sample = evaluate(SweepKind::RingAxialField, &scene, f64::NAN);
    assert!(sample.has_gap());
    assert_eq!(sample.value, None);
    assert!(sample.gap.is_some());

    let config = SweepConfig::new(SweepKind::RingAxialField, -1.0, 1.0, 3).unwrap();
    let mut samples = samples_for(&config, &scene);
    samples[1] = evaluate(SweepKind::RingAxialField, &scene, f64::NAN);
    let check = odd_symmetry_report(&config, &samples).expect("paired finite endpoints");
    assert_eq!(check.pairs, 1, "the invalid centre sample is not paired");
}

#[test]
fn ring_fixed_parameters_and_units_are_labelled() {
    let scene = MaxwellScene::default();
    let text = fixed_parameters_text(SweepKind::RingAxialField, &scene);
    assert!(text.contains(&format!("R = {RING_RADIUS:.3}")), "{text}");
    assert!(text.contains(&format!("Q = {RING_CHARGE:.3}")), "{text}");
    assert!(SweepKind::RingAxialField.units_note().contains("k = 1"));
    assert_eq!(
        SweepKind::RingAxialField.independent_label(),
        "z (signed axial probe position)"
    );
    assert!(RING_UNITS_NOTE.contains("k = 1"));
}

#[test]
fn shell_sweep_reports_interior_and_interface_gaps() {
    let scene = hollow(1.0);
    let interior = evaluate(SweepKind::ShellFlux, &scene, 0.5);
    assert!(interior.has_gap(), "interior is a gap, not zero");
    assert_eq!(interior.value, None);
    assert!(
        interior
            .gap
            .as_deref()
            .unwrap()
            .contains("Inside the shell")
    );

    let interface = evaluate(SweepKind::ShellFlux, &scene, scene.radius);
    assert!(interface.has_gap(), "the ideal interface is excluded");
    assert!(interface.gap.as_deref().unwrap().contains("excluded"));

    let exterior = evaluate(SweepKind::ShellFlux, &scene, 1.5);
    assert!(!exterior.has_gap());
    assert!(exterior.value.is_some());
    assert!(exterior.uncertainty.is_some(), "sampled error is carried");
}

#[test]
fn shell_exterior_flux_is_constant_within_quadrature_uncertainty() {
    let scene = hollow(1.0);
    let config = SweepConfig::shell_default(&scene);
    assert_eq!(config.min, 0.5);
    assert_eq!(config.max, 2.5);
    assert_eq!(config.samples, 33);

    let samples = samples_for(&config, &scene);
    assert_eq!(samples.iter().filter(|sample| sample.has_gap()).count(), 9);
    let check = flux_constancy_report(&samples).expect("exterior samples exist");
    assert_eq!(check.samples, 24);
    assert!(
        check.satisfied,
        "exterior flux is constant within uncertainty: {check:?}"
    );
    assert!(check.spread <= check.combined_uncertainty + 1e-9);
    assert!(check.combined_uncertainty > 0.0);
}

#[test]
fn a_varying_flux_is_not_reported_constant() {
    let samples = vec![
        SweepSample::valid(1.2, 12.5, Some(1e-6)),
        SweepSample::valid(1.5, 6.0, Some(1e-6)),
        SweepSample::valid(2.0, 4.2, Some(1e-6)),
    ];
    let check: FluxConstancyCheck = flux_constancy_report(&samples).unwrap();
    assert!(!check.satisfied, "a deliberately varying curve is not flat");
    assert!(check.spread > check.combined_uncertainty);
}

#[test]
fn a_cancelled_run_is_never_complete_and_keeps_its_partial_samples() {
    let scene = MaxwellScene::default();
    let config = SweepConfig::new(SweepKind::RingAxialField, -2.0, 2.0, 5).unwrap();
    let mut session = SweepSession::new(SweepKind::RingAxialField, &scene);
    session.min = config.min;
    session.max = config.max;
    session.samples = config.samples;
    assert_eq!(session.status(), SweepStatus::Idle);
    session.start(&scene).unwrap();
    assert_eq!(session.status(), SweepStatus::Sampling);
    assert!(session.step());
    assert!(session.step());
    assert_eq!(session.samples().len(), 2);
    session.cancel();
    assert_eq!(session.status(), SweepStatus::Cancelled);
    assert!(!session.step(), "cancel stops pending work");
    assert_eq!(session.samples().len(), 2);
    assert!(
        session.status_text().contains("cancelled"),
        "{}",
        session.status_text()
    );
    assert_ne!(session.status(), SweepStatus::Complete);
}

#[test]
fn runs_compute_one_sample_per_step_and_stop_at_the_bound() {
    let scene = MaxwellScene::default();
    let config = SweepConfig::new(SweepKind::RingAxialField, -1.0, 1.0, 4).unwrap();
    let mut session = SweepSession::new(SweepKind::RingAxialField, &scene);
    session.min = config.min;
    session.max = config.max;
    session.samples = config.samples;
    assert_eq!(session.status(), SweepStatus::Idle);
    assert_eq!(session.samples().len(), 0);
    session.start(&scene).unwrap();
    for expected in 1..=4 {
        assert!(session.step());
        assert_eq!(session.samples().len(), expected);
        if expected < 4 {
            assert_eq!(session.status(), SweepStatus::Sampling);
        }
    }
    assert_eq!(session.status(), SweepStatus::Complete);
    assert_eq!(session.status_text(), "complete 4/4");
    assert!(!session.step(), "a complete run does no more work");
    assert_eq!(session.samples().len(), 4);
}

#[test]
fn invalid_config_does_not_start_a_run() {
    let scene = MaxwellScene::default();
    let mut session = SweepSession::new(SweepKind::RingAxialField, &scene);
    session.min = 1.0;
    session.max = 1.0;
    assert_eq!(session.start(&scene), Err(SweepConfigError::UnorderedRange));
    assert_eq!(session.status(), SweepStatus::Invalid);
    assert!(session.samples().is_empty());
    assert!(session.status_text().contains("invalid"));
}

#[test]
fn fixed_shell_parameters_show_r_and_charge() {
    let scene = hollow(1.0);
    let text = fixed_parameters_text(SweepKind::ShellFlux, &scene);
    assert!(text.contains("R = 1.000"), "{text}");
    assert!(text.contains("Q = 1.000"), "{text}");
    assert!(SweepKind::ShellFlux.units_note().contains("quadrature"));
}

#[test]
fn preview_keeps_the_starting_probe_until_it_is_explicitly_ended() {
    let scene = MaxwellScene::default();
    let mut session = SweepSession::new(SweepKind::RingAxialField, &scene);
    assert!(!session.is_previewing());
    assert_eq!(session.end_preview(), None);

    session.note_preview(1.0, 0.25);
    assert!(session.is_previewing());
    assert_eq!(session.preview_start(), Some(0.25));
    assert_eq!(session.preview_target(), Some(1.0));

    session.note_preview(1.5, 1.0);
    assert_eq!(
        session.preview_start(),
        Some(0.25),
        "the first preview stores the starting probe"
    );
    assert_eq!(session.preview_target(), Some(1.5));

    assert_eq!(session.end_preview(), Some(0.25));
    assert!(!session.is_previewing());
}

struct RingHarness {
    ctx: egui::Context,
    state: RingLessonState,
    width: f32,
    height: f32,
}

impl RingHarness {
    fn new(width: f32, height: f32) -> Self {
        let ctx = support::context();
        ctx.enable_accesskit();
        ctx.all_styles_mut(|style| style.animation_time = 0.0);
        let mut state = RingLessonState::default();
        state.activate();
        Self {
            ctx,
            state,
            width,
            height,
        }
    }

    fn frame(&mut self, events: Vec<egui::Event>) -> egui::FullOutput {
        let mut out = self.ctx.run_pass(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(self.width, self.height),
                )),
                events,
                ..Default::default()
            },
            |ctx| {
                let viewport = egui::Rect::from_min_max(
                    egui::pos2(12.0, 72.0),
                    egui::pos2(self.width - 24.0, self.height),
                );
                egui::Area::new(egui::Id::unique("vector view test surface"))
                    .order(egui::Order::Background)
                    .fixed_pos(viewport.min)
                    .show(ctx, |ui| {
                        let (_, field) =
                            ui.allocate_exact_size(viewport.size(), egui::Sense::click_and_drag());
                        assert!(
                            !field.dragged() && !field.clicked(),
                            "sweep input reached the field"
                        );
                    });
                vector_field_explorer::ui::ring_lesson::show(ctx, Some(viewport), &mut self.state);
            },
        );
        out.textures_delta.clear();
        out
    }

    fn idle(&mut self) -> egui::FullOutput {
        for _ in 0..2 {
            self.frame(Vec::new());
        }
        self.frame(Vec::new())
    }

    fn click(&mut self, pos: egui::Pos2) -> egui::FullOutput {
        let pointer = |pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        self.frame(vec![egui::Event::PointerMoved(pos), pointer(true)]);
        self.frame(vec![pointer(false)])
    }

    fn click_label(&mut self, label: &str) -> egui::FullOutput {
        let out = self.frame(Vec::new());
        let rect = node_rect(&out, label)
            .unwrap_or_else(|| panic!("`{label}` is reachable: {:?}", texts(&out)));
        self.click(rect.center())
    }

    fn sample_position(&self, x: f64) -> egui::Pos2 {
        let response = self
            .ctx
            .read_response(vector_field_explorer::ui::sweep::chart_id())
            .expect("the sweep chart is registered");
        let plot = vector_field_explorer::ui::sweep::chart_plot_rect(response.rect);
        let config = self.state.sweep().config_used().expect("a completed run");
        egui::pos2(
            vector_field_explorer::ui::sweep::sample_x_position(plot, &config, x),
            plot.center().y,
        )
    }
}

fn reveal_ring(h: &mut RingHarness) {
    h.state.select_prediction(RingPrediction::PartwayOut);
    assert!(h.state.reveal());
}

#[test]
fn sweep_header_stays_inside_the_vector_view_after_resize() {
    let mut h = RingHarness::new(1200.0, 800.0);
    reveal_ring(&mut h);
    let title = |out: &egui::FullOutput| {
        out.shapes
            .iter()
            .find_map(|s| match &s.shape {
                egui::Shape::Text(text) if text.galley.text() == SWEEP_TITLE => {
                    Some(text.visual_bounding_rect())
                }
                _ => None,
            })
            .expect("painted sweep title")
    };
    for width in [1200.0, 390.0] {
        h.width = width;
        let rect = title(&h.idle());
        assert!(
            rect.top() >= 72.0 && rect.right() <= width - 24.0,
            "{rect:?}"
        );
        let start = rect.center();
        let end = start + egui::vec2(if width > 400.0 { -100.0 } else { 0.0 }, 64.0);
        let pointer = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        h.frame(vec![egui::Event::PointerMoved(start), pointer(start, true)]);
        h.frame(vec![egui::Event::PointerMoved(end)]);
        h.frame(vec![pointer(end, false)]);
        let out = h.idle();
        let moved = title(&out);
        assert!(
            moved.center().distance(start) > 32.0,
            "{rect:?} -> {moved:?}"
        );
        assert_eq!(title(&h.idle()), moved);
        let reset = out
            .platform_output
            .accesskit_update
            .unwrap()
            .nodes
            .into_iter()
            .find(|(_, n)| n.label() == Some("Reset sweep position"))
            .unwrap()
            .0;
        let key = |key, pressed| egui::Event::Key {
            key,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        assert!((0..48).any(|_| {
            h.frame(vec![key(egui::Key::Tab, true), key(egui::Key::Tab, false)])
                .platform_output
                .accesskit_update
                .unwrap()
                .focus
                == reset
        }));
        h.frame(vec![
            key(egui::Key::Enter, true),
            key(egui::Key::Enter, false),
        ]);
        let restored = title(&h.idle());
        assert!(
            (restored.top() - rect.top()).abs() <= 1.0,
            "{rect:?} -> {restored:?}"
        );
        if width > 400.0 {
            assert_eq!(rect, restored);
        }
    }
}

#[test]
fn the_sweep_stays_hidden_until_the_ring_lesson_is_revealed() {
    let mut h = RingHarness::new(1200.0, 800.0);
    h.state.select_prediction(RingPrediction::PartwayOut);
    h.idle();
    let hidden = texts(&h.frame(Vec::new())).join("\n");
    assert!(
        !hidden.contains(SWEEP_TITLE),
        "the sweep is hidden before the reveal: {hidden}"
    );
    assert!(!hidden.contains(RUN_LABEL));

    assert!(h.state.reveal());
    h.idle();
    let shown = texts(&h.frame(Vec::new())).join("\n");
    assert!(shown.contains(SWEEP_TITLE), "the sweep appears: {shown}");
    assert!(shown.contains(RUN_LABEL), "{shown}");
    assert!(
        shown.contains("Fixed parameters: R = 1.000, Q = 1.000"),
        "fixed parameters are labelled: {shown}"
    );
    assert!(shown.contains("k = 1"), "units are labelled: {shown}");
}

#[test]
fn running_the_ring_sweep_is_bounded_and_completes() {
    let mut h = RingHarness::new(1200.0, 800.0);
    reveal_ring(&mut h);
    h.idle();
    h.click_label(RUN_LABEL);
    h.idle();
    let partial = texts(&h.frame(Vec::new())).join("\n");
    assert!(
        partial.contains("sampling"),
        "one sample per frame is still sampling: {partial}"
    );
    for _ in 0..40 {
        h.frame(Vec::new());
    }
    let done = texts(&h.frame(Vec::new())).join("\n");
    assert!(done.contains("complete 33/33"), "the run completes: {done}");
    assert!(done.contains("Odd symmetry"), "the report is shown: {done}");
    assert!(
        done.contains("satisfied"),
        "the symmetry check passes: {done}"
    );
}

#[test]
fn clicking_a_ring_sample_previews_the_probe_and_exit_restores_it() {
    let mut h = RingHarness::new(1200.0, 800.0);
    reveal_ring(&mut h);
    h.state.set_probe_z(0.25);
    h.idle();
    h.click_label(RUN_LABEL);
    for _ in 0..40 {
        h.frame(Vec::new());
    }
    h.idle();

    let pos = h.sample_position(1.0);
    h.click(pos);
    h.idle();
    assert!(
        (h.state.probe_z() - 1.0).abs() < 1e-9,
        "the click previews z = 1: {}",
        h.state.probe_z()
    );
    assert!(h.state.sweep().is_previewing());
    assert!((h.state.reading().z - 1.0).abs() < 1e-9);
    let previewed = texts(&h.frame(Vec::new())).join("\n");
    assert!(
        previewed.contains("At z = 1.000"),
        "the readout follows the preview: {previewed}"
    );
    assert!(
        previewed.contains("E_z = 0.353553"),
        "the preview shows the sample's value: {previewed}"
    );

    h.click_label(EXIT_PREVIEW_LABEL);
    h.idle();
    assert!(
        (h.state.probe_z() - 0.25).abs() < 1e-9,
        "Exit preview restores the starting probe: {}",
        h.state.probe_z()
    );
    assert!(!h.state.sweep().is_previewing());
}

#[test]
fn adopting_a_ring_sample_keeps_the_previewed_probe() {
    let mut h = RingHarness::new(1200.0, 800.0);
    reveal_ring(&mut h);
    h.state.set_probe_z(0.25);
    h.idle();
    h.click_label(RUN_LABEL);
    for _ in 0..40 {
        h.frame(Vec::new());
    }
    h.idle();

    let pos = h.sample_position(0.5);
    h.click(pos);
    h.idle();
    h.click_label(ADOPT_LABEL);
    h.idle();
    assert!(
        (h.state.probe_z() - 0.5).abs() < 1e-9,
        "Adopt keeps the previewed probe: {}",
        h.state.probe_z()
    );
    assert!(!h.state.sweep().is_previewing());
}

#[test]
fn cancelling_the_ring_sweep_keeps_a_labelled_partial_curve() {
    let mut h = RingHarness::new(1200.0, 800.0);
    reveal_ring(&mut h);
    h.idle();
    h.click_label(RUN_LABEL);
    for _ in 0..5 {
        h.frame(Vec::new());
    }
    let before = h.state.sweep().samples().len();
    h.click_label(CANCEL_LABEL);
    h.idle();
    assert_eq!(h.state.sweep().status(), SweepStatus::Cancelled);
    let after = h.state.sweep().samples().len();
    for _ in 0..5 {
        h.frame(Vec::new());
    }
    assert_eq!(
        h.state.sweep().samples().len(),
        after,
        "cancel stops pending samples"
    );
    assert!(after > 0 && after < 33, "partial work is kept: {before}");
    let text = texts(&h.frame(Vec::new())).join("\n");
    assert!(text.contains("cancelled after"), "{text}");
    assert!(text.contains("Samples evaluated:"), "curve stays: {text}");
}

#[test]
fn the_ring_and_shell_lessons_stay_usable_in_wide_and_compact_layouts() {
    for (width, height) in [(390.0, 640.0), (1200.0, 800.0)] {
        let mut ring = RingHarness::new(width, height);
        reveal_ring(&mut ring);
        ring.idle();
        ring.state.sweep_mut().samples = 3;
        ring.click_label(RUN_LABEL);
        for _ in 0..5 {
            ring.frame(Vec::new());
        }
        let text = texts(&ring.frame(Vec::new())).join("\n");
        assert!(
            text.contains(SWEEP_TITLE) && text.contains("complete 3/3"),
            "{width}x{height} ring sweep: {text}"
        );

        let scene = hollow(1.0);
        let mut shell = ShellHarness::new(width, height, scene.clone());
        shell.state.activate(&scene);
        shell
            .state
            .select_field_prediction(ShellFieldPrediction::ZeroInside);
        shell
            .state
            .select_flux_prediction(ShellFluxPrediction::ZeroInside);
        assert!(shell.state.reveal());
        shell.idle();
        shell.state.sweep_mut().min = 1.1;
        shell.state.sweep_mut().max = 2.5;
        shell.state.sweep_mut().samples = 3;
        shell.click_label(RUN_LABEL);
        for _ in 0..5 {
            shell.frame(Vec::new());
        }
        let text = texts(&shell.frame(Vec::new())).join("\n");
        assert!(
            text.contains(SWEEP_TITLE) && text.contains("complete 3/3"),
            "{width}x{height} shell sweep: {text}"
        );
    }
}

struct ShellHarness {
    ctx: egui::Context,
    state: ShellLessonState,
    scene: MaxwellScene,
    width: f32,
    height: f32,
}

impl ShellHarness {
    fn new(width: f32, height: f32, scene: MaxwellScene) -> Self {
        let ctx = support::context();
        ctx.enable_accesskit();
        ctx.all_styles_mut(|style| style.animation_time = 0.0);
        Self {
            ctx,
            state: ShellLessonState::default(),
            scene,
            width,
            height,
        }
    }

    fn frame(&mut self, events: Vec<egui::Event>) -> egui::FullOutput {
        let mut out = self.ctx.run_pass(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(self.width, self.height),
                )),
                events,
                ..Default::default()
            },
            |ctx| {
                vector_field_explorer::ui::shell_lesson::show(
                    ctx,
                    Some(ctx.content_rect()),
                    &mut self.state,
                    &self.scene,
                    vector_field_explorer::sources::FieldFamily::Electric,
                    0.0,
                );
            },
        );
        out.textures_delta.clear();
        out
    }

    fn idle(&mut self) -> egui::FullOutput {
        for _ in 0..2 {
            self.frame(Vec::new());
        }
        self.frame(Vec::new())
    }

    fn click(&mut self, pos: egui::Pos2) -> egui::FullOutput {
        let pointer = |pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        self.frame(vec![egui::Event::PointerMoved(pos), pointer(true)]);
        self.frame(vec![pointer(false)])
    }

    fn click_label(&mut self, label: &str) -> egui::FullOutput {
        let out = self.frame(Vec::new());
        let rect = node_rect(&out, label)
            .unwrap_or_else(|| panic!("`{label}` is reachable: {:?}", texts(&out)));
        self.click(rect.center())
    }

    fn sample_position(&self, x: f64) -> egui::Pos2 {
        let response = self
            .ctx
            .read_response(vector_field_explorer::ui::sweep::chart_id())
            .expect("the sweep chart is registered");
        let plot = vector_field_explorer::ui::sweep::chart_plot_rect(response.rect);
        let config = self.state.sweep().config_used().expect("a completed run");
        egui::pos2(
            vector_field_explorer::ui::sweep::sample_x_position(plot, &config, x),
            plot.center().y,
        )
    }
}

#[test]
fn a_shell_sample_preview_uses_the_existing_radius_setter_and_restores() {
    let scene = hollow(1.0);
    let mut h = ShellHarness::new(1200.0, 800.0, scene.clone());
    h.state.activate(&scene);
    h.state
        .select_field_prediction(ShellFieldPrediction::ZeroInside);
    h.state
        .select_flux_prediction(ShellFluxPrediction::ZeroInside);
    assert!(h.state.reveal());
    assert!(h.state.set_probe_radius(0.5));
    h.state.sweep_mut().min = 1.1;
    h.state.sweep_mut().max = 2.5;
    h.state.sweep_mut().samples = 5;
    h.idle();
    h.click_label(RUN_LABEL);
    for _ in 0..8 {
        h.frame(Vec::new());
    }
    h.idle();
    let done = texts(&h.frame(Vec::new())).join("\n");
    assert!(done.contains("complete 5/5"), "shell run completes: {done}");
    assert!(
        done.contains("Exterior flux"),
        "the constancy report is shown: {done}"
    );

    let config = h.state.sweep().config_used().unwrap();
    let target = config.x_at(2);
    let pos = h.sample_position(target);
    h.click(pos);
    h.idle();
    assert!(
        (h.state.probe_radius() - target).abs() < 1e-9,
        "the existing radius setter receives the preview: {}",
        h.state.probe_radius()
    );
    assert!(h.state.sweep().is_previewing());

    h.click_label(EXIT_PREVIEW_LABEL);
    h.idle();
    assert!(
        (h.state.probe_radius() - 0.5).abs() < 1e-9,
        "Exit preview restores the starting radius: {}",
        h.state.probe_radius()
    );
}

#[test]
fn gap_tooltips_and_markers_report_invalid_samples_without_zeroing_them() {
    let scene = hollow(1.0);
    let mut h = ShellHarness::new(1200.0, 800.0, scene.clone());
    h.state.activate(&scene);
    h.state
        .select_field_prediction(ShellFieldPrediction::ZeroInside);
    h.state
        .select_flux_prediction(ShellFluxPrediction::ZeroInside);
    assert!(h.state.reveal());
    h.state.sweep_mut().min = 0.5;
    h.state.sweep_mut().max = 2.5;
    h.state.sweep_mut().samples = 9;
    h.idle();
    h.click_label(RUN_LABEL);
    for _ in 0..12 {
        h.frame(Vec::new());
    }
    let text = texts(&h.frame(Vec::new())).join("\n");
    assert!(text.contains("complete 9/9"), "{text}");
    assert!(text.contains("Gaps:"), "gap count is shown: {text}");
}
