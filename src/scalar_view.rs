//! Scalar views (issue #92 slice A): bounded grids, contours and probes on the
//! current plane frame, with honest missing samples and no second evaluator.
//!
//! The contract is `docs/superpowers/specs/2026-11-18-scalar-views-contract.md`.
//! Sampling goes through [`ScalarExpr::eval_at`] with [`ScalarContext::Surface`]
//! after committed parameters are substituted into the formula text. A grid
//! paints a cell only when all four corners are finite, and a contour segment
//! is emitted only inside such a cell. Nothing here is persisted except probe
//! positions and anchors; sampled values are runtime state.

use serde::{Deserialize, Serialize};

use crate::{
    domain::Domain,
    parameters::{ParameterSet, RESERVED_NAMES},
    plane::PlaneFrame,
    scalar::{ScalarContext, ScalarExpr},
    seed_layout::SeedAnchor,
};

use glam::{Vec2, Vec3};

/// Largest accepted grid side (160² = 25,600 evaluations per rebuild).
pub const MAX_GRID_SIDE: u32 = 160;
/// Smallest accepted grid side.
pub const MIN_GRID_SIDE: u32 = 8;
/// Largest accepted point count on one line probe.
pub const MAX_LINE_SAMPLES: usize = 512;
/// Smallest accepted point count on one line probe.
pub const MIN_LINE_SAMPLES: usize = 2;
/// Most point and line probes together in one scene.
pub const MAX_PROBES: usize = 8;
/// Most explicit contour levels.
pub const MAX_CONTOUR_LEVELS: usize = 16;
/// Coalesce animated scalar refreshes to at most one per 100 ms (10 Hz).
pub const SCALAR_RESAMPLE_INTERVAL: f64 = 0.1;
/// User-facing reason for a formula that refers to `z`.
pub const UNSUPPORTED_Z_REASON: &str = "scalar views support x/y formulas";

/// One evaluated scalar sample. `value` is `None` when evaluation failed at
/// this point (pole, domain exit, non-finite result), never a substituted
/// number.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScalarSample {
    /// World position of the sample on the plane frame.
    pub position: Vec3,
    /// The evaluated value, or `None` for an undefined point.
    pub value: Option<f32>,
}

/// A sampled rectangular region on one plane frame.
#[derive(Debug, Clone, PartialEq)]
pub struct ScalarGrid {
    /// The plane-local frame the grid samples.
    pub frame: PlaneFrame,
    /// Number of samples per row; `MIN_GRID_SIDE..=MAX_GRID_SIDE`.
    pub width: u32,
    /// Number of rows; `MIN_GRID_SIDE..=MAX_GRID_SIDE`.
    pub height: u32,
    /// Simulated time the grid was sampled at.
    pub time: f64,
    /// Row-major samples, `width * height` long.
    pub samples: Vec<ScalarSample>,
}

impl ScalarGrid {
    /// Validate an already-sampled grid before accepting it.
    ///
    /// Bounds and the sample count are checked before any caller can allocate
    /// from a hostile record.
    pub fn new(
        frame: PlaneFrame,
        width: u32,
        height: u32,
        time: f64,
        samples: Vec<ScalarSample>,
    ) -> Result<Self, ScalarViewError> {
        validate_grid_size(width, height)?;
        validate_frame(&frame)?;
        let expected = (width as usize) * (height as usize);
        if samples.len() != expected {
            return Err(ScalarViewError::SampleCount {
                expected,
                got: samples.len(),
            });
        }
        Ok(Self {
            frame,
            width,
            height,
            time,
            samples,
        })
    }

    /// Sample `(i, j)` in row-major order.
    pub fn sample(&self, i: u32, j: u32) -> &ScalarSample {
        assert!(i < self.width && j < self.height, "sample outside grid");
        &self.samples[(j as usize) * (self.width as usize) + i as usize]
    }

    /// Whether the cell below-right of sample `(i, j)` has four finite corners.
    pub fn cell_is_painted(&self, i: u32, j: u32) -> bool {
        if i + 1 >= self.width || j + 1 >= self.height {
            return false;
        }
        [
            self.sample(i, j),
            self.sample(i + 1, j),
            self.sample(i, j + 1),
            self.sample(i + 1, j + 1),
        ]
        .iter()
        .all(|sample| sample.value.is_some())
    }

