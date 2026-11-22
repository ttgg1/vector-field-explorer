//! Issue #218: a session-only closed-box flux activity for the promoted
//! `Uniform field` catalogue prompt.
//!
//! The activity reads the formula already loaded in the scene. For a genuinely
//! constant three-component field it reports the six faces of a fixed closed
//! box, their outward normals, and each face's exact signed uniform-field flux
//! (`F · n` times the face area). It never adds a `State::probe_surfaces` entry,
//! saves anything, or changes the Formula-mode probe refusal.
//!
//! A genuinely constant two-component field has no 3D box flux of its own, so
//! the activity offers only the explicit extension `F=(Fx,Fy,0)`: it previews
//! the extension formula, switches through the existing formula/mode setters
//! only on confirmation, and keeps the original formula for an explicit
//! restore. Everything is normalized teaching units, not an SI calibration.

use eframe::egui;
use glam::DVec3;

use crate::field::FieldExpr;
use crate::viewmodel::State;

/// Heading of the activity.
pub const UNIFORM_FLUX_HEADING: &str = "Flux through a closed box";
/// The question the activity answers.
pub const UNIFORM_FLUX_QUESTION: &str =
    "Compare the flux entering and leaving a closed box in this uniform field.";
/// Generic name for the loaded formula field.
pub const UNIFORM_FLUX_FIELD_LABEL: &str = "F";
/// Explicit scope and unit statement.
pub const UNIFORM_FLUX_NORMALIZED_NOTE: &str = "Normalized teaching units: the field, box and face areas share one uncalibrated scale; these are not SI measurements.";
/// The generic field carries no electromagnetic role.
pub const UNIFORM_FLUX_GENERIC_ROLE_NOTE: &str =
    "F is the catalogue's generic formula field; no electric or magnetic role is assumed.";
/// The per-face reading is analytic for the uniform case.
pub const UNIFORM_FLUX_EXACT_NOTE: &str = "Exact uniform-field reading: each face's signed flux is F · n times that face's area; no sampled quadrature is used.";
/// Two components stay two components; only the explicit extension is 3D.
pub const UNIFORM_FLUX_TWO_COMPONENT_NOTE: &str = "A two-component field lies in the xy plane and has no 3D box flux of its own. The explicit 3D extension is F=(Fx,Fy,0).";
/// A varying formula gets no fabricated box table.
pub const UNIFORM_FLUX_NOT_UNIFORM_NOTE: &str =
    "This formula varies with position or time, so no uniform-field box flux is reported.";
/// The session-only contract.
pub const UNIFORM_FLUX_SESSION_NOTE: &str = "Session-only: no probe surface is created or saved, and the formula changes only when you confirm the 3D extension.";
/// The stated cancellation tolerance, in normalized units.
pub const UNIFORM_FLUX_TOLERANCE: f64 = 1e-9;
/// Half-extent of the displayed box, in normalized units.
pub const UNIFORM_FLUX_BOX_HALF_EXTENT: f64 = 1.0;

/// One face of the closed box.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UniformFluxFace {
    /// Face name, such as `-x`.
    pub name: &'static str,
    /// Unit outward normal.
    pub normal: [f64; 3],
    /// Face area.
    pub area: f64,
    /// Signed analytic flux through this face.
    pub flux: f64,
}

/// The exact uniform-field face table for one closed box.
#[derive(Debug, Clone, PartialEq)]
pub struct UniformBoxFlux {
    /// Loaded field components.
    pub field: [f64; 3],
    /// Box half-extent.
    pub half_extent: f64,
    /// The six faces, in outward-normal pairs.
    pub faces: [UniformFluxFace; 6],
    /// Signed flux total.
    pub total: f64,
    /// Stated cancellation tolerance.
    pub tolerance: f64,
}

impl UniformBoxFlux {
    /// Sum of the incoming (negative) face fluxes.
    pub fn incoming(&self) -> f64 {
        self.faces
            .iter()
            .filter(|face| face.flux < 0.0)
            .map(|face| face.flux)
            .sum()
    }

