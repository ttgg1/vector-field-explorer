//! Shared export provenance for source- and induction-mode PNG and SVG output.
//!
//! One text contract feeds the baked PNG banner, PNG `tEXt` metadata, the SVG
//! visible warning and the SVG `<metadata>` block. The object is derived at
//! export request time from the validated [`SourceScene`] plus cached
//! [`SourceSampling`], or from the durable [`InductionScene`] plus the shared
//! softening and cached [`InductionSampling`]; it never samples the field
//! itself and never creates vector data.
//!
//! Warning-presence rule: Sources and Induction always carry their
//! softened-model line. A missing sampling pass adds the matching
//! no-sampling line, a validation failure adds the matching invalid line, and
//! accuracy/unavailable counts add their lines only when nonzero.
//! `allow_inaccurate` changes the drawn count, never the warning.

use crate::induction::{InductionCase, InductionFamilyStatus, InductionSampling, InductionScene};
use crate::material::{
    MaterialCase, MaterialFamily, MaterialSampling, MaterialScene, MaterialView, material_family,
    validate_material,
};
use crate::ode::{ODE_STEP, OdeScene, OdeSolution, OdeSource};
use crate::sim_clock::SimClock;
use crate::source_scene::{
    FieldMode, SourceDisplay, SourceFamilyStatus, SourceSampling, SourceScene,
};

/// The always-present Sources model line. ASCII only: it is painted by the
/// built-in bitmap font and written through a Latin-1 PNG text chunk.
pub const MODEL_LINE: &str = "Sources - softened teaching model - a = {softening}";
/// Accuracy diagnosis line; `hidden` is total minus drawn.
pub const ACCURACY_LINE: &str = "{display} - accuracy not met: {hidden} hidden, {drawn} drawn";
/// One line per export when any result was unavailable.
pub const UNAVAILABLE_LINE: &str = "Unavailable: {count}";
/// Validation-failure line. The error text is escaped later by each backend.
pub const INVALID_LINE: &str = "Source scene invalid: {error}";
/// Warning when export starts before a sampling rebuild has completed.
pub const NO_SAMPLING_LINE: &str = "Source sampling unavailable";

/// The always-present Induction model line (design §9).
pub const INDUCTION_MODEL_LINE: &str =
    "Induction - quasistatic axisymmetric softened model - a = {softening}";
/// Warning when export starts before induction sampling has completed.
pub const INDUCTION_NO_SAMPLING_LINE: &str = "Induction sampling unavailable";
/// Validation-failure line for the induction scene or sampling pass.
pub const INDUCTION_INVALID_LINE: &str = "Induction scene invalid: {error}";

/// The always-present ODE model line. `{step}` is the production [`ODE_STEP`].
pub const ODE_MODEL_LINE: &str = "ODE - fixed-step RK4 teaching model - h = {step}";
/// ODE trajectory contract; the workbench is bounded and CPU-world-coordinate.
pub const ODE_BOUNDED_LINE: &str = "ODE - bounded world-coordinate trajectories";
/// Warning when an Ode export has no solved scene to describe.
pub const ODE_UNAVAILABLE_LINE: &str = "ODE solution unavailable";
/// Validation-failure line for the durable ODE scene.
pub const ODE_INVALID_LINE: &str = "ODE scene invalid: {error}";
/// Playback/timeline context for an available ODE solution.
pub const ODE_TIMELINE_LINE: &str = "timeline: duration = {duration}, step = {step}, t = {time}";

/// The always-present Materials model line. The bounded catalogue is exact;
/// this is the only Material model statement, and it is never suppressible.
pub const MATERIAL_MODEL_LINE: &str = "Materials - bounded analytic cases - no general solver";
/// Warning when no material sampling pass is available at export time.
pub const MATERIAL_NO_SAMPLING_LINE: &str = "Materials sampling unavailable";
/// Validation-failure line for the durable material scene.
pub const MATERIAL_INVALID_LINE: &str = "Materials scene invalid: {error}";