    /// Number of cells painted with four finite corners.
    pub fn painted_cells(&self) -> usize {
        self.count_cells(true)
    }

    /// Number of cells left transparent because a corner is missing.
    pub fn unpainted_cells(&self) -> usize {
        self.count_cells(false)
    }

    fn count_cells(&self, painted: bool) -> usize {
        (0..self.height.saturating_sub(1))
            .map(|j| {
                (0..self.width.saturating_sub(1))
                    .filter(|&i| self.cell_is_painted(i, j) == painted)
                    .count()
            })
            .sum()
    }
}

/// One contour segment in the grid's plane-local `(u, v)` coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ContourSegment {
    /// Segment start on the plane.
    pub start: Vec2,
    /// Segment end on the plane.
    pub end: Vec2,
    /// The explicit contour level the segment follows.
    pub level: f32,
}

/// A point probe. The world position is authoritative; the anchor is the same
/// optional editable-placement record the seed layout uses.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PointProbe {
    /// World position, always finite.
    #[serde(with = "position_serde")]
    pub position: Vec3,
    /// The planting plane, or `None` for a world-only probe.
    #[serde(default)]
    pub anchor: Option<SeedAnchor>,
    /// Last evaluated value; runtime state, never persisted.
    #[serde(skip)]
    pub value: Option<f32>,
}

impl PointProbe {
    /// Validate a point probe before use. Evaluation is separate so a moved
    /// frame never changes the document position.
    pub fn new(position: Vec3, anchor: Option<SeedAnchor>) -> Result<Self, ScalarViewError> {
        if !position.is_finite() {
            return Err(ScalarViewError::NonFinitePosition);
        }
        Ok(Self {
            position,
            anchor,
            value: None,
        })
    }

    /// Evaluate through the shared [`ScalarExpr::eval_at`] seam after projecting
    /// the world position onto the frame's local `(x, y)` axes.
    pub fn evaluate(&mut self, expr: &ScalarExpr, frame: &PlaneFrame, time: f64) -> Option<f32> {
        let local = frame.to_local(self.position);
        self.value = evaluate_at(expr, f64::from(local.x), f64::from(local.y), time);
        self.value
    }
}

/// A line probe between two world positions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LineProbe {
    /// World start position, always finite.
    #[serde(with = "position_serde")]
    pub start: Vec3,
    /// World end position, always finite.
    #[serde(with = "position_serde")]
    pub end: Vec3,
    /// The planting plane, or `None` for a world-only probe.
    #[serde(default)]
    pub anchor: Option<SeedAnchor>,
    /// Last sampled points; runtime state, never persisted.
    #[serde(skip)]
    pub samples: Vec<ScalarSample>,
}

impl LineProbe {
    /// Validate the endpoints and any retained samples.
    ///
    /// An empty sample list is valid: a restored document holds the line and
    /// resamples it through the same seam when the scalar panel is visible.
    pub fn new(
        start: Vec3,
        end: Vec3,
        anchor: Option<SeedAnchor>,
        samples: Vec<ScalarSample>,
    ) -> Result<Self, ScalarViewError> {
        if !start.is_finite() || !end.is_finite() {
            return Err(ScalarViewError::NonFinitePosition);
        }
        if samples.len() > MAX_LINE_SAMPLES {
            return Err(ScalarViewError::LineSamples {
                min: MIN_LINE_SAMPLES,
                max: MAX_LINE_SAMPLES,
                got: samples.len(),
            });
        }
        if samples.iter().any(|sample| !sample.position.is_finite()) {
            return Err(ScalarViewError::NonFinitePosition);
        }
        Ok(Self {
            start,
            end,
            anchor,
            samples,
        })
    }
}

/// The durable point and line probes of one scene.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ProbeSet {
    /// Point probes in insertion order.
    pub points: Vec<PointProbe>,
    /// Line probes in insertion order.
    pub lines: Vec<LineProbe>,
}

impl ProbeSet {
    /// Total point and line probes.
    pub fn len(&self) -> usize {
        self.points.len() + self.lines.len()
    }