    /// Sum of the outgoing (positive) face fluxes.
    pub fn outgoing(&self) -> f64 {
        self.faces
            .iter()
            .filter(|face| face.flux > 0.0)
            .map(|face| face.flux)
            .sum()
    }

    /// Whether the total cancels within the stated tolerance.
    pub fn cancels(&self) -> bool {
        self.total.abs() <= self.tolerance
    }
}

/// The explicit 3D extension offered for a constant two-component field.
#[derive(Debug, Clone, PartialEq)]
pub struct UniformFluxExtension {
    /// The loaded two components.
    pub field: [f64; 2],
    /// The extension formula `(Fx, Fy, 0)`.
    pub extension_formula: String,
}

/// What the activity can honestly read from the loaded formula.
#[derive(Debug, Clone, PartialEq)]
pub enum UniformFluxReading {
    /// A genuinely constant three-component field: the exact box table.
    ThreeComponent(Box<UniformBoxFlux>),
    /// A genuinely constant two-component field: the explicit extension.
    TwoComponent(UniformFluxExtension),
    /// The formula varies with position or time; no box flux is offered.
    NotUniform,
}

/// Sample points for constant detection. They span all three axes so a field
/// that varies in `z` is never mistaken for a constant 2D field.
const CONSTANT_SAMPLES: [[f64; 3]; 8] = [
    [-1.0, -1.0, -1.0],
    [1.0, 1.0, 1.0],
    [-1.0, 0.5, 0.75],
    [0.75, -0.5, -0.25],
    [0.5, -0.25, 1.0],
    [-0.25, 0.75, 0.5],
    [1.0, 0.0, 0.0],
    [0.0, 1.0, 0.0],
];

/// Read the loaded formula without mutating anything.
pub fn analyze(field: &FieldExpr) -> UniformFluxReading {
    if field.is_time_dependent() {
        return UniformFluxReading::NotUniform;
    }
    let Some(reference) = field.eval3_components_at_f64(DVec3::ZERO, 0.0) else {
        return UniformFluxReading::NotUniform;
    };
    let matches_reference = |sample: [f64; 3]| {
        (0..field.dimension).all(|axis| {
            let scale = reference[axis].abs().max(1.0);
            (sample[axis] - reference[axis]).abs() <= UNIFORM_FLUX_TOLERANCE * scale
        })
    };
    for point in CONSTANT_SAMPLES {
        let Some(sample) = field.eval3_components_at_f64(DVec3::from_array(point), 0.0) else {
            return UniformFluxReading::NotUniform;
        };
        if !matches_reference(sample) {
            return UniformFluxReading::NotUniform;
        }
    }
    // A parser that missed a `t` reference still cannot pass the second clock
    // sample.
    let Some(later) = field.eval3_components_at_f64(DVec3::ZERO, 1.0) else {
        return UniformFluxReading::NotUniform;
    };
    if !matches_reference(later) {
        return UniformFluxReading::NotUniform;
    }

    match field.dimension {
        3 => UniformFluxReading::ThreeComponent(Box::new(box_flux(reference))),
        2 => UniformFluxReading::TwoComponent(UniformFluxExtension {
            field: [reference[0], reference[1]],
            extension_formula: extension_formula([reference[0], reference[1]]),
        }),
        _ => UniformFluxReading::NotUniform,
    }
}

/// The explicit `(Fx, Fy, 0)` extension formula for a two-component field.
pub fn extension_formula(field: [f64; 2]) -> String {
    format!(
        "({}, {}, 0)",
        format_component(field[0]),
        format_component(field[1])
    )
}

fn format_component(value: f64) -> String {
    if value == 0.0 {
        "0".to_owned()
    } else {
        format!("{value}")
    }
}

