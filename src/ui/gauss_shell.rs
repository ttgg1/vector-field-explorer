//! Issue #219: a self-contained guided Gauss-law investigation for the shipped
//! hollow-sphere Maxwell case.
//!
//! The activity is session-only UI state. It never mutates
//! [`crate::viewmodel::State::probe_surfaces`]; every reading is computed
//! through a local [`ProbeField`] adapter over the current [`MaxwellScene`] and
//! the shipped [`crate::field_probe::flux`] full/half-resolution quadrature.
//! All values are normalized teaching quantities, not SI measurements.

use eframe::egui;
use glam::Vec3;

use crate::field_probe::{
    flux, FluxReport, GaussianSurface, IntegralStatus, ProbeField, SurfaceKind,
};
use crate::maxwell::{
    hollow_sphere_interface_tolerance, maxwell_field, MaxwellCase, MaxwellError, MaxwellScene,
};
use crate::sources::FieldFamily;

/// The explicit physics question that opens the investigation.
pub const GAUSS_SHELL_QUESTION: &str = "A charged spherical shell has radius R. If the Gaussian radius doubles, why does the flux stay the same?";

/// Scope line shown above every guided reading.
pub const GAUSS_SHELL_NORMALIZED_NOTE: &str = "Normalized teaching units sampled from the shipped Hollow sphere case; these are not SI measurements.";

/// Why the interior reports zero rather than an error.
pub const GAUSS_SHELL_INSIDE_NOTE: &str = "Inside the shell (r < R) the Gaussian surface encloses no net charge, so the radial field and the flux are zero.";

/// Why the ideal interface is excluded instead of returning a finite number.
pub const GAUSS_SHELL_INTERFACE_NOTE: &str = "At the ideal interface (r = R within the shipped case tolerance) the analytic field is discontinuous, so this sample is excluded and no finite field or flux is reported.";

/// Notes that the cancellation is a sampled estimate, not an exact identity.
pub const GAUSS_SHELL_ALGEBRA_NOTE: &str = "The products agree only within the reported half-resolution quadrature uncertainty; this is a sampled estimate, not an exact symbolic cancellation.";

/// Sphere subdivision count for the investigation. It matches the shipped probe
/// default and stays inside [`crate::field_probe::MAX_PROBE_SAMPLES`].
pub const GAUSS_SHELL_SEGMENTS: u32 = 16;

/// The investigation's one-shot request to the app.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GaussShellAction {
    /// Load the Hollow sphere case and open the guided readings.
    Investigate,
    /// Load the Hollow sphere case but leave the guided steps inactive.
    FreeExplore,
}

/// One reading's honest classification.
#[derive(Debug, Clone, PartialEq)]
pub enum GaussShellReadingKind {
    /// `r > R`: a finite radial field, area and sampled flux.
    Exterior {
        field: f64,
        area: f64,
        flux: FluxReport,
    },
    /// `r < R`: the field and flux vanish; the area is still real geometry.
    Interior { area: f64, flux: FluxReport },
    /// The ideal interface or an unavailable scene: never a finite reading.
    Excluded { reason: String },
}

/// One radius reading at the Gaussian radius `r`.
#[derive(Debug, Clone, PartialEq)]
pub struct GaussShellReading {
    pub radius: f64,
    pub kind: GaussShellReadingKind,
}

impl GaussShellReading {
    /// Radial field magnitude, or `None` for an excluded sample.
    pub fn field(&self) -> Option<f64> {
        match &self.kind {
            GaussShellReadingKind::Exterior { field, .. } => Some(*field),
            GaussShellReadingKind::Interior { .. } => Some(0.0),
            GaussShellReadingKind::Excluded { .. } => None,
        }
    }

    /// Analytic surface area, or `None` for an excluded sample.
    pub fn area(&self) -> Option<f64> {
        match &self.kind {
            GaussShellReadingKind::Exterior { area, .. }
            | GaussShellReadingKind::Interior { area, .. } => Some(*area),
            GaussShellReadingKind::Excluded { .. } => None,
        }
    }

