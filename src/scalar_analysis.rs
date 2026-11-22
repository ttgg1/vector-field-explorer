//! Scalar comparison series and potential gradients (issue #93 slice A).
//!
//! This is a pure model: bounded display-only comparison series, a validated
//! finite-difference gradient request, honest central/one-sided samples with a
//! refinement-agreement error estimate, and a one-entry invalidating cache.
//! Sampling reuses the #92 seam ([`ScalarExpr::eval_at`], the surface parser's
//! unsupported reasons and the 10 Hz clock rule). Nothing here reaches arrows,
//! colour mapping, probes, rendering or persistence beyond the `ViewRecord`
//! fields this module defines.
//!
//! The contract is
//! `docs/superpowers/specs/2026-11-18-scalar-analysis-contract.md`.

use std::collections::HashSet;

use glam::Vec3;
use serde::{Deserialize, Serialize};

use crate::{
    color_ramp::Rgb,
    domain::Domain,
    parameters::ParameterSet,
    plane::{PlaneAxis, PlaneFrame},
    scalar::ScalarExpr,
    scalar_view::{MAX_GRID_SIDE, SCALAR_RESAMPLE_INTERVAL, scalar_view_expr},
};

/// Most comparison series one scene may hold.
pub const MAX_COMPARISON_SERIES: usize = 4;
/// Longest comparison-series label in UTF-8 bytes.
pub const MAX_SERIES_LABEL_BYTES: usize = 48;
/// Most gradient samples one rebuild may produce (the #92 160² grid bound).
pub const MAX_GRADIENT_SAMPLES: usize = 25_600;
/// Smallest accepted finite-difference step.
pub const MIN_STEP: f32 = 1e-6;
/// Largest accepted finite-difference step.
pub const MAX_STEP: f32 = 1e-1;

/// The square gradient grid side: `MAX_GRID_SIDE² == MAX_GRADIENT_SAMPLES`.
const GRADIENT_GRID_SIDE: u32 = MAX_GRID_SIDE;

const _: () =
    assert!((GRADIENT_GRID_SIDE as usize) * (GRADIENT_GRID_SIDE as usize) == MAX_GRADIENT_SAMPLES);

/// What a gradient's source represents. Only an electric potential may be
/// labelled `E = -∇V`; a generic scalar stays `∇f`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SeriesRole {
    /// A generic scalar `f`.
    #[default]
    Scalar,
    /// An electric potential `V`; the field overlay is `E = -∇V`.
    ElectricPotential,
}

impl SeriesRole {
    /// The honest display label for this role.
    pub fn label(self) -> &'static str {
        match self {
            Self::Scalar => "∇f",
            Self::ElectricPotential => "E = -∇V",
        }
    }
}

/// User-facing note that the analytic dipole potential is a platform-neutral
/// route: it is plain expression text and needs no symbolic backend.
pub const ANALYTIC_DIPOLE_ROUTE_NOTE: &str = "Analytic dipole route: the closed-form potential is built from plain expression text, \
     so it is available on web and native.";
/// User-facing note naming the capability boundary: symbolic CAS operations
/// rely on the native engine, while the finite-difference `E = -∇V` overlay is
/// a separate platform-neutral path.
pub const NATIVE_SYMBOLIC_NOTE: &str = "Symbolic CAS operations (differentiate, symbolic gradient, divergence, curl) are native-only; \
     the analytic dipole potential and the finite-difference E = -∇V overlay stay available on web.";
/// The default finite-difference step for the scene gradient overlay.
pub const DEFAULT_GRADIENT_STEP: f32 = 1e-3;
/// The default usable-sample budget for the scene gradient overlay.
pub const DEFAULT_GRADIENT_MAX_ERROR: f32 = 5e-2;

/// The analytic ideal dipole potential on one slice, in surface-context
/// expression text.
///
/// The two equal and opposite unit charges sit at `±(separation / 2)` along
/// `dipole_axis` around the frame origin's projection. `separation_name` is a
/// committed parameter name such as `d`, so editing that parameter re-evaluates
/// both the contour and gradient layers from one expression. The region note
/// names the slice and the exact pole locations.
#[derive(Debug, Clone, PartialEq)]
pub struct AnalyticDipole {
    /// Surface-context potential `V(x, y)` on the slice.
    pub expr: String,
    /// The honest valid region: the slice and the two poles.
    pub valid_region: String,
    /// The layer role; always [`SeriesRole::ElectricPotential`].
    pub role: SeriesRole,
}