    /// Whether the scene has no probes.
    pub fn is_empty(&self) -> bool {
        self.points.is_empty() && self.lines.is_empty()
    }

    /// Append a point probe while the shared cap has room.
    pub fn push_point(&mut self, probe: PointProbe) -> Result<(), ScalarViewError> {
        if self.len() >= MAX_PROBES {
            return Err(ScalarViewError::ProbeLimit { max: MAX_PROBES });
        }
        self.points.push(probe);
        Ok(())
    }

    /// Append a line probe while the shared cap has room.
    pub fn push_line(&mut self, probe: LineProbe) -> Result<(), ScalarViewError> {
        if self.len() >= MAX_PROBES {
            return Err(ScalarViewError::ProbeLimit { max: MAX_PROBES });
        }
        self.lines.push(probe);
        Ok(())
    }

    /// Drop non-finite probes and anything past the cap, preserving order.
    pub fn sanitized(mut self) -> Self {
        self.points.retain(|probe| probe.position.is_finite());
        self.lines.retain(|probe| {
            probe.start.is_finite()
                && probe.end.is_finite()
                && probe.samples.len() <= MAX_LINE_SAMPLES
                && probe
                    .samples
                    .iter()
                    .all(|sample| sample.position.is_finite())
        });
        let total = self.len();
        if total > MAX_PROBES {
            let keep_points = self.points.len().min(MAX_PROBES);
            self.points.truncate(keep_points);
            self.lines.truncate(MAX_PROBES - keep_points);
        }
        self
    }
}

/// A request for one bounded scalar view rebuild. The plane-local `(u, v)` is
/// evaluated as the formula's `(x, y)`.
#[derive(Debug, Clone, PartialEq)]
pub struct ScalarViewRequest {
    /// Formula text as typed by the user.
    pub source: String,
    /// Committed shared named parameters.
    pub parameters: ParameterSet,
    /// The plane frame the grid covers.
    pub frame: PlaneFrame,
    /// The local domain covered by the frame.
    pub domain: Domain,
    /// Simulated time.
    pub time: f64,
    /// Grid side (the records are square).
    pub grid_side: u32,
    /// Explicit contour levels.
    pub levels: Vec<f32>,
}

/// The cache identity of one scalar view rebuild.
#[derive(Debug, Clone, PartialEq)]
pub struct ScalarViewKey {
    source: String,
    parameters: ParameterSet,
    frame: PlaneFrame,
    domain: Domain,
    quantized_time: i64,
    grid_side: u32,
    levels: Vec<f32>,
}

impl ScalarViewRequest {
    /// The cache key: formula, committed parameters, frame, domain and the
    /// clock quantised to [`SCALAR_RESAMPLE_INTERVAL`]. The grid side and
    /// explicit levels also change the records, so they are part of the key.
    pub fn key(&self) -> ScalarViewKey {
        ScalarViewKey {
            source: self.source.clone(),
            parameters: self.parameters.clone(),
            frame: self.frame,
            domain: self.domain,
            quantized_time: quantized_time(self.time),
            grid_side: self.grid_side,
            levels: self.levels.clone(),
        }
    }
}

/// One successfully rebuilt scalar view record.
#[derive(Debug, Clone, PartialEq)]
pub struct ScalarViewRecord {
    /// The sampled grid at the request's exact time.
    pub grid: ScalarGrid,
    /// Explicit contour segments in plane-local coordinates.
    pub contours: Vec<ContourSegment>,
}

/// The cache identity of a scalar view without its previewed contour values.
///
/// The explicit levels depend on a bounded preview of the field, so building a
/// full [`ScalarViewKey`] costs a sample. A collapsed panel still needs to know
/// whether its retained record describes the current state; this identity
/// carries the user-visible inputs (formula, parameters, frame, domain,
/// quantised clock, grid side and requested level count) without that preview.
#[derive(Debug, Clone, PartialEq)]
pub struct ScalarViewIdentity {
    source: String,
    parameters: ParameterSet,
    frame: PlaneFrame,
    domain: Domain,
    quantized_time: i64,
    grid_side: u32,
    level_count: usize,
}

