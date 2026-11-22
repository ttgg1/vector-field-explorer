//! Candidate field beside the active model (issue #215).
//!
//! The comparison is deliberately sampled at explicit points. It never claims
//! that two expressions are symbolically identical, and it never invents a
//! zero for a side that cannot be evaluated. Every value is a normalized
//! teaching quantity; neither side is an SI measurement.
//!
//! The difference is defined as **candidate minus model**, so a sign reversal
//! shows up as a difference of twice the model magnitude in the opposite
//! direction.

use glam::Vec3;

use crate::domain::Domain;
use crate::field::FieldExpr;

/// The identity disclaimer every sampled reading carries.
pub const NOT_IDENTITY_NOTE: &str = "Agreement is sampled at the shown points, not a symbolic identity; no symbolic check is performed.";

/// Both sides are shown in the same normalized teaching units and arrow scale.
pub const TEACHING_UNITS_NOTE: &str = "Both sides use the same normalized teaching units and arrow/scale convention; values are not SI.";

/// Relative differences below this count as numerical agreement.
const AGREEMENT_TOLERANCE: f64 = 1.0e-4;

/// A change larger than this between the two sampled relative differences is
/// read as a magnitude law that changes with position, not a uniform scale.
const RELATIVE_CHANGE_TOLERANCE: f64 = 1.0e-3;

/// How the candidate's direction relates to the model's at one point.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DirectionRelation {
    /// Both non-zero vectors point the same way.
    With,
    /// Both non-zero vectors point opposite ways.
    Against,
    /// Both non-zero vectors, but not parallel (nor anti-parallel).
    Different,
    /// At least one side is unavailable or zero, so direction is undefined.
    Undefined,
}

impl DirectionRelation {
    /// Short label for the reading.
    pub fn label(self) -> &'static str {
        match self {
            DirectionRelation::With => "with the model",
            DirectionRelation::Against => "against the model",
            DirectionRelation::Different => "differently from the model",
            DirectionRelation::Undefined => "undefined",
        }
    }
}

/// One side's sample at one point.
#[derive(Debug, Clone, PartialEq)]
pub enum SideSample {
    /// A finite vector in normalized teaching units.
    Available(Vec3),
    /// The side cannot be sampled here; the reason is shown verbatim.
    Unavailable(String),
}

impl SideSample {
    /// The sampled vector, when available.
    pub fn value(&self) -> Option<Vec3> {
        match self {
            SideSample::Available(vector) => Some(*vector),
            SideSample::Unavailable(_) => None,
        }
    }

    /// Why the side is unavailable, when it is.
    pub fn unavailable_reason(&self) -> Option<&str> {
        match self {
            SideSample::Available(_) => None,
            SideSample::Unavailable(reason) => Some(reason.as_str()),
        }
    }
}

/// Both sides at one shared point.
#[derive(Debug, Clone, PartialEq)]
pub struct PointComparison {
    /// The shared world point.
    pub point: Vec3,
    /// The active model field's sample.
    pub model: SideSample,
    /// The candidate expression's sample.
    pub candidate: SideSample,
    /// `candidate - model`; `None` when either side is unavailable.
    pub difference: Option<Vec3>,
    /// `|model|`, when available.
    pub model_magnitude: Option<f64>,
    /// `|candidate|`, when available.
    pub candidate_magnitude: Option<f64>,
    /// `|candidate - model|`, when both sides are available.
    pub difference_magnitude: Option<f64>,
    /// `|candidate - model| / |model|`; undefined at zero model magnitude.
    pub relative_difference: Option<f64>,
    /// Explains an undefined relative difference where the sides themselves
    /// are available (the zero-magnitude case).
    pub relative_difference_note: Option<String>,
    /// How the candidate direction relates to the model direction.
    pub relation: DirectionRelation,
}

/// Two labeled samples plus the sampled-agreement reading.
#[derive(Debug, Clone, PartialEq)]
pub struct ComparisonReport {
    /// First shared point.
    pub first: PointComparison,
    /// Second shared point (the radial r/2r partner by default).
    pub second: PointComparison,
    /// Honest prose reading; always states that agreement is sampled.
    pub note: String,
}

/// Sample both fields at `point` and compare them.
pub fn compare_point(
    model: &FieldExpr,
    candidate: &FieldExpr,
    point: Vec3,
    time: f64,
    domain: &Domain,
) -> PointComparison {
    let model_sample = sample_side(model, point, time, domain);
    let candidate_sample = sample_side(candidate, point, time, domain);

    let model_value = model_sample.value();
    let candidate_value = candidate_sample.value();
    let model_magnitude = model_value.map(|value| value.as_dvec3().length());
    let candidate_magnitude = candidate_value.map(|value| value.as_dvec3().length());
    let difference = match (model_value, candidate_value) {
        (Some(model), Some(candidate)) => Some(candidate - model),
        _ => None,
    };
    let difference_magnitude = difference.map(|value| value.as_dvec3().length());
    let relative_difference = match (model_magnitude, difference_magnitude) {
        (Some(model_length), Some(difference_length)) if model_length > 0.0 => {
            Some(difference_length / model_length)
        }
        _ => None,
    };
    let relative_difference_note = match (model_magnitude, difference_magnitude) {
        (Some(0.0), Some(_)) => {
            Some("Relative difference is undefined because the model magnitude is zero.".to_owned())
        }
        _ => None,
    };
    let relation = match (model_value, candidate_value) {
        (Some(model), Some(candidate)) => direction_relation(model, candidate),
        _ => DirectionRelation::Undefined,
    };

    PointComparison {
        point,
        model: model_sample,
        candidate: candidate_sample,
        difference,
        model_magnitude,
        candidate_magnitude,
        difference_magnitude,
        relative_difference,
        relative_difference_note,
        relation,
    }
}