/// Build the analytic ideal dipole potential on `frame`.
///
/// The result is the ideal `1/r` form with normalized unit charges. It uses
/// the existing surface parser's `x`, `y` and committed-parameter names; it
/// never adds a second grammar or solver. A dipole normal to the slice projects
/// both charges onto the same point; the expression is then identically zero
/// there and the region note says so instead of hiding the degeneracy.
pub fn analytic_dipole_potential(
    dipole_axis: PlaneAxis,
    separation_name: &str,
    frame: &PlaneFrame,
) -> Result<AnalyticDipole, String> {
    if separation_name.trim().is_empty() {
        return Err("the dipole separation needs a parameter name".to_owned());
    }
    validate_frame(frame)?;
    let axis = match dipole_axis {
        PlaneAxis::X => Vec3::X,
        PlaneAxis::Y => Vec3::Y,
        PlaneAxis::Z => Vec3::Z,
    };
    let origin_u = f64::from(frame.origin.dot(frame.right));
    let origin_v = f64::from(frame.origin.dot(frame.up));
    let tau_u = f64::from(axis.dot(frame.right)) * 0.5;
    let tau_v = f64::from(axis.dot(frame.up)) * 0.5;
    let u_positive = format!("({tau_u}*{separation_name} + ({origin_u}))");
    let v_positive = format!("({tau_v}*{separation_name} + ({origin_v}))");
    let u_negative = format!("(-{tau_u}*{separation_name} + ({origin_u}))");
    let v_negative = format!("(-{tau_v}*{separation_name} + ({origin_v}))");
    let expr = format!(
        "1/sqrt((x-{u_positive})^2+(y-{v_positive})^2) \
         - 1/sqrt((x-{u_negative})^2+(y-{v_negative})^2)"
    );
    let projected_apart = tau_u.abs() > f64::EPSILON || tau_v.abs() > f64::EPSILON;
    let poles = if projected_apart {
        format!(
            "the two charge positions project to ({u_positive}, {v_positive}) and \
             ({u_negative}, {v_negative}) on this slice; V is undefined exactly at those points"
        )
    } else {
        "this dipole is normal to the slice, so both charges project to the same point; \
         V is zero everywhere else on the slice and undefined at that point"
            .to_owned()
    };
    let valid_region = format!(
        "Analytic on the {} for the committed {separation_name}: {poles}.",
        crate::scalar_view::slice_label(frame)
    );
    Ok(AnalyticDipole {
        expr,
        valid_region,
        role: SeriesRole::ElectricPotential,
    })
}

/// A validated `E = -∇V` request for an analytic potential on `frame`.
pub fn potential_field_spec(expr: String, frame: PlaneFrame) -> Result<GradientSpec, String> {
    let spec = GradientSpec {
        role: SeriesRole::ElectricPotential,
        expr,
        frame,
        step: DEFAULT_GRADIENT_STEP,
        max_error: DEFAULT_GRADIENT_MAX_ERROR,
    };
    spec.validate()?;
    Ok(spec)
}

/// A stable per-record comparison-series identifier.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(transparent)]
pub struct SeriesId(pub u32);

/// Independent colour and dash for one comparison series. Colour mapping is
/// never consulted: a series keeps this plain sRGB colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SeriesStyle {
    /// The series line colour.
    #[serde(default = "default_series_color")]
    pub color: Rgb,
    /// Whether the curve is dashed.
    #[serde(default)]
    pub dashed: bool,
}

impl SeriesStyle {
    /// Distinct fallback colours for the bounded series list.
    pub const FALLBACK_COLORS: [Rgb; MAX_COMPARISON_SERIES] = [
        Rgb([31, 119, 180]),
        Rgb([255, 127, 14]),
        Rgb([44, 160, 44]),
        Rgb([214, 39, 40]),
    ];

    /// The palette style for `index`, cycling through [`Self::FALLBACK_COLORS`].
    pub fn fallback(index: usize) -> Self {
        Self {
            color: Self::FALLBACK_COLORS[index % Self::FALLBACK_COLORS.len()],
            dashed: index % 2 == 1,
        }
    }
}

impl Default for SeriesStyle {
    fn default() -> Self {
        Self::fallback(0)
    }
}