impl ScalarViewIdentity {
    /// Build the identity from raw state. `time` is quantised exactly as
    /// [`ScalarViewRequest::key`] quantises it; `level_count` is the number of
    /// contour levels requested, not the previewed values.
    pub fn new(
        source: &str,
        parameters: &ParameterSet,
        frame: PlaneFrame,
        domain: Domain,
        time: f64,
        grid_side: u32,
        level_count: usize,
    ) -> Self {
        Self {
            source: source.to_owned(),
            parameters: parameters.clone(),
            frame,
            domain,
            quantized_time: quantized_time(time),
            grid_side,
            level_count,
        }
    }
}

impl ScalarViewKey {
    /// Whether this key and `identity` describe the same sampled grid. The
    /// heatmap only needs these fields: contour-level previews legitimately
    /// collapse to an empty list on a constant or zero-time field, and that
    /// must not hide the filled cells.
    fn matches_grid(&self, identity: &ScalarViewIdentity) -> bool {
        self.source == identity.source
            && self.parameters == identity.parameters
            && self.frame == identity.frame
            && self.domain == identity.domain
            && self.quantized_time == identity.quantized_time
            && self.grid_side == identity.grid_side
    }

    /// Whether this key and `identity` describe the same view apart from the
    /// previewed level values.
    fn matches_identity(&self, identity: &ScalarViewIdentity) -> bool {
        self.matches_grid(identity) && self.levels.len() == identity.level_count
    }
}

/// A one-entry, bounded cache for scalar view records.
///
/// Hidden panels never sample: [`ScalarViewCache::refresh`] returns the last
/// record (if any) without changing the key. Showing the panel after a key
/// change performs exactly one rebuild; an unchanged key is free. An invalid
/// formula stores its reason and is not retried until the key changes.
#[derive(Debug, Clone, Default)]
pub struct ScalarViewCache {
    key: Option<ScalarViewKey>,
    record: Option<ScalarViewRecord>,
    error: Option<String>,
    resamples: u64,
}

impl ScalarViewCache {
    /// Refresh only while visible; return the current record, rebuilding at
    /// most once per changed key.
    pub fn refresh(
        &mut self,
        request: &ScalarViewRequest,
        visible: bool,
    ) -> Option<&ScalarViewRecord> {
        if !visible {
            return self.record.as_ref();
        }
        let key = request.key();
        if self.key.as_ref() == Some(&key) {
            return self.record.as_ref();
        }
        self.key = Some(key);
        self.resamples = self.resamples.saturating_add(1);
        match sample_scalar_view(request) {
            Ok(record) => {
                self.record = Some(record);
                self.error = None;
            }
            Err(error) => {
                self.record = None;
                self.error = Some(error.to_string());
            }
        }
        self.record.as_ref()
    }

    /// The last successfully rebuilt record.
    pub fn record(&self) -> Option<&ScalarViewRecord> {
        self.record.as_ref()
    }

    /// Whether the retained record still describes `identity`'s formula,
    /// parameters, frame, domain, quantised clock, grid side and requested
    /// contour-level count. Concrete level values are excluded because they
    /// depend on the previewed value range; a collapsed panel uses this check
    /// without paying for that preview.
    pub fn record_matches(&self, identity: &ScalarViewIdentity) -> bool {
        self.record.is_some()
            && self
                .key
                .as_ref()
                .is_some_and(|key| key.matches_identity(identity))
    }

    /// Whether the retained record still describes `identity`'s sampled grid,
    /// ignoring contour levels. A heatmap is built only from that grid, and a
    /// constant or zero-time field can produce an empty level list even though
    /// the cells are valid, so the contour-preview check would wrongly hide it.
    pub fn record_matches_grid(&self, identity: &ScalarViewIdentity) -> bool {
        self.record.is_some()
            && self
                .key
                .as_ref()
                .is_some_and(|key| key.matches_grid(identity))
    }

    /// The reason the last rebuild failed, if it did.
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    /// Number of rebuilds performed; the app tests use this to prove hidden
    /// panels and throttled frames do no work.
    pub fn resamples(&self) -> u64 {
        self.resamples
    }
}