fn box_flux(field: [f64; 3]) -> UniformBoxFlux {
    let half_extent = UNIFORM_FLUX_BOX_HALF_EXTENT;
    let area = (2.0 * half_extent).powi(2);
    let faces = [
        face("-x", [-1.0, 0.0, 0.0], area, field),
        face("+x", [1.0, 0.0, 0.0], area, field),
        face("-y", [0.0, -1.0, 0.0], area, field),
        face("+y", [0.0, 1.0, 0.0], area, field),
        face("-z", [0.0, 0.0, -1.0], area, field),
        face("+z", [0.0, 0.0, 1.0], area, field),
    ];
    let total = faces.iter().map(|face| face.flux).sum();
    UniformBoxFlux {
        field,
        half_extent,
        faces,
        total,
        tolerance: UNIFORM_FLUX_TOLERANCE,
    }
}

fn face(name: &'static str, normal: [f64; 3], area: f64, field: [f64; 3]) -> UniformFluxFace {
    let component = normal[0] * field[0] + normal[1] * field[1] + normal[2] * field[2];
    UniformFluxFace {
        name,
        normal,
        area,
        flux: component * area,
    }
}

/// Where the original formula is restored from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UniformFluxRestore {
    /// The formula before the explicit 3D extension.
    pub formula: String,
    /// The projection before the explicit 3D extension.
    pub mode_3d: bool,
}

/// Session-only activity state. `Default` is closed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UniformFluxState {
    open: bool,
    preview: bool,
    restore: Option<UniformFluxRestore>,
}

impl UniformFluxState {
    /// Open the activity.
    pub fn open(&mut self) {
        self.open = true;
        self.preview = false;
    }

    /// Whether the activity is open.
    pub fn is_open(&self) -> bool {
        self.open
    }

    /// Close the activity without touching the loaded formula or the restore
    /// point.
    pub fn close(&mut self) {
        self.open = false;
        self.preview = false;
    }

    /// Whether the extension preview is expanded.
    pub fn is_previewing(&self) -> bool {
        self.preview
    }

    /// Expand the explicit 3D extension preview without switching.
    pub fn preview_extension(&mut self) {
        self.preview = self.open;
    }

    /// Collapse the preview without switching.
    pub fn cancel_preview(&mut self) {
        self.preview = false;
    }

    /// The restore point captured by the last explicit extension.
    pub fn restore_point(&self) -> Option<&UniformFluxRestore> {
        self.restore.as_ref()
    }

    /// Whether a restore point is held.
    pub fn restore_available(&self) -> bool {
        self.restore.is_some()
    }

    /// Load the extension formula and switch to 3D, keeping the original.
    pub fn apply_extension(&mut self, state: &mut State, extension: &str) -> bool {
        if !self.open || extension.is_empty() || state.formula == extension {
            return false;
        }
        self.restore = Some(UniformFluxRestore {
            formula: state.formula.clone(),
            mode_3d: state.mode_3d,
        });
        state.set_formula(extension.to_owned());
        state.set_mode(true);
        self.preview = false;
        true
    }

    /// Restore the original formula and projection.
    pub fn restore_original(&mut self, state: &mut State) -> bool {
        let Some(restore) = self.restore.take() else {
            return false;
        };
        state.set_formula(restore.formula);
        state.set_mode(restore.mode_3d);
        true
    }
}