    /// The shipped flux report, or `None` for an excluded sample.
    pub fn flux(&self) -> Option<&FluxReport> {
        match &self.kind {
            GaussShellReadingKind::Exterior { flux, .. }
            | GaussShellReadingKind::Interior { flux, .. } => Some(flux),
            GaussShellReadingKind::Excluded { .. } => None,
        }
    }

    /// The completed full/half-resolution estimate and its uncertainty.
    pub fn flux_estimate(&self) -> Option<(f64, f64)> {
        match self.flux().map(|report| &report.status) {
            Some(IntegralStatus::Estimate { value, error }) => Some((*value, *error)),
            _ => None,
        }
    }
}

/// The two live readings at `r` and `2r`, cached against their exact inputs.
#[derive(Debug, Clone, PartialEq)]
pub struct GaussShellReport {
    pub radius: f64,
    pub at_radius: GaussShellReading,
    pub at_double: GaussShellReading,
}

/// Local, never-stored field adapter over the current Maxwell scene.
struct MaxwellProbe<'a> {
    scene: &'a MaxwellScene,
}

impl ProbeField for MaxwellProbe<'_> {
    fn eval(&self, point: Vec3, _time: f64) -> Option<Vec3> {
        maxwell_field(self.scene, point).ok()
    }
}

/// Compute one honest reading at `radius`. This is the pure seam shared by the
/// panel and the tests.
pub fn compute_reading(
    scene: &MaxwellScene,
    radius: f64,
    family: FieldFamily,
    time: f64,
) -> GaussShellReading {
    if scene.case != MaxwellCase::HollowSphere {
        return GaussShellReading {
            radius,
            kind: GaussShellReadingKind::Excluded {
                reason:
                    "The guided Gauss-shell investigation is defined for the Hollow sphere case."
                        .to_owned(),
            },
        };
    }

    let tolerance = hollow_sphere_interface_tolerance(scene.radius);
    if (radius - scene.radius).abs() <= tolerance {
        return GaussShellReading {
            radius,
            kind: GaussShellReadingKind::Excluded {
                reason: GAUSS_SHELL_INTERFACE_NOTE.to_owned(),
            },
        };
    }

    let surface = GaussianSurface {
        kind: SurfaceKind::Sphere,
        center: Vec3::ZERO,
        axis: Vec3::Z,
        size: Vec3::splat((2.0 * radius) as f32),
        segments: GAUSS_SHELL_SEGMENTS,
    };
    let area = surface.area();
    let probe = MaxwellProbe { scene };
    let report = flux(&probe, &surface, family, time);

    if radius < scene.radius {
        return GaussShellReading {
            radius,
            kind: GaussShellReadingKind::Interior { area, flux: report },
        };
    }

    let point = Vec3::new(radius as f32, 0.0, 0.0);
    match maxwell_field(scene, point) {
        Ok(field) if field.is_finite() => GaussShellReading {
            radius,
            kind: GaussShellReadingKind::Exterior {
                field: f64::from(field.length()),
                area,
                flux: report,
            },
        },
        Ok(_) => excluded(radius, "The radial field sample was not finite."),
        Err(error) => excluded(radius, &maxwell_error_text(&error)),
    }
}

fn excluded(radius: f64, reason: &str) -> GaussShellReading {
    GaussShellReading {
        radius,
        kind: GaussShellReadingKind::Excluded {
            reason: reason.to_owned(),
        },
    }
}

fn maxwell_error_text(error: &MaxwellError) -> String {
    match error {
        MaxwellError::InvalidScene(text)
        | MaxwellError::Unsupported(text)
        | MaxwellError::Unavailable(text) => text.clone(),
    }
}

