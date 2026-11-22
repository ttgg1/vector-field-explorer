//! Issue #219: a self-contained guided Gauss-law investigation for the shipped
//! hollow-sphere Maxwell case.
//!
//! The activity is session-only UI state. It never mutates
//! [`crate::viewmodel::State::probe_surfaces`]; every reading is computed
//! through a local [`ProbeField`] adapter over the current [`MaxwellScene`] and
//! the shipped [`crate::field_probe::flux`] full/half-resolution quadrature.
//! All values are normalized teaching quantities, not SI measurements.

use std::ops::RangeInclusive;

use eframe::egui;
use glam::Vec3;

use crate::field_probe::{
    FluxReport, GaussianSurface, IntegralStatus, ProbeField, SurfaceKind, flux,
};
use crate::maxwell::{
    MAXWELL_INTERFACE_TOLERANCE_FACTOR, MaxwellCase, MaxwellError, MaxwellScene,
    hollow_sphere_interface_tolerance, maxwell_field,
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

/// Smallest radius either object accepts from the exact-entry control.
pub const GAUSS_SHELL_MIN_RADIUS: f64 = 0.01;
/// Largest radius the exact-entry controls use as their absolute cap. The
/// shell control additionally stops below the scene's `outer_radius`; see
/// [`shell_radius_range`].
pub const GAUSS_SHELL_MAX_RADIUS: f64 = 100.0;
/// How many activity-local radius changes the session keeps for undo.
pub const MAX_GAUSS_RADIUS_UNDO: usize = 16;
/// Scope line for the activity-local undo, distinct from document history.
pub const GAUSS_SHELL_LOCAL_UNDO_NOTE: &str = "This activity-local undo restores only a radius from this investigation; it is not the document history.";
/// The explicit undo label when the measuring radius was the last change.
pub const UNDO_MEASURING_RADIUS: &str = "Undo change measuring radius";
/// The explicit undo label when the shell radius was the last change.
pub const UNDO_SHELL_RADIUS: &str = "Undo change shell radius";

/// The exact-entry range advertised for the shell radius `R`.
///
/// The committed scene rejects `R >= outer_radius`, so the maximum stops one
/// finite interface tolerance below the current outer radius rather than
/// advertising a radius the ordinary Maxwell setter must refuse. `None` means
/// the scene has no valid shell radius above [`GAUSS_SHELL_MIN_RADIUS`]; the
/// panel then states the limit instead of offering an invalid control.
pub fn shell_radius_range(scene: &MaxwellScene) -> Option<RangeInclusive<f64>> {
    if !scene.outer_radius.is_finite() || scene.outer_radius <= GAUSS_SHELL_MIN_RADIUS {
        return None;
    }
    let margin = scene.outer_radius.max(1.0) * MAXWELL_INTERFACE_TOLERANCE_FACTOR;
    let max = (scene.outer_radius - margin).min(GAUSS_SHELL_MAX_RADIUS);
    (max >= GAUSS_SHELL_MIN_RADIUS).then_some(GAUSS_SHELL_MIN_RADIUS..=max)
}

/// The two physical objects the investigation lets the learner select.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GaussObject {
    /// The charged hollow sphere's own radius `R` ([`MaxwellScene::radius`]).
    Shell,
    /// The Gaussian measuring surface's radius `r`
    /// ([`GaussShellState::probe_radius`]).
    #[default]
    MeasuringSurface,
}

impl GaussObject {
    /// Both objects, in the order the list shows them.
    pub const ALL: [Self; 2] = [Self::Shell, Self::MeasuringSurface];

    /// The name shown in the viewport list and the selection summary.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Shell => "Shell",
            Self::MeasuringSurface => "Measuring surface",
        }
    }

    /// The radius symbol (`R` or `r`) shown beside the name.
    pub const fn symbol(self) -> &'static str {
        match self {
            Self::Shell => "R",
            Self::MeasuringSurface => "r",
        }
    }

    /// The undo label for this object's last change.
    pub const fn undo_label(self) -> &'static str {
        match self {
            Self::Shell => UNDO_SHELL_RADIUS,
            Self::MeasuringSurface => UNDO_MEASURING_RADIUS,
        }
    }
}

/// One bounded, session-only radius change kept by the activity.
#[derive(Debug, Clone, Copy, PartialEq)]
struct RadiusUndo {
    object: GaussObject,
    before: f64,
}