/// Capture-time export context shared by the PNG and SVG paths.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ExportProvenance {
    /// Warning/context lines in display order. Never empty for Sources or
    /// Induction.
    pub warning_lines: Vec<String>,
    /// `vfe.*` key/value pairs for PNG text chunks and SVG metadata.
    pub metadata: Vec<(String, String)>,
    /// True when this provenance came from Sources mode.
    pub source_mode: bool,
    /// True when this provenance came from Induction mode.
    pub induction_mode: bool,
    /// True when this provenance came from Ode mode.
    pub ode_mode: bool,
    /// True when this provenance came from Materials mode.
    pub material_mode: bool,
}

/// Derive export provenance for the current field mode.
///
/// Formula mode returns `None`: ordinary formula exports keep their existing
/// bytes and semantics. Sources mode always returns `Some`, even when sampling
/// is missing or invalid, so an export can never silently drop the warning.
pub fn source_export_provenance(
    mode: FieldMode,
    scene: &SourceScene,
    sampling: Option<&SourceSampling>,
) -> Option<ExportProvenance> {
    if mode != FieldMode::Sources {
        return None;
    }

    let softening = scene.softening;
    let display = display_label(scene.display);
    let mut warning_lines = vec![MODEL_LINE.replace("{softening}", &softening.to_string())];
    let mut metadata = vec![
        (
            "vfe.source_model".to_string(),
            "softened teaching model".to_string(),
        ),
        ("vfe.softening".to_string(), softening.to_string()),
        ("vfe.display".to_string(), display.to_string()),
    ];

    match sampling {
        None => {
            warning_lines.push(NO_SAMPLING_LINE.to_string());
            metadata.push(("vfe.accuracy_not_met_total".to_string(), "0".to_string()));
            metadata.push(("vfe.accuracy_not_met_drawn".to_string(), "0".to_string()));
            metadata.push(("vfe.unavailable".to_string(), "0".to_string()));
        }
        Some(sampling) => {
            let (accuracy_not_met_total, accuracy_not_met_drawn, unavailable) =
                counts(scene.display, sampling);

            metadata.push((
                "vfe.accuracy_not_met_total".to_string(),
                accuracy_not_met_total.to_string(),
            ));
            metadata.push((
                "vfe.accuracy_not_met_drawn".to_string(),
                accuracy_not_met_drawn.to_string(),
            ));
            metadata.push(("vfe.unavailable".to_string(), unavailable.to_string()));

            if accuracy_not_met_total > 0 {
                let hidden = accuracy_not_met_total.saturating_sub(accuracy_not_met_drawn);
                warning_lines.push(
                    ACCURACY_LINE
                        .replace("{display}", display)
                        .replace("{hidden}", &hidden.to_string())
                        .replace("{drawn}", &accuracy_not_met_drawn.to_string()),
                );
            }
            if unavailable > 0 {
                warning_lines.push(UNAVAILABLE_LINE.replace("{count}", &unavailable.to_string()));
            }
            if let Some(error) = &sampling.validation_error {
                warning_lines.push(INVALID_LINE.replace("{error}", error));
                metadata.push(("vfe.validation_error".to_string(), error.clone()));
            }
        }
    }

    Some(ExportProvenance {
        warning_lines,
        metadata,
        source_mode: true,
        induction_mode: false,
        ode_mode: false,
        material_mode: false,
    })
}