/// One display-only comparison curve. `expr` is parsed with the existing
/// `ScalarContext::Curve` by the plot consumer (slice B); it never feeds
/// gradients, arrows or colours.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ComparisonSeries {
    /// Stable, user-visible key.
    #[serde(default)]
    pub id: SeriesId,
    /// Label of at most [`MAX_SERIES_LABEL_BYTES`] bytes.
    #[serde(default)]
    pub label: String,
    /// Curve expression text.
    #[serde(default)]
    pub expr: String,
    /// Independent colour and dash.
    #[serde(default)]
    pub style: SeriesStyle,
}

impl ComparisonSeries {
    /// Build one series record without validating it.
    pub fn new(id: SeriesId, label: String, expr: String, style: SeriesStyle) -> Self {
        Self {
            id,
            label,
            expr,
            style,
        }
    }

    /// Reject an empty/over-long label or an empty expression. Duplicate ids
    /// are a set-level rule, not a series rule.
    pub fn validate(&self) -> Result<(), String> {
        if self.label.is_empty() {
            return Err("comparison series label must not be empty".to_owned());
        }
        if self.label.len() > MAX_SERIES_LABEL_BYTES {
            return Err(format!(
                "comparison series label is {} bytes; at most {MAX_SERIES_LABEL_BYTES} are allowed",
                self.label.len()
            ));
        }
        if self.expr.trim().is_empty() {
            return Err("comparison series expression must not be empty".to_owned());
        }
        Ok(())
    }
}

/// A bounded, uniquely-identified set of comparison series.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ComparisonSet {
    series: Vec<ComparisonSeries>,
}

impl ComparisonSet {
    /// An empty set.
    pub fn new() -> Self {
        Self::default()
    }

    /// Build a set without validating or repairing it. Startup restore uses
    /// this so corrupt stored data can be sanitized in one place.
    pub fn from_series(series: Vec<ComparisonSeries>) -> Self {
        Self { series }
    }

    /// Number of series.
    pub fn len(&self) -> usize {
        self.series.len()
    }

    /// Whether the set holds no series.
    pub fn is_empty(&self) -> bool {
        self.series.is_empty()
    }

    /// The series in display order.
    pub fn as_slice(&self) -> &[ComparisonSeries] {
        &self.series
    }

    /// The series in display order.
    pub fn iter(&self) -> impl Iterator<Item = &ComparisonSeries> {
        self.series.iter()
    }

    /// One series by id.
    pub fn get(&self, id: SeriesId) -> Option<&ComparisonSeries> {
        self.series.iter().find(|series| series.id == id)
    }

    /// Validate the cap, unique ids and every series.
    pub fn validate(&self) -> Result<(), String> {
        if self.series.len() > MAX_COMPARISON_SERIES {
            return Err(format!(
                "comparison series set has {} entries; at most {MAX_COMPARISON_SERIES} are allowed",
                self.series.len()
            ));
        }
        let mut seen = HashSet::with_capacity(self.series.len());
        for series in &self.series {
            series.validate()?;
            if !seen.insert(series.id) {
                return Err(format!("duplicate comparison series id {}", series.id.0));
            }
        }
        Ok(())
    }

    /// Append a validated, uniquely-identified series while the cap has room.
    /// A rejected push never changes the set.
    pub fn push(&mut self, series: ComparisonSeries) -> Result<(), String> {
        if self.series.len() >= MAX_COMPARISON_SERIES {
            return Err(format!(
                "at most {MAX_COMPARISON_SERIES} comparison series are allowed"
            ));
        }
        series.validate()?;
        if self.series.iter().any(|existing| existing.id == series.id) {
            return Err(format!("duplicate comparison series id {}", series.id.0));
        }
        self.series.push(series);
        Ok(())
    }

    /// Remove one series by id.
    pub fn remove(&mut self, id: SeriesId) -> Option<ComparisonSeries> {
        let index = self.series.iter().position(|series| series.id == id)?;
        Some(self.series.remove(index))
    }

    /// Total repair for stored data: drop empty expressions and duplicate ids,
    /// truncate over-long labels on a character boundary, give empty labels a
    /// stable fallback, and enforce the cap while preserving order. Valid
    /// colours are never overridden; a missing style decoded through the serde
    /// default uses the palette fallback.
    pub fn sanitized(mut self) -> Self {
        let mut seen = HashSet::with_capacity(self.series.len());
        let mut repaired = Vec::with_capacity(self.series.len().min(MAX_COMPARISON_SERIES));
        for mut series in self.series.drain(..) {
            if series.expr.trim().is_empty() || !seen.insert(series.id) {
                continue;
            }
            if series.label.is_empty() {
                series.label = format!("series {}", repaired.len() + 1);
            }
            if series.label.len() > MAX_SERIES_LABEL_BYTES {
                series.label = truncate_label(&series.label);
            }
            repaired.push(series);
            if repaired.len() >= MAX_COMPARISON_SERIES {
                break;
            }
        }
        Self { series: repaired }
    }
}

