//! Issue #95 slice B: probe document state, report cache and wireframe lines.
//!
//! Every value is a normalized teaching quantity, not an SI measurement.

use eframe::egui;
use glam::Vec3;

use crate::support;

use vector_field_explorer::field_probe::{
    CirculationLoop, GaussianSurface, IntegralStatus, LoopKind, MAX_LOOPS, MAX_SURFACES,
    SurfaceKind,
};
use vector_field_explorer::layer_style::LayerStyles;
use vector_field_explorer::source_scene::{FieldMode, SourceDisplay, SourceEntry, SourceScene};
use vector_field_explorer::sources::{FieldFamily, SourceKind};
use vector_field_explorer::ui::sidebar::{self, Controls};
use vector_field_explorer::viewmodel::State;

fn sphere(center: Vec3, radius: f32, segments: u32) -> GaussianSurface {
    GaussianSurface {
        kind: SurfaceKind::Sphere,
        center,
        axis: Vec3::Z,
        size: Vec3::splat(2.0 * radius),
        segments,
    }
}

fn circle(center: Vec3, radius: f32, segments: u32) -> CirculationLoop {
    CirculationLoop {
        kind: LoopKind::Circle,
        center,
        axis: Vec3::Z,
        size: Vec3::splat(2.0 * radius),
        segments,
    }
}

fn point_charge_scene() -> SourceScene {
    SourceScene {
        entries: vec![SourceEntry {
            enabled: true,
            source: vector_field_explorer::sources::Source {
                center: glam::DVec3::ZERO,
                axis: glam::DVec3::Z,
                kind: SourceKind::PointCharge { charge: 1.0 },
            },
        }],
        display: SourceDisplay::Electric,
        ..SourceScene::default()
    }
}

#[test]
fn adding_and_updating_probes_dirty_once_and_rejected_edits_do_not() {
    let mut state = State::new(true);
    state.dirty = false;

    let first = sphere(Vec3::ZERO, 1.0, 16);
    assert_eq!(
        state
            .add_probe_surface(first)
            .expect("the surface is valid"),
        0
    );
    assert!(state.dirty, "an accepted add dirties once");
    assert_eq!(state.probe_surfaces(), &[first]);

    state.dirty = false;
    state
        .update_probe_surface(0, first)
        .expect("an identical update is a validated no-op");
    assert!(!state.dirty, "an identical update never dirties");
    assert_eq!(state.probe_surfaces(), &[first]);

    let mut invalid = first;
    invalid.segments = 2;
    state.dirty = false;
    assert!(state.update_probe_surface(0, invalid).is_err());
    assert!(!state.dirty, "a rejected update never dirties");
    assert_eq!(
        state.probe_surfaces(),
        &[first],
        "a rejected update leaves the stored geometry"
    );

    let moved = sphere(Vec3::new(0.5, 0.0, 0.0), 1.0, 16);
    state.dirty = false;
    state
        .update_probe_surface(0, moved)
        .expect("the moved surface is valid");
    assert!(state.dirty, "a real update dirties once");
    assert_eq!(state.probe_surfaces(), &[moved]);

    state.dirty = false;
    assert!(state.remove_probe_surface(0));
    assert!(state.dirty, "a real removal dirties once");
    state.dirty = false;
    assert!(!state.remove_probe_surface(0));
    assert!(!state.dirty, "an out-of-range removal never dirties");

    let mut state = State::new(true);
    state.dirty = false;
    let first = circle(Vec3::ZERO, 1.0, 16);
    assert_eq!(state.add_probe_loop(first).expect("the loop is valid"), 0);
    assert!(state.dirty);
    state.dirty = false;
    state
        .update_probe_loop(0, first)
        .expect("an identical update is a no-op");
    assert!(!state.dirty);
    let mut invalid = first;
    invalid.segments = 0;
    assert!(state.update_probe_loop(0, invalid).is_err());
    assert!(!state.dirty);
    assert_eq!(state.probe_loops(), &[first]);

    let mut state = State::new(true);
    for _ in 0..MAX_SURFACES {
        state
            .add_probe_surface(sphere(Vec3::ZERO, 1.0, 16))
            .expect("the list has room");
    }
    state.dirty = false;
    assert!(
        state
            .add_probe_surface(sphere(Vec3::ZERO, 1.0, 16))
            .is_err()
    );
    assert!(!state.dirty, "a cap rejection never dirties");
    for _ in 0..MAX_LOOPS {
        state
            .add_probe_loop(circle(Vec3::ZERO, 1.0, 16))
            .expect("the list has room");
    }
    state.dirty = false;
    assert!(state.add_probe_loop(circle(Vec3::ZERO, 1.0, 16)).is_err());
    assert!(!state.dirty, "a cap rejection never dirties");
}