/// Parse a scalar-view formula: substitute committed parameters, then parse
/// with [`ScalarContext::Surface`]. A formula that references `z` reports the
/// explicit unsupported reason instead of parser internals.
pub fn scalar_view_expr(
    source: &str,
    parameters: &ParameterSet,
) -> Result<ScalarExpr, ScalarViewError> {
    let substituted = substitute_parameters(source, parameters);
    ScalarExpr::parse(&substituted, ScalarContext::Surface).map_err(|error| {
        if references_z(source) {
            ScalarViewError::UnsupportedZ
        } else {
            ScalarViewError::Parse(error.to_string())
        }
    })
}

/// Sample a full grid through the shared evaluator.
///
/// The requested side is validated before the sample vector is allocated. The
/// grid covers `frame.local_domain()`; each sample maps local `(u, v)` to the
/// formula's `(x, y)` and to a world position on the frame.
pub fn sample_grid(
    expr: &ScalarExpr,
    frame: PlaneFrame,
    width: u32,
    height: u32,
    time: f64,
) -> Result<ScalarGrid, ScalarViewError> {
    validate_grid_size(width, height)?;
    validate_frame(&frame)?;
    let mut samples = Vec::with_capacity((width as usize) * (height as usize));
    for j in 0..height {
        let v = axis(j, height, f64::from(frame.half_v));
        for i in 0..width {
            let u = axis(i, width, f64::from(frame.half_u));
            let value = evaluate_at(expr, u, v, time);
            let position = frame.to_world(Vec3::new(u as f32, v as f32, 0.0));
            samples.push(ScalarSample {
                position: if position.is_finite() {
                    position
                } else {
                    Vec3::splat(f32::NAN)
                },
                value,
            });
        }
    }
    ScalarGrid::new(frame, width, height, time, samples)
}

/// Sample a line through the shared evaluator, preserving missing values.
///
/// `count` is validated before allocation. Each world point is projected to the
/// frame's local `(x, y)`; a world-only line still evaluates on the frame.
pub fn sample_line(
    expr: &ScalarExpr,
    frame: &PlaneFrame,
    start: Vec3,
    end: Vec3,
    count: usize,
    time: f64,
) -> Result<Vec<ScalarSample>, ScalarViewError> {
    if !(MIN_LINE_SAMPLES..=MAX_LINE_SAMPLES).contains(&count) {
        return Err(ScalarViewError::LineSamples {
            min: MIN_LINE_SAMPLES,
            max: MAX_LINE_SAMPLES,
            got: count,
        });
    }
    if !start.is_finite() || !end.is_finite() {
        return Err(ScalarViewError::NonFinitePosition);
    }
    validate_frame(frame)?;
    let mut samples = Vec::with_capacity(count);
    for index in 0..count {
        let t = index as f64 / (count - 1) as f64;
        let position = start.as_dvec3().lerp(end.as_dvec3(), t).as_vec3();
        let local = frame.to_local(position);
        samples.push(ScalarSample {
            position,
            value: evaluate_at(expr, f64::from(local.x), f64::from(local.y), time),
        });
    }
    Ok(samples)
}

/// Build a scalar view record: parse, sample the grid, then extract contours.
pub fn sample_scalar_view(
    request: &ScalarViewRequest,
) -> Result<ScalarViewRecord, ScalarViewError> {
    let expr = scalar_view_expr(&request.source, &request.parameters)?;
    let grid = sample_grid(
        &expr,
        request.frame,
        request.grid_side,
        request.grid_side,
        request.time,
    )?;
    let contours = contour_segments(&grid, &request.levels)?;
    Ok(ScalarViewRecord { grid, contours })
}