/// Derive export provenance for the induction teaching demo.
///
/// Formula and Sources return `None`; Induction always returns `Some`, even
/// when sampling is missing or the scene fails validation, so an induction
/// export can never silently drop its model/accuracy warning. `softening` is
/// the one shared scene softening (`State::sources.softening`); a cached
/// sampling pass must have been built with the same value.
pub fn induction_export_provenance(
    mode: FieldMode,
    scene: &InductionScene,
    softening: f64,
    sampling: Option<&InductionSampling>,
) -> Option<ExportProvenance> {
    if mode != FieldMode::Induction {
        return None;
    }
    if let Some(sampling) = sampling {
        debug_assert_eq!(
            sampling.softening, softening,
            "export softening must match the cached induction sampling",
        );
    }

    let display = display_label(scene.display);
    let mut warning_lines =
        vec![INDUCTION_MODEL_LINE.replace("{softening}", &softening.to_string())];
    let mut metadata = vec![
        (
            "vfe.model".to_string(),
            "quasistatic-axisymmetric-faraday".to_string(),
        ),
        (
            "vfe.induction_case".to_string(),
            induction_case_label(scene.case).to_string(),
        ),
        (
            "vfe.waveform".to_string(),
            "amplitude*cos(2*pi*frequency*t)".to_string(),
        ),
        (
            "vfe.waveform_derivative".to_string(),
            "-amplitude*2*pi*frequency*sin(2*pi*frequency*t)".to_string(),
        ),
        ("vfe.amplitude".to_string(), scene.amplitude.to_string()),
        ("vfe.frequency".to_string(), scene.frequency.to_string()),
        ("vfe.softening".to_string(), softening.to_string()),
        ("vfe.display".to_string(), display.to_string()),
    ];

    let mut scene_error = None;
    if let Err(error) = scene.validate() {
        warning_lines.push(INDUCTION_INVALID_LINE.replace("{error}", &error));
        metadata.push(("vfe.validation_error".to_string(), error.clone()));
        scene_error = Some(error);
    }

    match sampling {
        None => {
            warning_lines.push(INDUCTION_NO_SAMPLING_LINE.to_string());
            metadata.push(("vfe.accuracy_not_met_total".to_string(), "0".to_string()));
            metadata.push(("vfe.accuracy_not_met_drawn".to_string(), "0".to_string()));
            metadata.push(("vfe.unavailable".to_string(), "0".to_string()));
        }
        Some(sampling) => {
            let (accuracy_not_met_total, accuracy_not_met_drawn, unavailable) =
                induction_counts(scene.display, sampling);

            metadata.push((
                "vfe.accuracy_not_met_total".to_string(),
                accuracy_not_met_total.to_string(),
            ));
            metadata.push((
                "vfe.accuracy_not_met_drawn".to_string(),
                accuracy_not_met_drawn.to_string(),
            ));
            metadata.push(("vfe.unavailable".to_string(), unavailable.to_string()));

            if accuracy_not_met_total > 0 {
                let hidden = accuracy_not_met_total.saturating_sub(accuracy_not_met_drawn);
                warning_lines.push(
                    ACCURACY_LINE
                        .replace("{display}", display)
                        .replace("{hidden}", &hidden.to_string())
                        .replace("{drawn}", &accuracy_not_met_drawn.to_string()),
                );
            }
            if unavailable > 0 {
                warning_lines.push(UNAVAILABLE_LINE.replace("{count}", &unavailable.to_string()));
            }
            if scene_error.is_none()
                && let Some(error) = &sampling.validation_error
            {
                warning_lines.push(INDUCTION_INVALID_LINE.replace("{error}", error));
                metadata.push(("vfe.validation_error".to_string(), error.clone()));
            }
        }
    }

    Some(ExportProvenance {
        warning_lines,
        metadata,
        source_mode: false,
        induction_mode: true,
        ode_mode: false,
        material_mode: false,
    })
}

/// Derive export provenance for the ODE workbench.
///
/// Formula, Sources and Induction return `None`; Ode always returns `Some`, so
/// a pending or failed solve can never silently drop the teaching-model warning.
/// The function reads only the durable scene, the already-computed solution and
/// the shared clock: it never samples the field or re-integrates. `error` is the
/// retained scene/solve error, exported as a warning and metadata only when
/// present. `timeline` contributes the current playback time, not solver work.
pub fn ode_export_provenance(
    mode: FieldMode,
    scene: &OdeScene,
    solution: Option<&OdeSolution>,
    error: Option<&str>,
    timeline: &SimClock,
) -> Option<ExportProvenance> {
    if mode != FieldMode::Ode {
        return None;
    }

    let mut warning_lines = vec![
        ODE_MODEL_LINE.replace("{step}", &ODE_STEP.to_string()),
        ODE_BOUNDED_LINE.to_string(),
    ];
    let (dimension, step, duration) = match solution {
        Some(solution) => (solution.dimension, solution.step, solution.duration),
        None => (scene.dimension, ODE_STEP, scene.duration),
    };
    let mut metadata = vec![
        ("vfe.model".to_string(), "ode-fixed-step-rk4".to_string()),
        (
            "vfe.ode_source".to_string(),
            ode_source_label(scene.source).to_string(),
        ),
        ("vfe.ode_dimension".to_string(), dimension.to_string()),
        ("vfe.ode_step".to_string(), step.to_string()),
        ("vfe.ode_duration".to_string(), duration.to_string()),
    ];

    match solution {
        Some(solution) => warning_lines.push(
            ODE_TIMELINE_LINE
                .replace("{duration}", &solution.duration.to_string())
                .replace("{step}", &solution.step.to_string())
                .replace("{time}", &format!("{:.3}", timeline.time)),
        ),
        None => warning_lines.push(ODE_UNAVAILABLE_LINE.to_string()),
    }
    if let Some(error) = error {
        warning_lines.push(ODE_INVALID_LINE.replace("{error}", error));
        metadata.push(("vfe.validation_error".to_string(), error.to_string()));
    }

    Some(ExportProvenance {
        warning_lines,
        metadata,
        source_mode: false,
        induction_mode: false,
        ode_mode: true,
        material_mode: false,
    })
}