/// Compare the two fields at a labeled pair of points.
pub fn compare_samples(
    model: &FieldExpr,
    candidate: &FieldExpr,
    first: Vec3,
    second: Vec3,
    time: f64,
    domain: &Domain,
) -> ComparisonReport {
    let first = compare_point(model, candidate, first, time, domain);
    let second = compare_point(model, candidate, second, time, domain);
    let note = sampled_agreement_note(&first, &second);
    ComparisonReport {
        first,
        second,
        note,
    }
}

fn sample_side(field: &FieldExpr, point: Vec3, time: f64, domain: &Domain) -> SideSample {
    if !point.is_finite() {
        return SideSample::Unavailable("the shared point is not finite".to_owned());
    }
    let inside = point.cmpge(domain.min).all() && point.cmple(domain.max).all();
    if !inside {
        return SideSample::Unavailable(
            "outside the visible sampled domain at this point".to_owned(),
        );
    }
    match field.eval3_at_f64(point.as_dvec3(), time) {
        Some(vector) if vector.is_finite() => SideSample::Available(vector),
        Some(_) => SideSample::Unavailable("non-finite field value at this point".to_owned()),
        None => SideSample::Unavailable(
            "the field could not be evaluated at this point (unsupported, singular or missing)"
                .to_owned(),
        ),
    }
}

fn direction_relation(model: Vec3, candidate: Vec3) -> DirectionRelation {
    let model = model.as_dvec3();
    let candidate = candidate.as_dvec3();
    let model_length = model.length();
    let candidate_length = candidate.length();
    if model_length == 0.0 || candidate_length == 0.0 {
        return DirectionRelation::Undefined;
    }
    let cosine = model.dot(candidate) / (model_length * candidate_length);
    if cosine >= 1.0 - 1.0e-6 {
        DirectionRelation::With
    } else if cosine <= -1.0 + 1.0e-6 {
        DirectionRelation::Against
    } else {
        DirectionRelation::Different
    }
}

fn sampled_agreement_note(first: &PointComparison, second: &PointComparison) -> String {
    let mut unavailable = Vec::new();
    for (label, reading) in [("first point", first), ("second point", second)] {
        if let Some(reason) = reading.model.unavailable_reason() {
            unavailable.push(format!("{label} model unavailable ({reason})"));
        }
        if let Some(reason) = reading.candidate.unavailable_reason() {
            unavailable.push(format!("{label} candidate unavailable ({reason})"));
        }
    }
    if !unavailable.is_empty() {
        return format!(
            "Comparison unavailable: {}. {NOT_IDENTITY_NOTE}",
            unavailable.join("; ")
        );
    }

    let body = if first.relation == DirectionRelation::Against
        && second.relation == DirectionRelation::Against
    {
        "The candidate points against the model at both sampled points, which is consistent with a sign error."
            .to_owned()
    } else if first.relation == DirectionRelation::With
        && second.relation == DirectionRelation::With
    {
        matching_note(first, second)
    } else if first.relation == second.relation {
        let phrase = match first.relation {
            DirectionRelation::With => "points with the model",
            DirectionRelation::Against => "points against the model",
            DirectionRelation::Different => "points differently from the model",
            DirectionRelation::Undefined => "has an undefined direction",
        };
        format!("The candidate {phrase} at both sampled points.")
    } else {
        "The two points disagree in direction, so no single sign or scale reading applies below."
            .to_owned()
    };
    format!("{body} {NOT_IDENTITY_NOTE}")
}

fn matching_note(first: &PointComparison, second: &PointComparison) -> String {
    // Both directions agree (`With`), so both model magnitudes are non-zero and
    // both relative differences are defined by `compare_point`.
    let first_relative = first
        .relative_difference
        .expect("directions agree, so the first model magnitude is non-zero");
    let second_relative = second
        .relative_difference
        .expect("directions agree, so the second model magnitude is non-zero");

    if first_relative <= AGREEMENT_TOLERANCE && second_relative <= AGREEMENT_TOLERANCE {
        "The candidate matches the model at both sampled points to within numerical agreement."
            .to_owned()
    } else if (first_relative - second_relative).abs()
        <= RELATIVE_CHANGE_TOLERANCE * first_relative.max(second_relative).max(1.0)
    {
        "The relative difference is unchanged between the two sampled points, which is consistent with a uniform scale error rather than a sign or power-law error."
            .to_owned()
    } else {
        "The relative difference changes between the two sampled points, which is consistent with a power-law magnitude error rather than a sign error."
            .to_owned()
    }
}