#[test]
fn flux_through_a_sphere_uses_the_active_family_and_mode() {
    let mut state = State::new(true);
    state.set_field_mode(FieldMode::Sources);
    state.set_source_scene(point_charge_scene());
    state.set_probe_family(FieldFamily::Electric);
    let surface = sphere(Vec3::ZERO, 1.0, 32);
    state.add_probe_surface(surface).expect("valid surface");
    state.refresh_probe_reports(true);
    let electric = state
        .probe_surface_report(0)
        .expect("the visible refresh built a report")
        .clone();
    assert_eq!(electric.family, FieldFamily::Electric);
    let IntegralStatus::Estimate { value, .. } = electric.status else {
        panic!("the electric flux should resolve: {:?}", electric.status);
    };
    // Normalized point-charge field q r / (r^2 + s^2)^(3/2); flux through a
    // sphere of radius R is 4 pi q R^3 / (R^2 + s^2)^(3/2). Softening is the
    // source scene default.
    let softening = state.sources.softening;
    let expected = std::f64::consts::TAU * 2.0 / (1.0 + softening * softening).powf(1.5);
    assert!(
        (value - expected).abs() < expected.abs() * 2e-2 + 1e-3,
        "electric flux {value} should match the analytic {expected}"
    );
    assert!(electric.samples > 0);

    state.set_probe_family(FieldFamily::Magnetic);
    state.refresh_probe_reports(true);
    let magnetic = state
        .probe_surface_report(0)
        .expect("the refresh replaced the report")
        .clone();
    assert_eq!(magnetic.family, FieldFamily::Magnetic);
    let IntegralStatus::Estimate { value, .. } = magnetic.status else {
        panic!("the empty magnetic family should be an exact zero");
    };
    assert!(
        value.abs() < 1e-6,
        "a charge-only scene has no magnetic flux, got {value}"
    );
}

#[test]
fn unsupported_modes_report_a_reason_instead_of_a_number() {
    let mut state = State::new(true);
    state
        .add_probe_surface(sphere(Vec3::ZERO, 1.0, 16))
        .expect("valid surface");

    // The default Formula mode cannot supply a probe field.
    state.refresh_probe_reports(true);
    match &state
        .probe_surface_report(0)
        .expect("a report exists")
        .status
    {
        IntegralStatus::Unresolved { reason } => {
            assert!(
                reason.contains("Formula"),
                "the reason names the mode: {reason}"
            );
        }
        other => panic!("Formula must be unsupported, got {other:?}"),
    }

    state.set_field_mode(FieldMode::Ode);
    state.refresh_probe_reports(true);
    match &state
        .probe_surface_report(0)
        .expect("a report exists")
        .status
    {
        IntegralStatus::Unresolved { reason } => {
            assert!(
                reason.contains("Ode"),
                "the reason names the mode: {reason}"
            );
        }
        other => panic!("Ode must be unsupported, got {other:?}"),
    }

    // Issue #220: Induction supplies the induced electric field and its
    // unit-current magnetic field, so a magnetic check now resolves instead of
    // being refused. The closed surface encloses no magnetic charge, so its
    // flux stays zero within the reported spread.
    state.set_field_mode(FieldMode::Induction);
    state.set_probe_family(FieldFamily::Magnetic);
    state.refresh_probe_reports(true);
    match &state
        .probe_surface_report(0)
        .expect("a report exists")
        .status
    {
        IntegralStatus::Estimate { value, error } => {
            assert!(value.is_finite());
            assert!(
                value.abs() <= error + 1e-6,
                "a closed surface around the induction B has zero net flux, got {value} ± {error}"
            );
        }
        other => panic!("an induction magnetic check must now resolve, got {other:?}"),
    }
}