/// A validated finite-difference request for one surface expression.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GradientSpec {
    /// What the differentiated source represents and how it is labelled.
    #[serde(default)]
    pub role: SeriesRole,
    /// The differentiated surface expression text.
    #[serde(default)]
    pub expr: String,
    /// The plane frame the samples cover.
    #[serde(default = "default_gradient_frame", with = "frame_serde")]
    pub frame: PlaneFrame,
    /// The finite-difference step, validated and never clamped.
    #[serde(default)]
    pub step: f32,
    /// Budget for a usable sample.
    #[serde(default)]
    pub max_error: f32,
}

impl GradientSpec {
    /// The honest display label for this request's role.
    pub fn label(&self) -> &'static str {
        self.role.label()
    }

    /// Reject an empty source, an invalid frame, a step outside
    /// `[MIN_STEP, MAX_STEP]` (never clamped) and a negative/non-finite budget.
    pub fn validate(&self) -> Result<(), String> {
        if self.expr.trim().is_empty() {
            return Err("gradient expression must not be empty".to_owned());
        }
        validate_frame(&self.frame)?;
        if !self.step.is_finite() || !(MIN_STEP..=MAX_STEP).contains(&self.step) {
            return Err(format!(
                "gradient step {} is outside {MIN_STEP}..={MAX_STEP}",
                self.step
            ));
        }
        if !self.max_error.is_finite() || self.max_error < 0.0 {
            return Err("gradient max_error must be finite and at least zero".to_owned());
        }
        Ok(())
    }
}

/// One finite-difference sample on a plane frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GradientSample {
    /// World position of the sample on the frame.
    pub position: Vec3,
    /// The display field the overlay must draw: `E = -∇V` for an electric
    /// potential, `∇f` for a generic scalar. `None` at a hole.
    pub gradient: Option<Vec3>,
    /// Largest component disagreement between `h` and `h / 2`; exactly `0.0`
    /// when `gradient` is `None`.
    pub error_estimate: f32,
    /// False when the sample is missing or over `max_error`.
    pub usable: bool,
    /// Whether a domain edge forced the inward one-sided stencil.
    pub one_sided: bool,
}

/// Sample the display field on the spec's frame at the #92 grid resolution.
///
/// Central differences use `h = spec.step`; exact domain edges fall back to the
/// inward one-sided stencil and flag `one_sided`. Any non-finite stencil
/// evaluation yields `gradient: None` and preserves the hole. `gradient` is the
/// field the overlay must draw: `E = -∇V` for
/// [`SeriesRole::ElectricPotential`] and `∇f` for [`SeriesRole::Scalar`]; the
/// negation is applied after differentiation and leaves the error indicator
/// sign-independent. The error estimate is the largest component difference
/// between the `h` and `h / 2` derivatives and is never fabricated for a
/// missing sample. Unsupported sources report the #92 unsupported/parse reason
/// instead of sampling. A finite `f64` gradient or error estimate that overflows
/// `f32` is reported as a hole (`gradient: None`, `error_estimate: 0.0`),
/// matching the #92 checked-height rule.
pub fn sample_gradient(
    spec: &GradientSpec,
    parameters: &ParameterSet,
    time: f64,
    _domain: &Domain,
) -> Result<Vec<GradientSample>, String> {
    spec.validate()?;
    // The #92 seam owns the explicit z reason, the parser message and the
    // committed-parameter substitution. Parsing `spec.expr` here makes the
    // sampled expression always match the request and cache identity.
    let expr = scalar_view_expr(&spec.expr, parameters).map_err(|error| error.to_string())?;
    // Sampling is frame-local, exactly like #92 `sample_grid`; `domain` is part
    // of the contract seam and the cache identity, not a second bound.
    let side = GRADIENT_GRID_SIDE;
    let context = SampleContext {
        expr: &expr,
        frame: &spec.frame,
        time,
        step: f64::from(spec.step),
        max_error: spec.max_error,
        half_u: f64::from(spec.frame.half_u),
        half_v: f64::from(spec.frame.half_v),
        sign: if spec.role == SeriesRole::ElectricPotential {
            -1.0
        } else {
            1.0
        },
    };
    let mut samples = Vec::with_capacity((side as usize) * (side as usize));
    for j in 0..side {
        let v = axis(j, side, context.half_v);
        for i in 0..side {
            let u = axis(i, side, context.half_u);
            samples.push(context.sample(u, v));
        }
    }
    Ok(samples)
}

