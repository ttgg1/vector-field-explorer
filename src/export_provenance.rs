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
use crate::maxwell::{
    MAXWELL_BEM_NOTE, MaxwellCase, MaxwellFamily, MaxwellSampling, MaxwellScene, MaxwellStop,
    maxwell_assumptions, maxwell_family, validate_maxwell,
};
use crate::maxwell_wave::WaveSpec;
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

/// The always-present Maxwell teaching-model line. The catalogue is bounded and
/// ideal; this line is never suppressible for a Maxwell export.
pub const MAXWELL_MODEL_LINE: &str = "Maxwell - ideal canonical teaching cases - no general solver";
/// The display-amplitude contract. Maxwell fields are normalized display
/// values, not SI quantities and not a constant engine.
pub const MAXWELL_DISPLAY_UNITS_LINE: &str =
    "Normalized display amplitudes; no SI constant engine.";
/// Warning when an export starts before the first Maxwell sampling pass.
pub const MAXWELL_NO_SAMPLING_LINE: &str = "Maxwell sampling unavailable";
/// Validation-failure line for the durable Maxwell scene.
pub const MAXWELL_INVALID_LINE: &str = "Maxwell scene invalid: {error}";

/// The always-present plane-wave model line. A wave replaces the case field, so
/// a wave export names the wave rather than the hidden case model.
pub const MAXWELL_WAVE_MODEL_LINE: &str =
    "Plane wave - analytic source-free full-wave illustration - no general solver";
/// Warning when a wave export starts before the first wave sampling pass.
pub const MAXWELL_WAVE_NO_SAMPLING_LINE: &str = "Plane-wave sampling unavailable";
/// Validation-failure line for a stored plane-wave spec.
pub const MAXWELL_WAVE_INVALID_LINE: &str = "Plane wave invalid: {error}";
/// The wave's normalized-unit contract; not SI and not a boundary-value result.
pub const MAXWELL_WAVE_UNITS_LINE: &str =
    "Normalized teaching units: c = 1, so |E| = |B|; these are not SI measurements.";

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
    /// True when this provenance came from Maxwell mode.
    pub maxwell_mode: bool,
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
        maxwell_mode: false,
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
        maxwell_mode: false,
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
        maxwell_mode: false,
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
        maxwell_mode: false,
    }
}

/// Derive export provenance for the bounded Maxwell teaching catalogue.
///
/// Pure and request-time: it reads only the durable scene and the already
/// cached sampling pass. It never samples the grid and never runs the 2D BEM
/// solve; a missing pass is reported instead. Non-Maxwell modes return `None`,
/// so another adapter cannot gain Maxwell metadata. The model line, the
/// display-unit contract and — for `Conductor2D` — the infinite-cylinder BEM
/// line are always present. Validation and unavailable/sampling warnings are
/// appended only when present.
pub fn maxwell_export_provenance(
    mode: FieldMode,
    scene: &MaxwellScene,
    sampling: Option<&MaxwellSampling>,
) -> Option<ExportProvenance> {
    if mode != FieldMode::Maxwell {
        return None;
    }

    let family = maxwell_family(scene.case);
    let mut warning_lines = vec![MAXWELL_MODEL_LINE.to_string()];
    if scene.case == MaxwellCase::Conductor2D {
        warning_lines.push(MAXWELL_BEM_NOTE.to_string());
    }
    let mut metadata = vec![
        (
            "vfe.model".to_string(),
            "maxwell-bounded-teaching".to_string(),
        ),
        (
            "vfe.maxwell_case".to_string(),
            maxwell_case_label(scene.case).to_string(),
        ),
        (
            "vfe.maxwell_family".to_string(),
            maxwell_family_label(family).to_string(),
        ),
        ("vfe.maxwell_units".to_string(), "display-only".to_string()),
        (
            "vfe.maxwell_assumptions".to_string(),
            maxwell_assumptions(scene).to_string(),
        ),
    ];

    let validation_error = validate_maxwell(scene)
        .err()
        .map(|error| maxwell_error_text(&error));
    if let Some(error) = &validation_error {
        warning_lines.push(MAXWELL_INVALID_LINE.replace("{error}", error));
        metadata.push(("vfe.validation_error".to_string(), error.clone()));
    }

    match sampling {
        None => warning_lines.push(MAXWELL_NO_SAMPLING_LINE.to_string()),
        Some(sampling) => {
            metadata.push((
                "vfe.maxwell_accuracy".to_string(),
                maxwell_stop_label(sampling.accuracy).to_string(),
            ));
            metadata.push((
                "vfe.maxwell_omitted".to_string(),
                sampling.omitted.to_string(),
            ));
            metadata.push((
                "vfe.maxwell_omitted_large".to_string(),
                sampling.omitted_large.to_string(),
            ));
            if sampling.accuracy == MaxwellStop::AccuracyNotMet {
                warning_lines.push(crate::maxwell::MAXWELL_ACCURACY_NOT_MET_WARNING.to_string());
            }
            if let Some(convergence) = sampling.convergence {
                metadata.push((
                    "vfe.maxwell_bem_coarse".to_string(),
                    convergence.coarse_elements.to_string(),
                ));
                metadata.push((
                    "vfe.maxwell_bem_fine".to_string(),
                    convergence.fine_elements.to_string(),
                ));
                metadata.push((
                    "vfe.maxwell_bem_relative_difference".to_string(),
                    convergence.relative_difference.to_string(),
                ));
            }
            if validation_error.is_none()
                && let Some(warning) = &sampling.warning
            {
                // A sampling pass joins independent warnings with `; `, but a
                // single warning can itself contain a semicolon. Strip the two
                // lines this function already emits instead of splitting the
                // text, so the exact unavailable message survives.
                let mut warning = warning.as_str();
                if let Some(rest) = warning.strip_prefix(MAXWELL_BEM_NOTE) {
                    warning = rest.strip_prefix("; ").unwrap_or(rest);
                }
                if let Some(rest) =
                    warning.strip_prefix(crate::maxwell::MAXWELL_ACCURACY_NOT_MET_WARNING)
                {
                    warning = rest.strip_prefix("; ").unwrap_or(rest);
                }
                let warning = warning.trim();
                if !warning.is_empty() && !warning_lines.iter().any(|line| line == warning) {
                    warning_lines.push(warning.to_string());
                }
            }
        }
    }
    warning_lines.push(MAXWELL_DISPLAY_UNITS_LINE.to_string());

    Some(ExportProvenance {
        warning_lines,
        metadata,
        source_mode: false,
        induction_mode: false,
        ode_mode: false,
        material_mode: false,
        maxwell_mode: true,
    })
}