#[test]
fn probe_reports_are_cached_until_geometry_or_mode_changes() {
    let mut state = State::new(true);
    state
        .add_probe_surface(sphere(Vec3::ZERO, 1.0, 16))
        .expect("valid surface");

    state.refresh_probe_reports(true);
    let first = state.probe_report_generation();
    assert!(first > 0, "the first visible frame samples");
    state.refresh_probe_reports(true);
    assert_eq!(
        state.probe_report_generation(),
        first,
        "an unchanged identity reuses the cache"
    );
    state.refresh_probe_reports(false);
    assert_eq!(
        state.probe_report_generation(),
        first,
        "a collapsed section never samples"
    );

    state
        .update_probe_surface(0, sphere(Vec3::new(0.25, 0.0, 0.0), 1.0, 16))
        .expect("valid update");
    state.refresh_probe_reports(true);
    let after_geometry = state.probe_report_generation();
    assert!(
        after_geometry > first,
        "changed geometry refreshes the report"
    );

    state.set_field_mode(FieldMode::Ode);
    state.refresh_probe_reports(true);
    assert!(
        state.probe_report_generation() > after_geometry,
        "a changed mode refreshes the report"
    );
}

#[test]
fn probe_wireframes_are_included_in_composed_capture_lines() {
    let mut state = State::new(true);
    state
        .add_probe_surface(sphere(Vec3::ZERO, 1.0, 16))
        .expect("valid surface");
    state
        .add_probe_loop(circle(Vec3::ZERO, 0.75, 16))
        .expect("valid loop");

    let overlay = state.probe_overlay_lines(&LayerStyles::default());
    assert!(
        !overlay.is_empty(),
        "a sphere and a circle emit wireframe segments"
    );
    assert!(
        overlay.iter().all(|line| line.a[3] == 1.0),
        "wireframe instances use the normal line start marker"
    );

    let (composed, _) = vector_field_explorer::app::compose_vector_lines(&state, &[], &[]);
    let plain = {
        let bare = State::new(true);
        vector_field_explorer::app::compose_vector_lines(&bare, &[], &[])
    };
    assert_eq!(
        composed.len(),
        plain.0.len() + overlay.len(),
        "the composed capture includes every probe wireframe segment"
    );
}