/// Session-only guided-investigation state. `Default` is inactive.
#[derive(Debug, Clone, Default)]
pub struct GaussShellState {
    pub active: bool,
    pub probe_radius: f64,
    pinned: Vec<GaussShellReading>,
    report: Option<GaussShellReport>,
    report_scene: Option<MaxwellScene>,
    report_family: Option<FieldFamily>,
    report_radius: f64,
    report_time: f64,
}

impl GaussShellState {
    /// Activate the investigation for `scene`, choosing a first exterior radius.
    /// Starts a fresh session: pinned readings and cached reports are cleared.
    pub fn activate(&mut self, scene: &MaxwellScene) {
        self.active = true;
        self.pinned.clear();
        self.invalidate_report();
        self.probe_radius = if scene.radius.is_finite() && scene.radius > 0.0 {
            (scene.radius * 1.5).clamp(0.01, 100.0)
        } else {
            1.5
        };
    }

    /// Leave the guided steps. The caller keeps the loaded Hollow sphere case.
    pub fn deactivate(&mut self) {
        *self = Self::default();
    }

    /// Return to the inactive default session.
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Drop a cached report without changing the active selection or pins.
    pub fn invalidate_report(&mut self) {
        self.report = None;
        self.report_scene = None;
        self.report_family = None;
        self.report_radius = 0.0;
        self.report_time = 0.0;
    }

    /// The live `r` and `2r` readings, recomputed only when an input changes.
    pub fn report_for(
        &mut self,
        scene: &MaxwellScene,
        family: FieldFamily,
        time: f64,
    ) -> &GaussShellReport {
        let fresh = self.report.is_some()
            && self.report_scene.as_ref() == Some(scene)
            && self.report_family == Some(family)
            && self.report_radius.to_bits() == self.probe_radius.to_bits()
            && self.report_time.to_bits() == time.to_bits();
        if !fresh {
            let radius = self.probe_radius;
            self.report = Some(GaussShellReport {
                radius,
                at_radius: compute_reading(scene, radius, family, time),
                at_double: compute_reading(scene, radius * 2.0, family, time),
            });
            self.report_scene = Some(scene.clone());
            self.report_family = Some(family);
            self.report_radius = radius;
            self.report_time = time;
        }
        self.report.as_ref().expect("report was just built")
    }

    /// Snapshot one reading. At most two are pinned; a third replaces the
    /// oldest so the two cards stay visible.
    pub fn pin(&mut self, reading: GaussShellReading) {
        if self.pinned.len() >= 2 {
            self.pinned.remove(0);
        }
        self.pinned.push(reading);
    }

    /// The pinned readings in pin order.
    pub fn pinned(&self) -> &[GaussShellReading] {
        &self.pinned
    }

    /// Remove both pinned readings.
    pub fn clear_pinned(&mut self) {
        self.pinned.clear();
    }
}