/// Draw the activity. It renders only while open; closing it leaves the loaded
/// formula untouched.
pub fn show(ui: &mut egui::Ui, state: &mut State, activity: &mut UniformFluxState) {
    if !activity.is_open() {
        return;
    }
    ui.separator();
    ui.heading(UNIFORM_FLUX_HEADING);
    ui.label(UNIFORM_FLUX_QUESTION);
    ui.label(UNIFORM_FLUX_NORMALIZED_NOTE);
    ui.label(format!("Field {UNIFORM_FLUX_FIELD_LABEL}"));
    ui.weak(UNIFORM_FLUX_GENERIC_ROLE_NOTE);
    ui.label(format!("Loaded formula: {}", state.formula));
    ui.weak(UNIFORM_FLUX_SESSION_NOTE);

    if !state.parse_error.is_empty() {
        ui.colored_label(
            ui.visuals().warn_fg_color,
            format!(
                "The loaded formula does not parse, so no box flux is reported: {}",
                state.parse_error
            ),
        );
    } else {
        match analyze(&state.field) {
            UniformFluxReading::ThreeComponent(flux) => show_box(ui, &flux),
            UniformFluxReading::TwoComponent(extension) => {
                show_extension(ui, state, activity, &extension);
            }
            UniformFluxReading::NotUniform => {
                ui.colored_label(ui.visuals().warn_fg_color, UNIFORM_FLUX_NOT_UNIFORM_NOTE);
            }
        }
    }

    if let Some(point) = activity.restore_point().cloned() {
        ui.separator();
        ui.label(format!(
            "Original formula kept for restore: {} ({})",
            point.formula,
            if point.mode_3d { "3D" } else { "2D" }
        ));
    }
    if activity.restore_available() && ui.button("Restore original formula").clicked() {
        activity.restore_original(state);
    }

    if ui
        .button("Close investigation")
        .on_hover_text("Leave the activity; the loaded formula stays as it is")
        .clicked()
    {
        activity.close();
    }
}

fn show_box(ui: &mut egui::Ui, flux: &UniformBoxFlux) {
    ui.label(UNIFORM_FLUX_EXACT_NOTE);
    ui.label(format!(
        "Box centred on the origin, half-extent {:.0}; every face has area {:.0}.",
        flux.half_extent, flux.faces[0].area
    ));
    for face in &flux.faces {
        let direction = if face.flux > 0.0 {
            "outgoing"
        } else if face.flux < 0.0 {
            "incoming"
        } else {
            "zero"
        };
        ui.label(format!(
            "Face {} · outward normal ({}, {}, {}) · area {:.0} · signed flux {:+.3} ({direction})",
            face.name, face.normal[0], face.normal[1], face.normal[2], face.area, face.flux
        ));
    }
    ui.label(format!("Incoming total: {:+.6}", flux.incoming()));
    ui.label(format!("Outgoing total: {:+.6}", flux.outgoing()));
    ui.label(format!(
        "Total flux: {:+.6e} · cancellation tolerance {:.0e} in normalized units",
        flux.total, flux.tolerance
    ));
    if flux.cancels() {
        ui.label(format!(
            "Incoming and outgoing contributions cancel within the stated {:.0e} tolerance.",
            flux.tolerance
        ));
    } else {
        ui.colored_label(
            ui.visuals().warn_fg_color,
            format!(
                "The incoming and outgoing contributions do not cancel within the stated {:.0e} tolerance.",
                flux.tolerance
            ),
        );
    }
}

fn show_extension(
    ui: &mut egui::Ui,
    state: &mut State,
    activity: &mut UniformFluxState,
    extension: &UniformFluxExtension,
) {
    ui.label(UNIFORM_FLUX_TWO_COMPONENT_NOTE);
    ui.label(format!(
        "With this formula the extension is F=(Fx,Fy,0) = {}.",
        extension.extension_formula
    ));
    if activity.is_previewing() {
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.strong("Preview of the 3D extension");
            ui.monospace(&extension.extension_formula);
            ui.label("Switching loads this formula and the 3D view; the current formula stays stored for restore.");
            ui.horizontal_wrapped(|ui| {
                if ui.button("Switch to 3D extension").clicked() {
                    activity.apply_extension(state, &extension.extension_formula);
                }
                if ui.button("Cancel").clicked() {
                    activity.cancel_preview();
                }
            });
        });
    } else if ui
        .button("Preview 3D extension")
        .on_hover_text("Show the extension formula before switching")
        .clicked()
    {
        activity.preview_extension();
    }
}