/// Marching-squares segments for explicit, validated contour levels.
///
/// A segment is emitted only inside a cell whose four corners are finite. No
/// smoothing, implicit level guessing or interpolation across holes happens.
pub fn contour_segments(
    grid: &ScalarGrid,
    levels: &[f32],
) -> Result<Vec<ContourSegment>, ScalarViewError> {
    validate_levels(levels)?;
    let mut segments = Vec::new();
    for j in 0..grid.height.saturating_sub(1) {
        for i in 0..grid.width.saturating_sub(1) {
            let corners = [
                grid.sample(i, j),
                grid.sample(i + 1, j),
                grid.sample(i, j + 1),
                grid.sample(i + 1, j + 1),
            ];
            let [a, b, c, d] = corners;
            let (Some(a_value), Some(b_value), Some(c_value), Some(d_value)) =
                (a.value, b.value, c.value, d.value)
            else {
                continue;
            };
            let values = [a_value, b_value, c_value, d_value];
            let local = |sample: &ScalarSample| {
                let local = grid.frame.to_local(sample.position);
                Vec2::new(local.x, local.y)
            };
            let points = [local(a), local(b), local(c), local(d)];
            for &level in levels {
                let mut crossings = Vec::with_capacity(4);
                for [first, second] in [[0, 1], [1, 2], [2, 3], [3, 0]] {
                    if let Some(point) = edge_crossing(
                        points[first],
                        points[second],
                        values[first],
                        values[second],
                        level,
                    ) {
                        crossings.push(point);
                    }
                }
                match crossings.as_slice() {
                    [start, end] => segments.push(ContourSegment {
                        start: *start,
                        end: *end,
                        level,
                    }),
                    [first, second, third, fourth] => {
                        segments.push(ContourSegment {
                            start: *first,
                            end: *second,
                            level,
                        });
                        segments.push(ContourSegment {
                            start: *third,
                            end: *fourth,
                            level,
                        });
                    }
                    _ => {}
                }
            }
        }
    }
    Ok(segments)
}

/// A bounded scalar-view failure. User-facing text is lowercase and names the
/// limit; the `z` reason is stable for the panel.
#[derive(Debug, Clone, PartialEq)]
pub enum ScalarViewError {
    /// A grid side was outside the accepted bounds.
    GridSize { min: u32, max: u32, got: u32 },
    /// A grid's sample vector did not match `width * height`.
    SampleCount { expected: usize, got: usize },
    /// A line sample count was outside the accepted bounds.
    LineSamples { min: usize, max: usize, got: usize },
    /// Contour levels were non-finite, not strictly increasing or too many.
    ContourLevels { reason: String },
    /// A plane frame carried a non-finite component or negative half extent.
    NonFiniteFrame,
    /// A probe position was not finite.
    NonFinitePosition,
    /// The shared probe cap was reached.
    ProbeLimit { max: usize },
    /// A formula referenced `z`, which a scalar view cannot map.
    UnsupportedZ,
    /// The parser rejected the formula.
    Parse(String),
}

impl std::fmt::Display for ScalarViewError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::GridSize { min, max, got } => {
                write!(f, "scalar grid side {got} is outside {min}..={max}")
            }
            Self::SampleCount { expected, got } => write!(
                f,
                "scalar grid sample count {got} does not match the expected {expected}"
            ),
            Self::LineSamples { min, max, got } => {
                write!(f, "line probe sample count {got} is outside {min}..={max}")
            }
            Self::ContourLevels { reason } => write!(f, "invalid contour levels: {reason}"),
            Self::NonFiniteFrame => write!(f, "scalar view plane frame must be finite"),
            Self::NonFinitePosition => write!(f, "scalar probe position must be finite"),
            Self::ProbeLimit { max } => write!(f, "at most {max} probes are allowed"),
            Self::UnsupportedZ => write!(f, "{UNSUPPORTED_Z_REASON}"),
            Self::Parse(message) => write!(f, "{message}"),
        }
    }
}

impl std::error::Error for ScalarViewError {}

fn validate_grid_size(width: u32, height: u32) -> Result<(), ScalarViewError> {
    for got in [width, height] {
        if !(MIN_GRID_SIDE..=MAX_GRID_SIDE).contains(&got) {
            return Err(ScalarViewError::GridSize {
                min: MIN_GRID_SIDE,
                max: MAX_GRID_SIDE,
                got,
            });
        }
    }
    Ok(())
}

fn validate_frame(frame: &PlaneFrame) -> Result<(), ScalarViewError> {
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
        Err(ScalarViewError::NonFiniteFrame)
    }
}

