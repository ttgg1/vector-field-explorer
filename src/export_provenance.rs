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
    })
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