fn radius_in_range(value: f64) -> bool {
    value.is_finite() && (GAUSS_SHELL_MIN_RADIUS..=GAUSS_SHELL_MAX_RADIUS).contains(&value)
}

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
    /// The shared selection: both the viewport click and the list write here.
    pub selected: GaussObject,
    /// Which object a click/drag targets while `R` and `r` coincide.
    pub overlap_choice: GaussObject,
    /// A shell-radius edit waiting for the app to apply it to the committed
    /// scene through the ordinary Maxwell control seam.
    pending_shell_radius: Option<f64>,
    /// The last shell edit the committed scene rejected, shown inline.
    shell_edit_error: Option<String>,
    /// The radius a multi-frame drag started from, so one gesture is one undo.
    edit_baseline: Option<(GaussObject, f64)>,
    /// Bounded activity-local undo; never persisted and never document history.
    undo: Vec<RadiusUndo>,
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
        self.selected = GaussObject::default();
        self.overlap_choice = GaussObject::default();
        self.pending_shell_radius = None;
        self.shell_edit_error = None;
        self.edit_baseline = None;
        self.undo.clear();
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

    /// Share one selection between the viewport and the keyboard list.
    pub fn select(&mut self, object: GaussObject) {
        self.selected = object;
    }

    /// Whether the shell and the measuring surface coincide within the shipped
    /// interface tolerance, so a click or drag cannot tell them apart.
    pub fn objects_overlap(&self, scene: &MaxwellScene) -> bool {
        scene.case == MaxwellCase::HollowSphere
            && (self.probe_radius - scene.radius).abs()
                <= hollow_sphere_interface_tolerance(scene.radius)
    }

    /// What a drag or viewport click targets: the selected object, or the
    /// explicit overlap choice while `R` and `r` coincide. Never a silent guess.
    pub fn edit_target(&self, scene: &MaxwellScene) -> GaussObject {
        if self.objects_overlap(scene) {
            self.overlap_choice
        } else {
            self.selected
        }
    }

    /// Set `r` from an exact numeric entry. Invalid values are refused without
    /// touching either radius; a valid change records one activity-local undo.
    pub fn set_probe_radius(&mut self, value: f64) -> bool {
        if !radius_in_range(value) {
            return false;
        }
        if value.to_bits() == self.probe_radius.to_bits() {
            return true;
        }
        self.push_undo(GaussObject::MeasuringSurface, self.probe_radius);
        self.probe_radius = value;
        self.invalidate_report();
        true
    }

    /// Double `r`, leaving the shell `R` untouched; the dependent readings
    /// follow through the ordinary [`Self::report_for`] cache.
    pub fn double_probe_radius(&mut self) -> bool {
        self.set_probe_radius(self.probe_radius * 2.0)
    }

    /// Ask the app to apply a shell radius through the ordinary Maxwell scene
    /// control. The request is validated here and consumed by the app handler.
    pub fn request_shell_radius(&mut self, value: f64) -> bool {
        if !radius_in_range(value) {
            return false;
        }
        if self.pending_shell_radius == Some(value) {
            return true;
        }
        self.pending_shell_radius = Some(value);
        self.shell_edit_error = None;
        self.invalidate_report();
        true
    }

    /// Preview a shell drag without recording a second undo entry; the gesture
    /// is committed by [`Self::end_edit`] when the pointer or focus is released.
    pub fn preview_shell_radius(&mut self, value: f64) -> bool {
        if !radius_in_range(value) {
            return false;
        }
        self.pending_shell_radius = Some(value);
        self.shell_edit_error = None;
        self.invalidate_report();
        true
    }

    /// The pending shell-radius request, if any. The app's #229 handler takes
    /// it and applies it through [`crate::viewmodel::State::set_maxwell_scene`].
    pub fn take_pending_shell_radius(&mut self) -> Option<f64> {
        self.pending_shell_radius.take()
    }

    /// The pending shell radius without consuming it, for the panel display.
    pub fn pending_shell_radius(&self) -> Option<f64> {
        self.pending_shell_radius
    }

    /// Record the committed scene's rejection of a shell edit.
    pub fn note_shell_edit_error(&mut self, message: String) {
        self.shell_edit_error = Some(message);
    }

    /// The last shell-edit rejection, if any.
    pub fn shell_edit_error(&self) -> Option<&str> {
        self.shell_edit_error.as_deref()
    }

    /// Start one drag/text gesture at `before`; repeated calls keep the first
    /// baseline so a multi-frame drag stays one undo entry.
    pub fn begin_edit(&mut self, object: GaussObject, before: f64) {
        if self.edit_baseline.is_none() {
            self.edit_baseline = Some((object, before));
        }
    }

    /// Commit one drag/text gesture as a single bounded undo entry.
    pub fn end_edit(&mut self, object: GaussObject) {
        let Some((baseline_object, before)) = self.edit_baseline.take() else {
            return;
        };
        if baseline_object != object {
            return;
        }
        let after = match object {
            GaussObject::Shell => self.pending_shell_radius.unwrap_or(before),
            GaussObject::MeasuringSurface => self.probe_radius,
        };
        if after.to_bits() != before.to_bits() {
            self.push_undo(object, before);
        }
    }

    /// The label of the last activity-local change, or `None` when the stack is
    /// empty. Wording is explicit that this is not document history.
    pub fn undo_label(&self) -> Option<&'static str> {
        self.undo.last().map(|entry| entry.object.undo_label())
    }

    /// Restore one radius from the activity-local stack. Returns the object that
    /// was restored. A shell restore becomes a pending app request.
    pub fn undo_last(&mut self) -> Option<GaussObject> {
        let entry = self.undo.pop()?;
        match entry.object {
            GaussObject::MeasuringSurface => {
                self.probe_radius = entry.before;
            }
            GaussObject::Shell => {
                self.pending_shell_radius = Some(entry.before);
            }
        }
        self.invalidate_report();
        Some(entry.object)
    }

    fn push_undo(&mut self, object: GaussObject, before: f64) {
        if self.undo.len() >= MAX_GAUSS_RADIUS_UNDO {
            self.undo.remove(0);
        }
        self.undo.push(RadiusUndo { object, before });
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
    ui.separator();
    ui.label("Select an object (or click its outline in the viewport)");
    let mut selected = state.selected;
    for object in GaussObject::ALL {
        ui.radio_value(
            &mut selected,
            object,
            format!("{} ({})", object.label(), object.symbol()),
        )
        .on_hover_text("Tab to the list, then Space or Enter to select");
    }
    if selected != state.selected {
        state.select(selected);
    }

    let overlapping = state.objects_overlap(scene);
    if overlapping {
        ui.colored_label(
            ui.visuals().warn_fg_color,
            "The shell and measuring surface coincide within the interface tolerance.",
        );
        ui.label("Which object should a drag or viewport click target?");
        let mut choice = state.overlap_choice;
        ui.radio_value(&mut choice, GaussObject::Shell, "Target shell (R)");
        ui.radio_value(
            &mut choice,
            GaussObject::MeasuringSurface,
            "Target measuring surface (r)",
        );
        state.overlap_choice = choice;
    }

    let selected_value = match state.selected {
        GaussObject::Shell => scene.radius,
        GaussObject::MeasuringSurface => state.probe_radius,
    };
    ui.strong(format!(
        "Selected: {} ({} = {:.3})",
        state.selected.label(),
        state.selected.symbol(),
        selected_value
    ));

    let target = state.edit_target(scene);
    if target != state.selected {
        ui.label(format!(
            "Edit target (overlap selector): {} ({})",
            target.label(),
            target.symbol()
        ));
    }
    match target {
        GaussObject::Shell => {
            let before = scene.radius;
            let mut value = state.pending_shell_radius().unwrap_or(before);
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.label("Shell radius R");
                if let Some(range) = shell_radius_range(scene) {
                    let response = ui
                        .add(egui::DragValue::new(&mut value).speed(0.05).range(range))
                        .on_hover_text("Drag handle, or type the charged shell's exact radius R");
                    if response.drag_started() || response.changed() {
                        state.begin_edit(GaussObject::Shell, before);
                    }
                    if response.changed() {
                        state.preview_shell_radius(value);
                    }
                    if response.drag_stopped() || response.lost_focus() {
                        state.end_edit(GaussObject::Shell);
                    }
                } else {
                    ui.colored_label(
                        ui.visuals().error_fg_color,
                        "No shell radius above 0.01 fits below this scene's outer radius.",
                    );
                }
                ui.small(format!(
                    "Shell R must stay below the outer radius {:.3}.",
                    scene.outer_radius
                ));
                if let Some(error) = state.shell_edit_error() {
                    ui.colored_label(ui.visuals().error_fg_color, error);
                }
                ui.label(format!(
                    "Measuring surface r = {:.3} (unchanged)",
                    state.probe_radius
                ));
            });
        }
        GaussObject::MeasuringSurface => {
            let before = state.probe_radius;
            let mut value = before;
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.label("Probe radius r");
                let response = ui
                    .add(
                        egui::DragValue::new(&mut value)
                            .speed(0.05)
                            .range(GAUSS_SHELL_MIN_RADIUS..=GAUSS_SHELL_MAX_RADIUS),
                    )
                    .on_hover_text("Gaussian-surface radius r, separate from the shell radius R");
                if response.drag_started() || response.changed() {
                    state.begin_edit(GaussObject::MeasuringSurface, before);
                }
                if response.changed() && radius_in_range(value) {
                    state.probe_radius = value;
                    state.invalidate_report();
                }
                if response.drag_stopped() || response.lost_focus() {
                    state.end_edit(GaussObject::MeasuringSurface);
                }
                ui.label(format!(
                    "Shell R = {:.3} (unchanged), 2r = {:.3}",
                    scene.radius,
                    state.probe_radius * 2.0
                ));
            });
        }
    }

    ui.horizontal_wrapped(|ui| {
        if ui
            .button("Double measuring radius")
            .on_hover_text("Set r = 2r; the shell R and unrelated state stay fixed")
            .clicked()
        {
            state.double_probe_radius();
        }
        if let Some(label) = state.undo_label()
            && ui
                .button(label)
                .on_hover_text(GAUSS_SHELL_LOCAL_UNDO_NOTE)
                .clicked()
        {
            state.undo_last();
        }
    });
    ui.small(GAUSS_SHELL_LOCAL_UNDO_NOTE);

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