#[derive(Debug, Clone, Copy)]
enum Axis {
    U,
    V,
}

struct SampleContext<'a> {
    expr: &'a ScalarExpr,
    frame: &'a PlaneFrame,
    time: f64,
    step: f64,
    max_error: f32,
    half_u: f64,
    half_v: f64,
    /// `-1` for the electric field `E = -∇V`, `1` for `∇f`.
    sign: f32,
}

impl SampleContext<'_> {
    fn sample(&self, u: f64, v: f64) -> GradientSample {
        let local = Vec3::new(u as f32, v as f32, 0.0);
        let position = self.frame.to_world(local);
        let position = if position.is_finite() {
            position
        } else {
            Vec3::splat(f32::NAN)
        };
        let one_sided = is_boundary(u, self.half_u) || is_boundary(v, self.half_v);

        let du = self.partial(Axis::U, u, v, self.step);
        let du_half = self.partial(Axis::U, u, v, self.step / 2.0);
        let dv = self.partial(Axis::V, u, v, self.step);
        let dv_half = self.partial(Axis::V, u, v, self.step / 2.0);

        let Some((du, du_half, dv, dv_half)) = du
            .zip(du_half)
            .zip(dv)
            .zip(dv_half)
            .map(|(((du, du_half), dv), dv_half)| (du, du_half, dv, dv_half))
        else {
            return GradientSample {
                position,
                gradient: None,
                error_estimate: 0.0,
                usable: false,
                one_sided,
            };
        };

        let world = (self.frame.right * (du as f32) + self.frame.up * (dv as f32)) * self.sign;
        let error = (du - du_half).abs().max((dv - dv_half).abs()) as f32;
        if !world.is_finite() || !error.is_finite() {
            return GradientSample {
                position,
                gradient: None,
                error_estimate: 0.0,
                usable: false,
                one_sided,
            };
        }
        GradientSample {
            position,
            gradient: Some(world),
            error_estimate: error,
            usable: error <= self.max_error,
            one_sided,
        }
    }

    /// One partial derivative using the central stencil or the inward one-sided
    /// stencil at an exact domain edge. A non-finite stencil evaluation is a
    /// hole.
    fn partial(&self, axis: Axis, u: f64, v: f64, h: f64) -> Option<f64> {
        let (coordinate, half) = match axis {
            Axis::U => (u, self.half_u),
            Axis::V => (v, self.half_v),
        };
        let sample = |delta: f64| match axis {
            Axis::U => self.expr.eval_at(u + delta, v, self.time),
            Axis::V => self.expr.eval_at(u, v + delta, self.time),
        };
        if coordinate <= -half {
            let inward = sample(h)?;
            let center = sample(0.0)?;
            Some((inward - center) / h)
        } else if coordinate >= half {
            let center = sample(0.0)?;
            let inward = sample(-h)?;
            Some((center - inward) / h)
        } else {
            let forward = sample(h)?;
            let backward = sample(-h)?;
            Some((forward - backward) / (2.0 * h))
        }
    }
}

fn is_boundary(coordinate: f64, half: f64) -> bool {
    coordinate <= -half || coordinate >= half
}

fn axis(index: u32, count: u32, half: f64) -> f64 {
    if count <= 1 {
        0.0
    } else {
        (index as f64 / (count - 1) as f64 * 2.0 - 1.0) * half
    }
}

fn truncate_label(label: &str) -> String {
    let mut end = MAX_SERIES_LABEL_BYTES.min(label.len());
    while end > 0 && !label.is_char_boundary(end) {
        end -= 1;
    }
    label[..end].to_owned()
}

fn validate_frame(frame: &PlaneFrame) -> Result<(), String> {
    let finite = [
        frame.origin,
        frame.right,
        frame.up,
        frame.normal,
        Vec3::new(frame.half_u, frame.half_v, 0.0),
    ]
    .iter()
    .all(|value| value.is_finite());
    if finite && frame.half_u >= 0.0 && frame.half_v >= 0.0 {
        Ok(())
    } else {
        Err("scalar view plane frame must be finite".to_owned())
    }
}