/// Draw the investigation. Inactive state offers the question plus
/// `Investigate` / `Free explore`; active state draws the radius control, the
/// `r` and `2r` readings, pin controls, ratio/algebra evidence and the
/// inside/interface states.
pub fn show(
    ui: &mut egui::Ui,
    state: &mut GaussShellState,
    scene: &MaxwellScene,
    family: FieldFamily,
    time: f64,
) -> Option<GaussShellAction> {
    ui.separator();
    ui.heading("Gauss's law investigation");
    ui.label(GAUSS_SHELL_QUESTION);

    if !state.active {
        let mut action = None;
        ui.horizontal_wrapped(|ui| {
            if ui
                .button("Investigate")
                .on_hover_text("Load the hollow sphere and open the guided readings")
                .clicked()
            {
                action = Some(GaussShellAction::Investigate);
            }
            if ui
                .button("Free explore")
                .on_hover_text("Load the hollow sphere without forcing the guided steps")
                .clicked()
            {
                action = Some(GaussShellAction::FreeExplore);
            }
        });
        return action;
    }

    ui.label(GAUSS_SHELL_NORMALIZED_NOTE);
    ui.horizontal_wrapped(|ui| {
        ui.label("Probe radius r");
        let mut radius = state.probe_radius;
        if ui
            .add(
                egui::DragValue::new(&mut radius)
                    .speed(0.05)
                    .range(0.01..=100.0),
            )
            .on_hover_text("Gaussian-surface radius r, separate from the shell radius R")
            .changed()
        {
            state.probe_radius = radius;
        }
        ui.label(format!(
            "(shell R = {:.3}, 2r = {:.3})",
            scene.radius,
            state.probe_radius * 2.0
        ));
    });

    let report = state.report_for(scene, family, time).clone();
    let pin_radius = reading_card(ui, "At r", &report.at_radius, "Pin r reading");
    let pin_double = reading_card(ui, "At 2r", &report.at_double, "Pin 2r reading");
    if pin_radius {
        state.pin(report.at_radius.clone());
    }
    if pin_double {
        state.pin(report.at_double.clone());
    }

    show_ratios(ui, &report);
    show_algebra(ui, &report.at_radius, &report.at_double);

    if !state.pinned.is_empty() {
        ui.separator();
        ui.label("Pinned readings (snapshot at pin time)");
        for reading in state.pinned.iter() {
            pinned_card(ui, reading);
        }
        if state.pinned().len() == 2 {
            show_pinned_algebra(ui, state.pinned());
        }
        if ui.button("Clear pinned readings").clicked() {
            state.clear_pinned();
        }
    }

    let mut action = None;
    ui.horizontal_wrapped(|ui| {
        if ui
            .button("Free explore")
            .on_hover_text("Leave the guided steps; the Hollow sphere case stays loaded")
            .clicked()
        {
            action = Some(GaussShellAction::FreeExplore);
        }
    });
    action
}

fn reading_card(
    ui: &mut egui::Ui,
    title: &str,
    reading: &GaussShellReading,
    pin_label: &str,
) -> bool {
    let mut pinned = false;
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.label(format!("{title} = {:.4}", reading.radius));
        match &reading.kind {
            GaussShellReadingKind::Exterior { field, area, flux } => {
                ui.label(format!("Radial field E = {field:.6}"));
                ui.label(format!("Surface area A = {area:.6}"));
                ui.label(format!("Flux Phi = {}", flux_text(flux)));
            }
            GaussShellReadingKind::Interior { area, flux } => {
                ui.label(GAUSS_SHELL_INSIDE_NOTE);
                ui.label("Radial field E = 0.000000");
                ui.label(format!("Surface area A = {area:.6}"));
                ui.label(format!("Flux Phi = {}", flux_text(flux)));
            }
            GaussShellReadingKind::Excluded { reason } => {
                ui.colored_label(
                    ui.visuals().warn_fg_color,
                    format!("Excluded sample: {reason}"),
                );
            }
        }
        pinned = ui.button(pin_label).clicked();
    });
    pinned
}

fn pinned_card(ui: &mut egui::Ui, reading: &GaussShellReading) {
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.label(format!("Pinned r = {:.4}", reading.radius));
        match &reading.kind {
            GaussShellReadingKind::Exterior { field, area, flux } => {
                ui.label(format!(
                    "E = {field:.6} · A = {area:.6} · E*A = {:.6}",
                    field * area
                ));
                ui.label(format!("Flux Phi = {}", flux_text(flux)));
            }
            GaussShellReadingKind::Interior { area, flux } => {
                ui.label(format!("E = 0.000000 · A = {area:.6} · E*A = 0.000000"));
                ui.label(format!("Flux Phi = {}", flux_text(flux)));
            }
            GaussShellReadingKind::Excluded { reason } => {
                ui.colored_label(
                    ui.visuals().warn_fg_color,
                    format!("Excluded sample: {reason}"),
                );
            }
        }
    });
}