#[test]
fn sidebar_probe_rows_add_remove_and_show_honest_status() {
    struct Sidebar {
        ctx: egui::Context,
        state: State,
        controls: Controls,
        time: f64,
    }

    impl Sidebar {
        fn new(mode: FieldMode) -> Self {
            let ctx = support::context();
            ctx.enable_accesskit();
            let mut state = State::new(false);
            state.set_field_mode(mode);
            Self {
                ctx,
                state,
                controls: Controls::new(),
                time: 0.0,
            }
        }

        fn frame(&mut self, events: Vec<egui::Event>) -> egui::FullOutput {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::pos2(0.0, 0.0),
                    egui::vec2(600.0, 4000.0),
                )),
                time: Some(self.time),
                predicted_dt: 1.0 / 60.0,
                events,
                ..Default::default()
            };
            self.time += 1.0 / 60.0;
            let mut output = {
                let state = &mut self.state;
                let controls = &mut self.controls;
                self.ctx.run_ui(input, |ui| {
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        sidebar::show(
                            ui,
                            state,
                            controls,
                            &mut vector_field_explorer::seed_layout_library::SeedLayoutLibrary::default(),
                        );
                    });
                })
            };
            output.textures_delta.clear();
            output
        }

        fn idle(&mut self) -> egui::FullOutput {
            self.frame(Vec::new())
        }

        fn click(&mut self, pos: egui::Pos2) -> egui::FullOutput {
            self.frame(pointer(pos, true));
            self.frame(pointer(pos, false))
        }
    }

    #[derive(Clone)]
    struct Node {
        role: egui::Role,
        label: Option<String>,
        value: Option<String>,
        rect: egui::Rect,
    }

    impl Node {
        fn center(&self) -> egui::Pos2 {
            self.rect.center()
        }
    }

    fn pointer(pos: egui::Pos2, pressed: bool) -> Vec<egui::Event> {
        vec![
            egui::Event::PointerMoved(pos),
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            },
        ]
    }

    fn nodes(output: &egui::FullOutput) -> Vec<Node> {
        let Some(update) = output.platform_output.accesskit_update.as_ref() else {
            return Vec::new();
        };
        update
            .nodes
            .iter()
            .filter_map(|(_, node)| {
                let bounds = node.bounds()?;
                Some(Node {
                    role: node.role(),
                    label: node.label().map(str::to_owned),
                    value: node.value().map(str::to_owned),
                    rect: egui::Rect::from_min_max(
                        egui::pos2(bounds.x0 as f32, bounds.y0 as f32),
                        egui::pos2(bounds.x1 as f32, bounds.y1 as f32),
                    ),
                })
            })
            .collect()
    }

    fn open_probes(s: &mut Sidebar) -> egui::FullOutput {
        let out = s.idle();
        let nodes = nodes(&out);
        let seen: Vec<_> = nodes
            .iter()
            .filter_map(|node| node.value.clone().or_else(|| node.label.clone()))
            .collect();
        let label = nodes
            .iter()
            .find(|node| {
                node.value.as_deref() == Some("Field probes")
                    || node.label.as_deref() == Some("Field probes")
            })
            .unwrap_or_else(|| panic!("the Field probes label is exposed; saw {seen:?}"));
        s.click(label.center());
        for _ in 0..20 {
            s.idle();
        }
        s.idle()
    }

    let mut s = Sidebar::new(FieldMode::Sources);
    let out = open_probes(&mut s);
    let rows = nodes(&out);
    let add = rows
        .iter()
        .find(|node| {
            node.role == egui::Role::Button
                && node
                    .label
                    .as_deref()
                    .is_some_and(|label| label == "Add sphere")
        })
        .expect("the Field probes section offers an Add sphere row");
    s.click(add.center());
    assert_eq!(
        s.state.probe_surfaces().len(),
        1,
        "the row adds one surface through the model seam"
    );
    // Let the now-visible section run once more so the cache sees the new
    // geometry: a missing report is not evidence of an honest status.
    s.idle();
    let report = s
        .state
        .probe_surface_report(0)
        .expect("the visible section builds a report for the added surface");
    match &report.status {
        IntegralStatus::Unresolved { reason } | IntegralStatus::Singular { reason } => {
            assert!(
                reason.to_lowercase().contains("empty"),
                "the empty Sources scene explains itself: {reason}"
            );
        }
        other => panic!("the empty Sources scene must not claim a number: {other:?}"),
    }

    let rows = nodes(&s.idle());
    let add_y = rows
        .iter()
        .find(|node| node.label.as_deref() == Some("Add sphere"))
        .expect("Add sphere is still exposed")
        .rect
        .min
        .y;
    let remove = rows
        .iter()
        .filter(|node| {
            node.role == egui::Role::Button
                && node.label.as_deref() == Some("Remove")
                && node.rect.center().y > add_y
        })
        .max_by(|a, b| a.rect.center().y.total_cmp(&b.rect.center().y))
        .expect("the probe row exposes Remove below the Add row");
    s.click(remove.center());
    assert!(
        s.state.probe_surfaces().is_empty(),
        "Remove deletes the probe row"
    );

    s.state.set_field_mode(FieldMode::Formula);
    let rows = nodes(&s.idle());
    assert!(
        rows.iter().any(|node| {
            node.role == egui::Role::Label
                && node
                    .value
                    .as_deref()
                    .is_some_and(|text| text.contains("Formula"))
        }),
        "an unsupported mode is disabled with a reason that names it"
    );
}