fn default_series_color() -> Rgb {
    SeriesStyle::FALLBACK_COLORS[0]
}

fn default_gradient_frame() -> PlaneFrame {
    PlaneFrame {
        axis: PlaneAxis::Z,
        origin: Vec3::ZERO,
        right: Vec3::X,
        up: Vec3::Y,
        normal: Vec3::Z,
        half_u: 1.0,
        half_v: 1.0,
    }
}

/// The cache identity of one gradient rebuild: spec fields, source text,
/// committed parameter fingerprint, domain, frame and the clock quantised to
/// [`SCALAR_RESAMPLE_INTERVAL`] (the #92 10 Hz rule).
#[derive(Debug, Clone, PartialEq)]
pub struct GradientCacheKey {
    role: SeriesRole,
    expr: String,
    step: f32,
    max_error: f32,
    frame: PlaneFrame,
    domain: Domain,
    parameters: ParameterSet,
    quantized_time: i64,
}

impl GradientCacheKey {
    /// Build the key from the request and the current committed state.
    pub fn new(spec: &GradientSpec, parameters: &ParameterSet, domain: &Domain, time: f64) -> Self {
        Self {
            role: spec.role,
            expr: spec.expr.clone(),
            step: spec.step,
            max_error: spec.max_error,
            frame: spec.frame,
            domain: *domain,
            parameters: parameters.clone(),
            quantized_time: quantized_time(time),
        }
    }
}

/// A one-entry bounded cache for gradient samples. Editing any key component
/// invalidates the retained record; an unchanged key is free. A failed rebuild
/// retains its reason until the key changes.
#[derive(Debug, Clone, Default)]
pub struct GradientCache {
    key: Option<GradientCacheKey>,
    samples: Option<Vec<GradientSample>>,
    error: Option<String>,
}

impl GradientCache {
    /// Whether the retained successful record still matches `key`.
    pub fn record_matches(&self, key: &GradientCacheKey) -> bool {
        self.samples.is_some() && self.key.as_ref() == Some(key)
    }

    /// Replace the retained entry with `result` for `key`. A failure clears the
    /// samples and stores the reason; both invalidate any older record.
    pub fn store(&mut self, key: GradientCacheKey, result: Result<Vec<GradientSample>, String>) {
        self.key = Some(key);
        match result {
            Ok(samples) => {
                self.samples = Some(samples);
                self.error = None;
            }
            Err(error) => {
                self.samples = None;
                self.error = Some(error);
            }
        }
    }

    /// The retained samples, if the last rebuild succeeded.
    pub fn samples(&self) -> Option<&[GradientSample]> {
        self.samples.as_deref()
    }

    /// The reason the last rebuild failed, if it did.
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }
}

fn quantized_time(time: f64) -> i64 {
    if time.is_finite() {
        (time / SCALAR_RESAMPLE_INTERVAL).floor() as i64
    } else {
        i64::MIN
    }
}

mod frame_serde {
    use super::{PlaneAxis, PlaneFrame, Vec3};
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    #[derive(Serialize, Deserialize)]
    struct FrameRecord {
        axis: String,
        origin: [f32; 3],
        right: [f32; 3],
        up: [f32; 3],
        normal: [f32; 3],
        half_u: f32,
        half_v: f32,
    }

    pub fn serialize<S: Serializer>(frame: &PlaneFrame, serializer: S) -> Result<S::Ok, S::Error> {
        FrameRecord {
            axis: frame.axis.label().to_owned(),
            origin: frame.origin.to_array(),
            right: frame.right.to_array(),
            up: frame.up.to_array(),
            normal: frame.normal.to_array(),
            half_u: frame.half_u,
            half_v: frame.half_v,
        }
        .serialize(serializer)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<PlaneFrame, D::Error> {
        let record = FrameRecord::deserialize(deserializer)?;
        let axis = match record.axis.as_str() {
            "x" => PlaneAxis::X,
            "y" => PlaneAxis::Y,
            "z" => PlaneAxis::Z,
            other => {
                return Err(serde::de::Error::custom(format!(
                    "unknown plane axis `{other}`"
                )));
            }
        };
        Ok(PlaneFrame {
            axis,
            origin: Vec3::from_array(record.origin),
            right: Vec3::from_array(record.right),
            up: Vec3::from_array(record.up),
            normal: Vec3::from_array(record.normal),
            half_u: record.half_u,
            half_v: record.half_v,
        })
    }
}