fn show_ratios(ui: &mut egui::Ui, report: &GaussShellReport) {
    let Some(first_field) = report.at_radius.field() else {
        return;
    };
    let Some(second_field) = report.at_double.field() else {
        return;
    };
    let (Some(first_area), Some(second_area)) = (report.at_radius.area(), report.at_double.area())
    else {
        return;
    };
    if second_field > 0.0 && first_area > 0.0 {
        ui.label(format!(
            "E(r) / E(2r) = {:.4} (doubling r quarters the field)",
            first_field / second_field
        ));
        ui.label(format!(
            "A(2r) / A(r) = {:.4} (doubling r quadruples the area)",
            second_area / first_area
        ));
    }
    if let (Some((first_flux, first_error)), Some((second_flux, second_error))) = (
        report.at_radius.flux_estimate(),
        report.at_double.flux_estimate(),
    ) {
        let difference = (first_flux - second_flux).abs();
        let uncertainty = first_error + second_error;
        ui.label(format!(
            "|Phi(r) - Phi(2r)| = {difference:.3e}; combined quadrature uncertainty = \
             {uncertainty:.3e}; equal within uncertainty: {}",
            if difference <= uncertainty + 1e-9 {
                "yes"
            } else {
                "no"
            }
        ));
    }
}

fn show_algebra(ui: &mut egui::Ui, first: &GaussShellReading, second: &GaussShellReading) {
    let (
        GaussShellReadingKind::Exterior {
            field: first_field,
            area: first_area,
            ..
        },
        GaussShellReadingKind::Exterior {
            field: second_field,
            area: second_area,
            ..
        },
    ) = (&first.kind, &second.kind)
    else {
        return;
    };
    ui.label(GAUSS_SHELL_ALGEBRA_NOTE);
    ui.label(format!(
        "E(r) * A(r) = {first_field:.6} * {first_area:.6} = {:.6}",
        first_field * first_area
    ));
    ui.label(format!(
        "E(2r) * A(2r) = {second_field:.6} * {second_area:.6} = {:.6}",
        second_field * second_area
    ));
    if let (Some((first_flux, first_error)), Some((second_flux, second_error))) =
        (first.flux_estimate(), second.flux_estimate())
    {
        ui.label(format!("Phi(r) = {first_flux:.6} +/- {first_error:.2e}"));
        ui.label(format!("Phi(2r) = {second_flux:.6} +/- {second_error:.2e}"));
    }
}

fn show_pinned_algebra(ui: &mut egui::Ui, pinned: &[GaussShellReading]) {
    let [first, second] = pinned else {
        return;
    };
    let (
        GaussShellReadingKind::Exterior {
            field: first_field,
            area: first_area,
            ..
        },
        GaussShellReadingKind::Exterior {
            field: second_field,
            area: second_area,
            ..
        },
    ) = (&first.kind, &second.kind)
    else {
        return;
    };
    ui.label("Pinned cancellation in normalized units");
    ui.label(format!(
        "E * A = {first_field:.6} * {first_area:.6} = {:.6} (r = {:.4})",
        first_field * first_area,
        first.radius
    ));
    ui.label(format!(
        "E * A = {second_field:.6} * {second_area:.6} = {:.6} (r = {:.4})",
        second_field * second_area,
        second.radius
    ));
    if let (Some((first_flux, first_error)), Some((second_flux, second_error))) =
        (first.flux_estimate(), second.flux_estimate())
    {
        ui.label(format!("Phi = {first_flux:.6} +/- {first_error:.2e}"));
        ui.label(format!("Phi = {second_flux:.6} +/- {second_error:.2e}"));
    }
}

fn flux_text(report: &FluxReport) -> String {
    match &report.status {
        IntegralStatus::Estimate { value, error } => {
            format!("{value:.6} +/- {error:.2e} (quadrature)")
        }
        IntegralStatus::Unresolved { reason } => format!("unavailable ({reason})"),
        IntegralStatus::Singular { reason } => format!("singular ({reason})"),
    }
}