fn validate_levels(levels: &[f32]) -> Result<(), ScalarViewError> {
    if levels.len() > MAX_CONTOUR_LEVELS {
        return Err(ScalarViewError::ContourLevels {
            reason: format!("at most {MAX_CONTOUR_LEVELS} levels are allowed"),
        });
    }
    let mut previous: Option<f32> = None;
    for &level in levels {
        if !level.is_finite() {
            return Err(ScalarViewError::ContourLevels {
                reason: "levels must be finite".to_string(),
            });
        }
        if previous.is_some_and(|previous| level <= previous) {
            return Err(ScalarViewError::ContourLevels {
                reason: "levels must be strictly increasing".to_string(),
            });
        }
        previous = Some(level);
    }
    Ok(())
}

fn quantized_time(time: f64) -> i64 {
    if time.is_finite() {
        (time / SCALAR_RESAMPLE_INTERVAL).floor() as i64
    } else {
        i64::MIN
    }
}

fn axis(index: u32, count: u32, half: f64) -> f64 {
    if count <= 1 {
        0.0
    } else {
        (index as f64 / (count - 1) as f64 * 2.0 - 1.0) * half
    }
}

fn evaluate_at(expr: &ScalarExpr, x: f64, y: f64, time: f64) -> Option<f32> {
    let value = expr.eval_at(x, y, time)? as f32;
    value.is_finite().then_some(value)
}

fn edge_crossing(
    start: Vec2,
    end: Vec2,
    start_value: f32,
    end_value: f32,
    level: f32,
) -> Option<Vec2> {
    if (start_value >= level) == (end_value >= level) {
        return None;
    }
    let denominator = end_value - start_value;
    if denominator == 0.0 || !denominator.is_finite() {
        return None;
    }
    let t = ((level - start_value) / denominator).clamp(0.0, 1.0);
    let point = start + (end - start) * t;
    point.is_finite().then_some(point)
}

/// Rewrite free identifiers that name a committed parameter to their value.
/// A call `name(` is left alone, as are reserved formula names.
fn substitute_parameters(source: &str, parameters: &ParameterSet) -> String {
    let bytes = source.as_bytes();
    let mut out = String::with_capacity(source.len());
    let mut i = 0;
    while i < bytes.len() {
        let byte = bytes[i];
        if byte.is_ascii_digit() || byte == b'.' {
            let start = i;
            while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b'.') {
                i += 1;
            }
            if i < bytes.len() && (bytes[i] == b'e' || bytes[i] == b'E') {
                let mut end = i + 1;
                if end < bytes.len() && (bytes[end] == b'+' || bytes[end] == b'-') {
                    end += 1;
                }
                if end < bytes.len() && bytes[end].is_ascii_digit() {
                    i = end;
                    while i < bytes.len() && bytes[i].is_ascii_digit() {
                        i += 1;
                    }
                }
            }
            out.push_str(&source[start..i]);
        } else if byte.is_ascii_alphabetic() || byte == b'_' {
            let start = i;
            i += 1;
            while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                i += 1;
            }
            let name = &source[start..i];
            let is_call = source[i..].trim_start().starts_with('(');
            match parameters.value(name) {
                Some(value) if !is_call && !RESERVED_NAMES.contains(&name) => {
                    out.push('(');
                    out.push_str(&format!("{value}"));
                    out.push(')');
                }
                _ => out.push_str(name),
            }
        } else {
            let ch = source[i..].chars().next().expect("valid char boundary");
            out.push(ch);
            i += ch.len_utf8();
        }
    }
    out
}

fn references_z(source: &str) -> bool {
    let bytes = source.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let byte = bytes[i];
        if byte.is_ascii_alphabetic() || byte == b'_' {
            let start = i;
            i += 1;
            while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                i += 1;
            }
            if &source[start..i] == "z" {
                return true;
            }
        } else {
            let ch = source[i..].chars().next().expect("valid char boundary");
            i += ch.len_utf8();
        }
    }
    false
}

mod position_serde {
    use glam::Vec3;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S: Serializer>(position: &Vec3, serializer: S) -> Result<S::Ok, S::Error> {
        position.to_array().serialize(serializer)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec3, D::Error> {
        let [x, y, z] = <[f32; 3]>::deserialize(deserializer)?;
        Ok(Vec3::new(x, y, z))
    }
}