/// Derive export provenance for the bounded Materials catalogue.
///
/// Pure and request-time: it reads the durable scene and the already-cached
/// sampling only. It never evaluates the field, samples a grid or solves a
/// boundary-value problem. Non-Materials modes return an empty provenance with
/// `material_mode` false, so exporting another adapter cannot gain material
/// metadata. In Materials mode the result always carries the bounded-model,
/// background/material constants, view, applied field and assumptions lines;
/// a missing sampling pass or a scene/sampling warning is always appended.
pub fn material_export_provenance(
    mode: FieldMode,
    scene: &MaterialScene,
    sampling: Option<&MaterialSampling>,
) -> ExportProvenance {
    if mode != FieldMode::Materials {
        return ExportProvenance::default();
    }

    let family = material_family(scene.case);
    let (family_label, field, units) = match family {
        MaterialFamily::Electric => ("electric", "E", "V/m"),
        MaterialFamily::Magnetic => ("magnetic", "B", "T"),
    };
    let [x, y, z] = scene.applied;
    let mut warning_lines = vec![
        MATERIAL_MODEL_LINE.to_string(),
        format!(
            "Background eps_r = {}, mu_r = {}",
            scene.background_epsilon_r, scene.background_mu_r
        ),
        format!(
            "Material eps_r = {}, mu_r = {}",
            scene.material_epsilon_r, scene.material_mu_r
        ),
        format!("Applied/response: {}", material_view_label(scene.view)),
        format!("Applied field: {field} = ({x}, {y}, {z}) {units}"),
        crate::material::material_assumptions(scene.case).to_string(),
    ];
    let mut warning_metadata = Vec::new();
    if let Err(error) = validate_material(scene) {
        let line = MATERIAL_INVALID_LINE.replace("{error}", &material_error_text(&error));
        warning_lines.push(line.clone());
        warning_metadata.push(line);
    }
    match sampling {
        None => {
            warning_lines.push(MATERIAL_NO_SAMPLING_LINE.to_string());
            warning_metadata.push(MATERIAL_NO_SAMPLING_LINE.to_string());
        }
        Some(sampling) => {
            if let Some(warning) = &sampling.warning
                && !warning.is_empty()
            {
                warning_lines.push(warning.clone());
                warning_metadata.push(warning.clone());
            }
        }
    }

    let mut metadata = vec![
        (
            "vfe.model".to_string(),
            "materials-bounded-analytic".to_string(),
        ),
        (
            "vfe.material_case".to_string(),
            material_case_label(scene.case).to_string(),
        ),
        ("vfe.material_family".to_string(), family_label.to_string()),
        (
            "vfe.background_eps_r".to_string(),
            scene.background_epsilon_r.to_string(),
        ),
        (
            "vfe.background_mu_r".to_string(),
            scene.background_mu_r.to_string(),
        ),
        (
            "vfe.material_eps_r".to_string(),
            scene.material_epsilon_r.to_string(),
        ),
        (
            "vfe.material_mu_r".to_string(),
            scene.material_mu_r.to_string(),
        ),
        (
            "vfe.material_view".to_string(),
            material_view_label(scene.view).to_string(),
        ),
        ("vfe.material_units".to_string(), units.to_string()),
        (
            "vfe.material_interface_axis".to_string(),
            material_axis_label(scene.interface_axis).to_string(),
        ),
        (
            "vfe.material_interface_offset".to_string(),
            scene.interface_offset.to_string(),
        ),
        (
            "vfe.material_applied".to_string(),
            format!("({x}, {y}, {z})"),
        ),
    ];
    if !warning_metadata.is_empty() {
        metadata.push((
            "vfe.material_warning".to_string(),
            warning_metadata.join("; "),
        ));
    }

    ExportProvenance {
        warning_lines,
        metadata,
        source_mode: false,
        induction_mode: false,
        ode_mode: false,
        material_mode: true,
    }
}