/// Derive export provenance for the displayed analytic plane wave.
///
/// Pure and request-time: it reads the durable [`WaveSpec`] and whether the
/// wave arrow pass has already been sampled. It never evaluates the field and
/// never samples the hidden Maxwell case. Non-Maxwell modes return `None`, so
/// another adapter cannot gain wave metadata. The wave display supplies both
/// electric and magnetic families, so the case's family is never inherited.
pub fn maxwell_wave_export_provenance(
    mode: FieldMode,
    spec: &WaveSpec,
    sampled: bool,
) -> Option<ExportProvenance> {
    if mode != FieldMode::Maxwell {
        return None;
    }

    let mut checked = *spec;
    let validation_error = checked.validate().err();
    let mut warning_lines = vec![MAXWELL_WAVE_MODEL_LINE.to_string()];
    let mut metadata = vec![
        (
            "vfe.model".to_string(),
            "maxwell-plane-wave-teaching".to_string(),
        ),
        ("vfe.maxwell_display".to_string(), "plane_wave".to_string()),
        (
            "vfe.maxwell_family".to_string(),
            "electric+magnetic".to_string(),
        ),
        (
            "vfe.maxwell_units".to_string(),
            "normalized-teaching".to_string(),
        ),
        (
            "vfe.maxwell_assumptions".to_string(),
            maxwell_wave_assumptions(&checked),
        ),
        (
            "vfe.maxwell_sampling".to_string(),
            if sampled { "sampled" } else { "unavailable" }.to_string(),
        ),
    ];

    if let Some(error) = validation_error {
        warning_lines.push(MAXWELL_WAVE_INVALID_LINE.replace("{error}", &error));
        metadata.push(("vfe.validation_error".to_string(), error));
    }
    if !sampled {
        warning_lines.push(MAXWELL_WAVE_NO_SAMPLING_LINE.to_string());
    }
    warning_lines.push(MAXWELL_WAVE_UNITS_LINE.to_string());

    Some(ExportProvenance {
        warning_lines,
        metadata,
        source_mode: false,
        induction_mode: false,
        ode_mode: false,
        material_mode: false,
        maxwell_mode: true,
    })
}

/// The wave's model assumptions and exact normalized parameters.
fn maxwell_wave_assumptions(spec: &WaveSpec) -> String {
    format!(
        "Analytic source-free plane wave in normalized teaching units (c = 1, |E| = |B|); amplitude = {}, wavelength = {}, direction = {}, polarization = {}, phase = {}. Not SI and not a boundary-value solution.",
        spec.amplitude,
        spec.wavelength,
        wave_axis(spec.direction),
        wave_axis(spec.polarization),
        spec.phase,
    )
}

fn wave_axis(vector: glam::Vec3) -> String {
    format!("({:.4}, {:.4}, {:.4})", vector.x, vector.y, vector.z)
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

fn maxwell_case_label(case: MaxwellCase) -> &'static str {
    match case {
        MaxwellCase::ParallelPlates => "parallel_plates",
        MaxwellCase::CoaxialElectric => "coaxial_electric",
        MaxwellCase::CoaxialMagnetic => "coaxial_magnetic",
        MaxwellCase::HollowSphere => "hollow_sphere",
        MaxwellCase::InfiniteWire => "infinite_wire",
        MaxwellCase::Solenoid => "solenoid",
        MaxwellCase::ImageSphere => "image_sphere",
        MaxwellCase::ImagePlane => "image_plane",
        MaxwellCase::Conductor2D => "conductor_2d",
    }
}

fn maxwell_family_label(family: MaxwellFamily) -> &'static str {
    match family {
        MaxwellFamily::Electric => "electric",
        MaxwellFamily::Magnetic => "magnetic",
    }
}

fn maxwell_stop_label(stop: MaxwellStop) -> &'static str {
    match stop {
        MaxwellStop::TargetMet => "target_met",
        MaxwellStop::AccuracyNotMet => "accuracy_not_met",
        MaxwellStop::Unavailable => "unavailable",
    }
}

fn maxwell_error_text(error: &crate::maxwell::MaxwellError) -> String {
    match error {
        crate::maxwell::MaxwellError::InvalidScene(message)
        | crate::maxwell::MaxwellError::Unsupported(message)
        | crate::maxwell::MaxwellError::Unavailable(message) => message.clone(),
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