/// The visible warning/context lines, in paint order.
pub fn provenance_lines(provenance: &ExportProvenance) -> &[String] {
    &provenance.warning_lines
}

/// The `vfe.*` metadata pairs, in write order.
pub fn provenance_metadata(provenance: &ExportProvenance) -> &[(String, String)] {
    &provenance.metadata
}

fn display_label(display: SourceDisplay) -> &'static str {
    match display {
        SourceDisplay::Electric => "Electric",
        SourceDisplay::Magnetic => "Magnetic",
        SourceDisplay::Both => "Both",
    }
}

fn induction_case_label(case: InductionCase) -> &'static str {
    match case {
        InductionCase::CurrentLoop => "current_loop",
        InductionCase::Solenoid => "solenoid",
    }
}

fn ode_source_label(source: OdeSource) -> &'static str {
    match source {
        OdeSource::Equations => "equations",
        OdeSource::CurrentField => "current_field",
    }
}

fn material_case_label(case: MaterialCase) -> &'static str {
    match case {
        MaterialCase::DielectricSphere => "dielectric_sphere",
        MaterialCase::DielectricBoundary => "dielectric_boundary",
        MaterialCase::PermeableSphere => "permeable_sphere",
        MaterialCase::PermeableBoundary => "permeable_boundary",
    }
}

fn material_view_label(view: MaterialView) -> &'static str {
    match view {
        MaterialView::Total => "Total",
        MaterialView::Applied => "Applied",
        MaterialView::Response => "Response",
    }
}

fn material_axis_label(axis: crate::plane::PlaneAxis) -> &'static str {
    axis.label()
}

fn material_error_text(error: &crate::material::MaterialError) -> String {
    match error {
        crate::material::MaterialError::InvalidScene(message)
        | crate::material::MaterialError::Unsupported(message) => message.clone(),
    }
}

/// Only families the active display actually shows contribute counts, so the
/// induction warning names the same families the viewport draws.
fn induction_counts(display: SourceDisplay, sampling: &InductionSampling) -> (usize, usize, usize) {
    let families: &[&InductionFamilyStatus] = match display {
        SourceDisplay::Electric => &[&sampling.electric],
        SourceDisplay::Magnetic => &[&sampling.magnetic],
        SourceDisplay::Both => &[&sampling.electric, &sampling.magnetic],
    };
    families
        .iter()
        .fold((0, 0, 0), |(total, drawn, unavailable), family| {
            (
                total + family.accuracy_not_met_total,
                drawn + family.accuracy_not_met_drawn,
                unavailable + family.unavailable,
            )
        })
}

/// Only families the active display actually shows contribute counts, so the
/// warning names the same families the viewport draws.
fn counts(display: SourceDisplay, sampling: &SourceSampling) -> (usize, usize, usize) {
    let families: &[&SourceFamilyStatus] = match display {
        SourceDisplay::Electric => &[&sampling.electric],
        SourceDisplay::Magnetic => &[&sampling.magnetic],
        SourceDisplay::Both => &[&sampling.electric, &sampling.magnetic],
    };
    families
        .iter()
        .fold((0, 0, 0), |(total, drawn, unavailable), family| {
            (
                total + family.accuracy_not_met_total,
                drawn + family.accuracy_not_met_drawn,
                unavailable + family.unavailable,
            )
        })
}
