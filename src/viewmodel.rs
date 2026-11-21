//! The viewmodel seam: the app's whole visible state and the pure snapshot
//! builder that turns it into the GPU scene.
//!
//! Spec ruling: exactly ONE new seam — pure functions state→SceneGPU and
//! input→camera actions, so the UI layer stays thin and the wiring logic is
//! testable without a window. No GPU types beyond the plain `SceneGPU`
//! instance layouts.

use super::camera::OrbitCamera;
use super::colormap::Colormap;
use super::domain::Domain;
use super::field::FieldExpr;
use super::flow::{Flow, SceneFlow};
use super::plane::{DIM_ALPHA, PlaneAxis, PlaneFrame, PlaneView, dim_color, on_plane};
use super::renderer::{
    ArrowInstanceGPU, HeatmapGPU, HeatmapLegend, LineInstanceGPU, SceneGPU, heatmap_legend,
};
use super::sampling::{ArrowData, ViewOptions, magnitude_range, sample_field_at, sample_plane_at};
use super::source_scene::{FieldMode, SourceSampling, SourceScene, sample_source_scene};
use super::streamlines::{
    CLICK_BURST_SEEDS, StreamlineOpts, burst_seeds, explicit_streamline_seeds, seed_grid, trace_at,
};
use crate::arrow_style::{ArrowParams, ArrowStyle, ArrowStyle2D};
use crate::color_mapping::{ColorMapping, ColorPalette, ColorQuantity, ColorSample, ColorStats};
use crate::color_ramp::{ColorRamp, ScaleMode};
use crate::induction::{
    InductionPrepared, InductionSampling, MAX_INDUCTION_DENSITY_2D, MAX_INDUCTION_DENSITY_3D,
    induction_key, prepare_induction, sample_induction,
};
use crate::layer_style::{LayerId, LayerStyles, PhysicsFamily, ResolvedLayerStyle};
use crate::material::{
    MaterialError, MaterialFamily, MaterialSampling, MaterialScene, material_family,
    material_outline, sample_materials, validate_material,
};
use crate::maxwell::{
    BemSolution, MAXWELL_DENSITY_CAP_WARNING, MAXWELL_MAX_DENSITY_2D, MAXWELL_MAX_DENSITY_3D,
    MaxwellError, MaxwellFamily, MaxwellSampling, MaxwellScene, maxwell_family, maxwell_outline,
    sample_maxwell_with_bem, solve_bem, validate_maxwell,
};
use crate::ode::{OdeChart, OdeError, OdeScene, OdeSolution, prepare_ode, solve_ode};
use crate::parameters::ParameterSet;
use crate::particles::{
    DOT_SPACING_FRACTION, FlowLayerStyle, MAX_PARTICLES, ParticleAppearance,
    effective_particle_count,
};
use crate::plot::{curve_edges_at, sample_curve_at};
use crate::reference::{sanitized_opacity, scaled_viewport_style};
use crate::scalar::{ScalarContext, ScalarExpr};
use crate::scalar_analysis::{
    ComparisonSeries, ComparisonSet, GradientSample, GradientSpec, SeriesRole, sample_gradient,
};
use crate::scalar_view::{
    MAX_CONTOUR_LEVELS, MAX_GRID_SIDE, MIN_GRID_SIDE, PointProbe, ProbeSet,
    SCALAR_RESAMPLE_INTERVAL, ScalarViewCache, ScalarViewError, ScalarViewIdentity,
    ScalarViewRecord, ScalarViewRequest, sample_grid, scalar_view_expr,
};
use crate::seed_layout::{
    MAX_GENERATED_PER_APPLICATION, MAX_SEEDS, SeedAnchor, SeedError, SeedLayout, SeedPoint,
};
use crate::viewport_style::ViewportStyle;

use std::sync::Arc;

mod plot;

use glam::{UVec3, Vec2, Vec3};
use serde::{Deserialize, Serialize};

/// Number of samples along one axis of a 2D grid.
pub const DEFAULT_DENSITY_2D: u32 = 24;
/// Number of samples along one axis of the default 3D grid.
pub const DEFAULT_DENSITY_3D: u32 = 16;
/// Density the sidebar clamps the 2D grid to.
pub const MAX_DENSITY_2D: u32 = 200;
/// Density the sidebar clamps the 3D grid to.
pub const MAX_DENSITY_3D: u32 = 40;
/// Densest supported 3D Materials grid.
pub const MATERIAL_MAX_DENSITY_3D: u32 = 40;
/// Densest supported 2D Materials slice grid.
pub const MATERIAL_MAX_DENSITY_2D: u32 = 200;
/// The smallest density the sidebar offers.
pub const MIN_DENSITY: u32 = 4;
/// Default scalar-view grid side (issue #92 slice B1).
pub const DEFAULT_SCALAR_GRID_SIDE: u32 = 64;
/// Default explicit contour-level count (issue #92 slice B1).
pub const DEFAULT_SCALAR_CONTOUR_LEVELS: u32 = 8;
/// Smallest grid side the Scalar view section offers (step 8 up to
/// [`MAX_GRID_SIDE`]).
pub const MIN_SCALAR_GRID_SIDE: u32 = 16;
/// Fallback text colour for scalar overlay lines before the UI first syncs the
/// active theme's text colour.
pub const DEFAULT_SCALAR_TEXT_COLOR: [f32; 4] = [0.72, 0.72, 0.72, 1.0];
/// Bounds and default of the particle dot size (ticket 30): a multiplier on
/// [`DOT_SPACING_FRACTION`], so `1.0` keeps the historical radius and the
/// fraction itself never moves.
pub const MIN_PARTICLE_SIZE: f32 = 0.25;
pub const MAX_PARTICLE_SIZE: f32 = 3.0;
pub const DEFAULT_PARTICLE_SIZE: f32 = 1.0;
/// The 2D plane is the domain this much wider, so the field fills the window.
pub const PLANE_MARGIN: f32 = 1.35;
/// Bounds of the arrow-length slider.
pub const MIN_ARROW_SCALE: f32 = 0.1;
pub const MAX_ARROW_SCALE: f32 = 4.0;
/// Bounds of the global arrow-opacity slider (ticket 36): a floor rather than
/// zero so a de-emphasised field never disappears, and `1.0` (the default) is
/// fully opaque.
pub const MIN_ARROW_OPACITY: f32 = 0.05;
pub const MAX_ARROW_OPACITY: f32 = 1.0;
/// Bounds of the plane-mode off-plane opacity (ticket 36). Its default is the
/// historical [`DIM_ALPHA`], not a second constant that could drift.
pub const MIN_PLANE_DIM_OPACITY: f32 = 0.0;
pub const MAX_PLANE_DIM_OPACITY: f32 = 1.0;
/// Bounds and default of the Proportional contrast exponent: the power applied
/// to `mag / max_mag` before the arrow length is scaled. `1.0` is the plain
/// linear mapping; below it lifts short arrows, above it pushes them down.
pub const MIN_PROPORTIONAL_EXPONENT: f32 = 0.25;
pub const MAX_PROPORTIONAL_EXPONENT: f32 = 4.0;
pub const DEFAULT_PROPORTIONAL_EXPONENT: f32 = 1.0;
/// Bounds and default of the magnitude scale: world units per unit of field
/// magnitude in [`ArrowLengthMode::Magnitude`]. Bounded here so the widget and
/// the seam share one range; the resulting arrow length itself is not clamped.
pub const MIN_MAGNITUDE_SCALE: f32 = 0.01;
pub const MAX_MAGNITUDE_SCALE: f32 = 10.0;
pub const DEFAULT_MAGNITUDE_SCALE: f32 = 1.0;
/// Bounds of one domain extent, world units. The editor keeps the sampled box
/// finite and in a range where a drag move is still meaningful; the seam's
/// [`clamp_domain_extent`] is the single place both the widgets and the
/// per-frame handoff take the bound from.
pub const MIN_DOMAIN_EXTENT: f32 = -100.0;
pub const MAX_DOMAIN_EXTENT: f32 = 100.0;
/// Line width streamlines are painted with, world units.
pub const STREAMLINE_WIDTH: f32 = 0.012;
/// Fixed point budget per streamline. Streamlines are not clock consumers, so
/// their length no longer follows a speed slider; the transport's clock speed
/// only animates particles (ticket 26).
pub const STREAMLINE_MAX_LEN: usize = 120;

/// Fallback when a streamline segment cannot be colour-mapped (a zero or
/// non-finite flow vector under an active [`ColorMapping`]).
const STREAMLINE_MID_GRAY: [u8; 3] = [128, 128, 128];

/// Seed jitter as a fraction of a seed-grid cell (`seed_grid` caps it below
/// half a cell so lattice neighbours never swap).
pub const STREAMLINE_JITTER: f32 = 0.45;
/// Hard cap on the explicit streamline seed count; `0` means Auto
/// (density-derived).
pub const MAX_STREAMLINES: u32 = 4096;

/// The starting field for each mode: the Pump preset (2D) and the 3D radial
/// preset, so a fresh app shows something immediately.
pub const FORMULA_2D: &str = "(x, y)";
pub const FORMULA_3D: &str = "(x, y, z)";

/// How a sampled field magnitude becomes an arrow's world-space length
/// (ticket 33). Replaces the old `normalize` bool; persisted records that still
/// carry one migrate through [`ArrowLengthMode::from_normalize`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ArrowLengthMode {
    /// Length follows magnitude, normalised so the longest arrow is exactly
    /// `arrow_scale` grid cells. The default.
    #[default]
    Proportional,
    /// Every arrow is exactly `arrow_scale` grid cells; only direction
    /// survives.
    Uniform,
    /// Length is the sampled magnitude itself in world units, multiplied by
    /// `magnitude_scale`, so fields of different magnitude can be compared
    /// directly. The arrow scale does not apply in this mode.
    Magnitude,
}

impl ArrowLengthMode {
    /// Every mode, in the order the Display selector shows them.
    pub const ALL: [ArrowLengthMode; 3] = [
        ArrowLengthMode::Proportional,
        ArrowLengthMode::Uniform,
        ArrowLengthMode::Magnitude,
    ];

    /// Short label for the mode selector.
    pub fn label(self) -> &'static str {
        match self {
            ArrowLengthMode::Proportional => "Proportional",
            ArrowLengthMode::Uniform => "Uniform",
            ArrowLengthMode::Magnitude => "Magnitude",
        }
    }

    /// One-line explanation, short enough for a tooltip.
    pub fn tooltip(self) -> &'static str {
        match self {
            ArrowLengthMode::Proportional => {
                "Length follows magnitude; the longest arrow is the arrow length scale"
            }
            ArrowLengthMode::Uniform => {
                "Every arrow has the same length, keeping only its direction"
            }
            ArrowLengthMode::Magnitude => "Length is the field magnitude in world units",
        }
    }

    /// The mode a legacy persisted `normalize` bool means: `true` painted every
    /// arrow the same length, `false` kept the magnitude proportion.
    pub fn from_normalize(normalize: bool) -> Self {
        if normalize {
            ArrowLengthMode::Uniform
        } else {
            ArrowLengthMode::Proportional
        }
    }
}

/// How streamlines get their seeds (ticket 50). The Feature toggle still gates
/// both sources; this mode only picks which seeds the on state paints.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum StreamlineSeedMode {
    /// The automatic jittered grid seeds plus any click seeds. The default, so
    /// a left click is never dead.
    #[default]
    Auto,
    /// Click seeds only; the automatic grid seeds are hidden.
    Click,
}

impl StreamlineSeedMode {
    /// Every mode, in the order the sidebar selector shows them.
    pub const ALL: [StreamlineSeedMode; 2] = [StreamlineSeedMode::Auto, StreamlineSeedMode::Click];

    /// Short label for the selector.
    pub fn label(self) -> &'static str {
        match self {
            StreamlineSeedMode::Auto => "Auto",
            StreamlineSeedMode::Click => "Click",
        }
    }

    /// One-line explanation, short enough for a tooltip.
    pub fn tooltip(self) -> &'static str {
        match self {
            StreamlineSeedMode::Auto => "Show the automatic grid seeds and accept clicks",
            StreamlineSeedMode::Click => "Show only the seeds planted by clicks",
        }
    }
}

/// What one explicit left click does (ticket 60). The S/P shortcuts use the
/// same two actions regardless of this setting: the selector only changes the
/// mouse gesture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum LeftClickAction {
    /// Plant one click streamline seed; no particle burst. The default.
    #[default]
    SeedStreamline,
    /// Add one particle burst; store no click seed.
    AddParticles,
}

impl LeftClickAction {
    /// Every action, in the order the Features row shows them.
    pub const ALL: [LeftClickAction; 2] = [
        LeftClickAction::SeedStreamline,
        LeftClickAction::AddParticles,
    ];

    /// Short label for the selector.
    pub fn label(self) -> &'static str {
        match self {
            LeftClickAction::SeedStreamline => "Seed streamline",
            LeftClickAction::AddParticles => "Add particles",
        }
    }

    /// One-line explanation, short enough for a tooltip.
    pub fn tooltip(self) -> &'static str {
        match self {
            LeftClickAction::SeedStreamline => {
                "A left click seeds one streamline (S does the same)"
            }
            LeftClickAction::AddParticles => {
                "A left click adds a particle burst in 3D (P does the same)"
            }
        }
    }
}

/// The state the user can touch: what field, how dense, how coloured, how
/// projected, from where.
#[derive(Clone)]
pub struct State {
    pub plot: crate::plot_state::PlotState,
    pub mode_3d: bool,
    pub formula: String,
    /// Durable shared named parameters (issue #89). Only
    /// [`State::commit_parameters`] may replace it in response to an edit.
    pub parameters: ParameterSet,
    /// Durable viewport/reference colours and widths (issue #82). Only
    /// [`State::commit_viewport_style`] may replace it in response to an edit;
    /// startup restores assign it directly.
    pub viewport_style: ViewportStyle,
    /// Durable per-layer and per-family appearance overrides (issue #84 slice
    /// A1). Only [`State::commit_layer_styles`] may replace it in response to
    /// an edit; an invalid commit leaves the previous record untouched.
    pub layer_styles: LayerStyles,
    pub field: FieldExpr,
    pub parse_error: String,
    pub density: u32,
    pub domain: Domain,
    pub arrow_scale: f32,
    /// Global arrow de-emphasis (ticket 36): the alpha every arrow is painted
    /// with, `1.0` (fully opaque) by default.
    pub arrow_opacity: f32,
    /// Opacity of arrows outside the selected slice in plane-by-plane mode
    /// (ticket 36). The effective off-plane alpha is `arrow_opacity *
    /// plane_dim_opacity`; the default is [`DIM_ALPHA`].
    pub plane_dim_opacity: f32,
    pub arrow_length_mode: ArrowLengthMode,
    pub magnitude_scale: f32,
    /// Power applied to `mag / max_mag` in [`ArrowLengthMode::Proportional`]
    /// (ticket 57). Independent of the two scales; `1.0` is linear.
    pub proportional_exponent: f32,
    pub colormap: Colormap,
    /// Custom colour ramp override (issue #83); `None` uses [`Self::colormap`].
    pub color_ramp: Option<ColorRamp>,
    /// Value-to-ramp scaling for colouring (issue #83).
    pub color_mode: ScaleMode,
    /// What the colour represents (issue #83).
    pub color_quantity: ColorQuantity,
    pub auto_fit: bool,
    pub color_min: f32,
    pub color_max: f32,
    pub arrows: bool,
    /// Shared across 2D and 3D: show the grid and domain box.
    pub show_grid: bool,
    /// Shared across 2D and 3D: show the coloured world axes.
    pub show_coordinate_system: bool,
    /// The 3D arrow family (ticket 45); it never selects the 2D arrow.
    pub arrow_style: ArrowStyle,
    /// The 2D arrow family (ticket 61); independent of [`State::arrow_style`].
    pub arrow_style_2d: ArrowStyle2D,
    /// The head/shaft proportions shared by every 3D family.
    pub arrow_params: ArrowParams,
    pub streamlines: bool,
    /// Which seeds the on state paints (ticket 50): the automatic grid, the
    /// click bursts, or both.
    pub seed_mode: StreamlineSeedMode,
    /// Explicit streamline seed count; `0` means Auto (density-derived).
    pub streamline_count: u32,
    /// Durable explicit streamline seeds (issue #87 slice S1). Only
    /// [`State::commit_seed_layout`] may replace it in response to an edit;
    /// startup restores assign it directly. S2 wires generation to it; until
    /// then the automatic grid still supplies the streamline seeds.
    pub seed_layout: SeedLayout,
    /// Durable scalar point/line probes (issue #92 slice A). Only the startup
    /// restore and the committed probe edits replace it; sampled values are
    /// runtime state and are never persisted.
    pub probes: ProbeSet,
    /// Durable display-only comparison series (issue #93 slice A). Only
    /// [`State::set_comparison_series`] replaces them in response to an edit;
    /// startup restore assigns directly. They never reach arrows, colour
    /// mapping or probes.
    pub comparison_series: ComparisonSet,
    /// Durable potential-gradient request (issue #93 slice A). Only
    /// [`State::set_gradient`] replaces it in response to an edit; startup
    /// restore assigns directly.
    pub gradient: Option<GradientSpec>,
    /// The Scalar view section's grid side (issue #92 slice B1). Runtime
    /// configuration, not document state; the request key invalidates the
    /// cached records when it changes.
    scalar_grid_side: u32,
    /// The explicit contour-level count the panel generates (issue #92 slice
    /// B1). `0` hides contours.
    scalar_contour_levels: u32,
    /// Persisted Scalar view toggle (issue #92 slice B2). `false` keeps the
    /// historical scene with no heatmap draw or upload.
    show_scalar_heatmap: bool,
    /// The active theme's text colour, mirrored by the sidebar each frame so
    /// contour and probe lines match the panel text in captures.
    scalar_text_color: [f32; 4],
    /// One-entry bounded scalar-view cache (issue #92 slice A). Runtime only;
    /// only [`State::refresh_scalar_view`] touches it, and only while the
    /// scalar panel is visible.
    scalar_view_cache: ScalarViewCache,
    /// Memoised explicit contour levels for the bounded preview (issue #92
    /// slice B1). Keyed by the state identity without level values, so a
    /// visible but idle frame does not re-evaluate the 32² preview.
    scalar_preview_levels: Option<(ScalarViewIdentity, Vec<f32>)>,
    /// One-entry comparison-curve and gradient-arrow cache (issue #93 slice B).
    /// Runtime only; [`State::refresh_scalar_analysis`] is its only sampler and
    /// it runs only while the Scalar analysis subsection is open.
    scalar_analysis: ScalarAnalysisRuntime,
    /// What one explicit left click does (ticket 60). The S/P shortcuts ignore
    /// it and always run their own action.
    pub left_click_action: LeftClickAction,
    pub particles: bool,
    /// Dot radius multiplier on the base grid-spacing fraction (ticket 30).
    /// Painted by the per-frame dot rebuild, so changing it is never a scene
    /// rebuild.
    pub particle_size: f32,
    /// Explicit particle count; `0` means Auto (density x 16) (ticket 30).
    /// Also consumed by the per-frame cloud, not the scene snapshot.
    pub particle_count: u32,
    /// Per-particle appearance (ticket 48): opacity, colour mode, trails and
    /// stagnation looping. Painted from the live cloud, never a scene rebuild.
    pub particle_appearance: ParticleAppearance,
    /// Which sampled slice plane-by-plane mode highlights (ticket 23).
    pub plane: PlaneView,
    /// Formula or the editable source scene; exactly one adapter is sampled.
    pub field_mode: FieldMode,
    /// The durable source list and its settings, preserved across mode switches.
    pub sources: SourceScene,
    /// The last source sampling pass for the active adapter.
    pub source_sampling: Option<SourceSampling>,
    /// The durable induction conductor, preserved across mode switches.
    pub induction: crate::induction::InductionScene,
    /// Unit-current geometry cache for the induction sampler.
    pub induction_prepared: Option<Arc<InductionPrepared>>,
    /// The last induction sampling pass at the mirrored clock time.
    pub induction_sampling: Option<InductionSampling>,
    /// The last rejected induction edit, shown by the induction panel/status.
    pub induction_error: Option<String>,
    /// One-line source status/hint owned by the panel and status bar.
    pub source_notice: Option<String>,
    /// Durable bounded analytic material scene, preserved across mode switches.
    pub material: MaterialScene,
    /// The last material sampling pass for the active Materials adapter.
    pub material_sampling: Option<MaterialSampling>,
    /// The last rejected material edit or sampling problem, shown by the
    /// status bar. A failed edit never replaces the last valid scene/sampling.
    pub material_error: Option<String>,
    /// Durable bounded Maxwell teaching scene, preserved across mode switches.
    pub maxwell: MaxwellScene,
    /// The last Maxwell sampling pass for the active 3D/2D view.
    pub maxwell_sampling: Option<MaxwellSampling>,
    /// The last rejected Maxwell edit or sampling/solve problem.
    pub maxwell_error: Option<String>,
    /// Last successful 2D conductor BEM solve, keyed by its BEM scene. Private:
    /// only [`State::refresh_maxwell_sampling`] may populate it.
    maxwell_bem: Option<Arc<BemSolution>>,
    /// A failed BEM solve is not retried until a scene edit or reset.
    maxwell_bem_failed: bool,
    /// Count of BEM solve attempts, exposed to the app test harness.
    pub(crate) maxwell_bem_solves: u64,
    /// Whether the last density request was reduced by a Maxwell cap.
    maxwell_density_capped: bool,
    /// Durable ODE workbench scene, preserved across mode switches.
    pub ode: OdeScene,
    /// Precomputed playback data; replaced only by a committed scene/edit.
    pub ode_solution: Option<Arc<OdeSolution>>,
    /// The last rejected ODE solve, shown by the panel/status bar.
    pub ode_error: Option<String>,
    /// Bumped once per newly solved ODE scene; the app rebuilds lines on it.
    pub ode_epoch: u64,
    /// Chart selection for the ODE panel.
    pub ode_chart: OdeChart,
    /// Variables plotted in the ODE time-series chart; at least one stays on.
    pub ode_variables: [bool; 3],
    /// Pending lazy solve while Ode is active. Private: callers use the scene
    /// setters and [`State::refresh_ode_if_dirty`].
    ode_dirty: bool,
    pub dirty: bool,
    /// Transient simulated time mirrored from the clock. Not persisted.
    pub time: f32,
}

/// Runtime readout for the gradient overlay: what the last refresh drew and
/// what it honestly omitted. The counts are zero until the panel samples.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ScalarAnalysisCounts {
    /// Arrow glyphs painted for usable samples.
    pub arrows: usize,
    /// Samples the overlay omitted because they were unusable or holes.
    pub hidden: usize,
    /// Samples whose stencil fell back to an inward one-sided difference.
    pub one_sided: usize,
}

/// Cache identity for the comparison curves and gradient arrows: every input
/// that changes the geometry, with the clock quantised to the #92 10 Hz rule.
#[derive(Debug, Clone, PartialEq)]
struct ScalarAnalysisKey {
    series: ComparisonSet,
    gradient: Option<GradientSpec>,
    parameters: ParameterSet,
    domain: Domain,
    frame: PlaneFrame,
    curve_samples: usize,
    text_color: [f32; 4],
    quantized_time: i64,
}

impl ScalarAnalysisKey {
    fn new(state: &State) -> Self {
        Self {
            series: state.comparison_series.clone(),
            gradient: state.gradient.clone(),
            parameters: state.parameters.clone(),
            domain: state.domain,
            frame: state.scalar_view_frame(),
            curve_samples: state.plot.settings.curve_samples,
            text_color: state.scalar_text_color,
            quantized_time: scalar_analysis_quantized_time(f64::from(state.time)),
        }
    }
}

/// One-entry, invalidating geometry cache for the scalar analysis overlay.
/// Hidden or unchanged frames reuse it; the overlay never samples.
#[derive(Debug, Clone, Default)]
struct ScalarAnalysisRuntime {
    key: Option<ScalarAnalysisKey>,
    curves: Vec<LineInstanceGPU>,
    gradient: Vec<LineInstanceGPU>,
    counts: ScalarAnalysisCounts,
    error: Option<String>,
    resamples: u64,
}

fn scalar_analysis_quantized_time(time: f64) -> i64 {
    if time.is_finite() {
        (time / SCALAR_RESAMPLE_INTERVAL).floor() as i64
    } else {
        i64::MIN
    }
}

fn series_color(color: crate::color_ramp::Rgb) -> [f32; 4] {
    [
        f32::from(color.0[0]) / 255.0,
        f32::from(color.0[1]) / 255.0,
        f32::from(color.0[2]) / 255.0,
        1.0,
    ]
}

fn fixed_color(color: [u8; 3]) -> [f32; 4] {
    [
        f32::from(color[0]) / 255.0,
        f32::from(color[1]) / 255.0,
        f32::from(color[2]) / 255.0,
        1.0,
    ]
}

fn finite_line(a: Vec3, b: Vec3, width: f32, color: [f32; 4]) -> Option<LineInstanceGPU> {
    (a.is_finite() && b.is_finite() && a != b).then_some(LineInstanceGPU {
        a: [a.x, a.y, a.z, 1.0],
        b: [b.x, b.y, b.z, width],
        color,
    })
}

/// Push one curve edge. Solid series keep it whole; a dashed series carries a
/// phase across edges so the pattern stays continuous along the polyline even
/// where an edge was rejected.
#[allow(clippy::too_many_arguments)]
fn push_curve_edge(
    out: &mut Vec<LineInstanceGPU>,
    a: Vec3,
    b: Vec3,
    width: f32,
    color: [f32; 4],
    dash: f32,
    gap: f32,
    phase: &mut f32,
) {
    if dash <= 0.0 || gap < 0.0 {
        if let Some(line) = finite_line(a, b, width, color) {
            out.push(line);
        }
        return;
    }
    let span = b - a;
    let length = span.length();
    if !length.is_finite() || length <= f32::EPSILON {
        return;
    }
    let direction = span / length;
    let cycle = dash + gap;
    let mut start = 0.0;
    while start < length {
        let position = (*phase + start) % cycle;
        if position < dash {
            let end = (start + (dash - position).min(length - start)).min(length);
            if end - start > f32::EPSILON
                && let Some(line) =
                    finite_line(a + direction * start, a + direction * end, width, color)
            {
                out.push(line);
            }
            start = end;
        } else {
            start += (cycle - position).min(length - start);
        }
    }
    *phase = (*phase + length) % cycle;
}

/// Build one comparison series' polyline on the shared plane frame.
///
/// A parse failure is returned with the series' label instead of silently
/// painting nothing, so the panel can surface the same honest reason as the
/// gradient source.
fn comparison_curve_lines(
    series: &ComparisonSeries,
    frame: &PlaneFrame,
    curve_samples: usize,
    time: f32,
) -> Result<Vec<LineInstanceGPU>, String> {
    // Comparison series are Curve-context display geometry: the same parser
    // and machinery as the existing Curve plot. They never become gradients.
    let expr = ScalarExpr::parse(&series.expr, ScalarContext::Curve).map_err(|error| {
        format!(
            "comparison series `{}` could not be parsed: {error}",
            series.label
        )
    })?;
    let local_domain = frame.local_domain();
    let slots = sample_curve_at(&expr, &local_domain, curve_samples, time);
    let edges = curve_edges_at(&expr, &local_domain, &slots, time);
    let color = series_color(series.style.color);
    let width = crate::reference::EDGE_WIDTH;
    let extent = (frame.half_u.max(frame.half_v) * 2.0).max(f32::EPSILON);
    let dash = if series.style.dashed {
        extent * 0.03
    } else {
        0.0
    };
    let gap = if series.style.dashed {
        extent * 0.02
    } else {
        0.0
    };
    let mut lines = Vec::with_capacity(edges.segments.len());
    let mut phase = 0.0;
    for [a, b] in edges.segments {
        push_curve_edge(
            &mut lines,
            frame.to_world(a),
            frame.to_world(b),
            width,
            color,
            dash,
            gap,
            &mut phase,
        );
    }
    Ok(lines)
}

/// Build the gradient arrow glyphs: one shaft plus two head segments per
/// usable sample, at a fixed length scaled from the sample spacing.
fn gradient_arrow_lines(
    samples: &[GradientSample],
    frame: &PlaneFrame,
    color: [f32; 4],
) -> (Vec<LineInstanceGPU>, ScalarAnalysisCounts) {
    let mut counts = ScalarAnalysisCounts {
        hidden: samples.iter().filter(|sample| !sample.usable).count(),
        one_sided: samples.iter().filter(|sample| sample.one_sided).count(),
        ..ScalarAnalysisCounts::default()
    };
    let side = (samples.len() as f64).sqrt().round() as usize;
    if side < 2 {
        return (Vec::new(), counts);
    }
    let spacing_u = (2.0 * f64::from(frame.half_u) / (side - 1) as f64) as f32;
    let spacing_v = (2.0 * f64::from(frame.half_v) / (side - 1) as f64) as f32;
    let spacing = spacing_u.max(spacing_v).max(f32::EPSILON);
    let length = spacing * 0.8;
    let head = length * 0.35;
    // The head opens 25° from the shaft on both sides.
    let (sin, cos) = 25.0f32.to_radians().sin_cos();
    let width = crate::reference::EDGE_WIDTH;
    let mut lines = Vec::new();
    for sample in samples {
        if !sample.usable {
            continue;
        }
        let Some(gradient) = sample.gradient else {
            continue;
        };
        if !sample.position.is_finite() || !gradient.is_finite() {
            continue;
        }
        let magnitude = gradient.length();
        if !magnitude.is_finite() || magnitude <= f32::EPSILON {
            // A zero gradient has no direction; there is no honest arrow to
            // paint, and it is not a "hidden" sample.
            continue;
        }
        let dir = gradient / magnitude;
        // An in-plane perpendicular, so both head segments stay on the slice.
        let perp = frame.normal.cross(dir).normalize_or_zero();
        if perp == Vec3::ZERO {
            continue;
        }
        let tip = sample.position + dir * length;
        let left = tip - (dir * cos + perp * sin) * head;
        let right = tip - (dir * cos - perp * sin) * head;
        for (a, b) in [(sample.position, tip), (tip, left), (tip, right)] {
            if let Some(line) = finite_line(a, b, width, color) {
                lines.push(line);
            }
        }
        counts.arrows += 1;
    }
    (lines, counts)
}

impl State {
    /// Flag the state as needing a fresh scene, and report that it did.
    ///
    /// Presentation-only edits (style, colours, grid, particles, streamlines)
    /// reach this seam but must not invalidate an Ode `CurrentField` solution:
    /// only RHS changes (formula and domain) call
    /// [`State::mark_ode_current_field_dirty`]. Typed-equation scenes are
    /// independent of field edits and are never invalidated here.
    /// [`State::set_ode_scene`] clears the pending flag after its own
    /// synchronous solve.
    pub fn mark_dirty(self: &mut State) -> bool {
        self.dirty = true;
        true
    }

    /// Atomically replace the named parameters after validating the whole set.
    /// A rejected set leaves the previous parameters, the dirty flag, the
    /// generation counters and every dependent cache untouched; an accepted
    /// set is assigned once and requests exactly one scene rebuild. Committing
    /// a set equal to the committed one is a validated no-op: it leaves the
    /// dirty flag, generation counters and caches untouched.
    ///
    /// An accepted set re-parses the current formula against it. If the formula
    /// no longer binds (a referenced parameter was removed) the previous valid
    /// field is kept and [`parse_error`] records the reason; the new set stays
    /// committed because its own values are valid.
    pub fn commit_parameters(self: &mut State, next: ParameterSet) -> Result<(), String> {
        next.validate()?;
        if next == self.parameters {
            return Ok(());
        }
        self.parameters = next;
        let parsed = FieldExpr::parse_with_parameters(self.formula.as_str(), &self.parameters);
        match parsed {
            Ok(field) => {
                let previous_grid = self.plane_grid();
                self.field = field;
                self.parse_error = String::new();
                self.sync_plane(previous_grid);
                self.mark_ode_current_field_dirty();
            }
            Err(error) => {
                log::debug!("field parse failed after parameter commit: {error}");
                self.parse_error = error.to_string();
            }
        }
        self.mark_dirty();
        Ok(())
    }

    /// The committed named-parameter value, if it exists.
    pub fn parameter_value(self: &State, name: &str) -> Option<f64> {
        self.parameters.value(name)
    }

    /// Atomically replace the viewport style after validating the whole record.
    /// A rejected style leaves the previous record and the dirty flag untouched;
    /// an accepted style is assigned once and requests exactly one scene
    /// rebuild. Committing a style equal to the committed one is a validated
    /// no-op and does not dirty the scene.
    pub fn commit_viewport_style(self: &mut State, next: ViewportStyle) -> Result<(), String> {
        next.validate()?;
        if next == self.viewport_style {
            return Ok(());
        }
        self.viewport_style = next;
        self.mark_dirty();
        Ok(())
    }

    /// Restore the shipped dark-laboratory style. Dirties the scene only when
    /// the committed record actually changed.
    pub fn reset_viewport_style(self: &mut State) {
        if self.viewport_style != ViewportStyle::default() {
            self.viewport_style = ViewportStyle::default();
            self.mark_dirty();
        }
    }

    /// Reset only the colour mapping (issue #86 [`EditScope::Color`]): builtin
    /// viridis, no custom ramp, linear scaling, magnitude quantity, auto-fit on
    /// and the range cleared. The document, domain, parameters, camera and layer
    /// styles are untouched. A real change bumps the plot generation once and
    /// dirties the scene once; an already-default mapping is a no-op.
    ///
    /// [`EditScope::Color`]: crate::edit_history::EditScope::Color
    pub fn reset_color_scope(self: &mut State) {
        let colors_changed = self.colormap != Colormap::Viridis || self.color_ramp.is_some();
        let changed = colors_changed
            || self.color_mode != ScaleMode::Linear
            || self.color_quantity != ColorQuantity::Magnitude
            || !self.auto_fit
            || self.color_min != 0.0
            || self.color_max != 1.0;
        if !changed {
            return;
        }
        self.colormap = Colormap::Viridis;
        self.color_ramp = None;
        self.color_mode = ScaleMode::Linear;
        self.color_quantity = ColorQuantity::Magnitude;
        self.auto_fit = true;
        self.color_min = 0.0;
        self.color_max = 1.0;
        if colors_changed {
            self.plot.generation += 1;
        }
        self.mark_dirty();
    }

    /// Reset the whole editable document to a fresh 2D session (issue #86
    /// [`EditScope::All`]): scene fields, parameters, sources, modes and the
    /// appearance record. Runtime state (time, particles, fly pose, layout) is
    /// not part of `State`; the app owns those and leaves them alone. The state
    /// starts dirty so the next frame rebuilds the default scene.
    ///
    /// [`EditScope::All`]: crate::edit_history::EditScope::All
    pub fn reset_document(self: &mut State) {
        *self = State::new(false);
    }

    /// The committed per-layer and per-family appearance record.
    pub fn layer_styles(self: &State) -> &LayerStyles {
        &self.layer_styles
    }

    /// Atomically replace the appearance overrides after validating the whole
    /// record. A rejected record leaves the previous styles and the dirty flag
    /// untouched; an accepted record requests exactly one scene rebuild.
    /// Committing a record equal to the committed one is a validated no-op and
    /// does not dirty the scene.
    pub fn commit_layer_styles(self: &mut State, next: LayerStyles) -> Result<(), String> {
        next.validate()?;
        if next == self.layer_styles {
            return Ok(());
        }
        self.layer_styles = next;
        self.mark_dirty();
        Ok(())
    }

    /// The committed explicit streamline seed layout (issue #87 slice S1).
    pub fn seed_layout(self: &State) -> &SeedLayout {
        &self.seed_layout
    }

    /// Atomically replace the explicit seed layout after validating it against
    /// the visible scene domain.
    ///
    /// A rejected layout leaves the previous seeds and the dirty flag
    /// untouched; an accepted layout requests exactly one scene rebuild.
    /// Committing a layout equal to the committed one is a validated no-op and
    /// does not dirty the scene. S1 only stores the layout: streamline
    /// generation still uses the automatic grid until slice S2 consumes it.
    pub fn commit_seed_layout(self: &mut State, next: SeedLayout) -> Result<(), String> {
        next.validate(&self.scene_domain())?;
        if next == self.seed_layout {
            return Ok(());
        }
        self.seed_layout = next;
        self.mark_dirty();
        Ok(())
    }

    /// Clear every explicit seed. Dirties the scene only when the layout was
    /// non-empty.
    pub fn reset_seed_layout(self: &mut State) {
        if !self.seed_layout.is_empty() {
            self.seed_layout = SeedLayout::default();
            self.mark_dirty();
        }
    }

    /// Wrap `position` as a seed planted on the plane currently being edited.
    ///
    /// The anchor records the axis, slice index and local `u`/`v`; without a
    /// plane slice the seed keeps `anchor = None`.
    pub fn seed_point_on_current_plane(self: &State, position: Vec3) -> SeedPoint {
        let anchor = self.plane_frame().map(|frame| {
            let plane = self.plane.clamped(self.plane_grid());
            let local = frame.to_local(position);
            SeedAnchor {
                axis: frame.axis,
                index: plane.index,
                u: local.x,
                v: local.y,
            }
        });
        SeedPoint { position, anchor }
    }

    /// Atomically append a generator/placement result to the explicit layout.
    ///
    /// The whole application is checked first: more than
    /// [`MAX_GENERATED_PER_APPLICATION`] seeds, a total above [`MAX_SEEDS`] or
    /// any invalid point rejects the request in full and leaves the committed
    /// layout and dirty flag untouched. A successful application dirties the
    /// scene once.
    pub fn apply_generated_seeds(self: &mut State, positions: &[Vec3]) -> Result<(), SeedError> {
        if positions.len() > MAX_GENERATED_PER_APPLICATION {
            return Err(SeedError::CountTooLarge {
                count: positions.len(),
                max: MAX_GENERATED_PER_APPLICATION,
            });
        }
        if self.seed_layout.len() + positions.len() > MAX_SEEDS {
            return Err(SeedError::LayoutFull {
                requested: positions.len(),
                existing: self.seed_layout.len(),
                max: MAX_SEEDS,
            });
        }
        for position in positions {
            if !position.is_finite() {
                return Err(SeedError::NonFinite("generated seed"));
            }
        }
        let points = positions
            .iter()
            .map(|position| self.seed_point_on_current_plane(*position));
        let next = SeedLayout {
            seeds: self
                .seed_layout
                .seeds
                .iter()
                .copied()
                .chain(points)
                .collect(),
        };
        next.validate(&self.scene_domain())
            .map_err(SeedError::Invalid)?;
        if next == self.seed_layout {
            return Ok(());
        }
        self.seed_layout = next;
        self.mark_dirty();
        Ok(())
    }

    /// Place one explicit seed from a viewport pick, using the current plane
    /// anchor and the same full-application validation as a generator.
    pub fn place_seed(self: &mut State, position: Vec3) -> Result<(), SeedError> {
        self.apply_generated_seeds(std::slice::from_ref(&position))
    }

    /// The Flow layer's resolved opacity and width multipliers (issue #84
    /// A2a), shared by the streamline and particle drawables.
    pub fn flow_layer_style(self: &State) -> FlowLayerStyle {
        self.flow_layer_style_for(self.layer_styles())
    }

    /// [`State::flow_layer_style`] for an explicit style record. The blueprint
    /// SVG path passes [`LayerStyles::default`].
    pub(crate) fn flow_layer_style_for(self: &State, styles: &LayerStyles) -> FlowLayerStyle {
        FlowLayerStyle::from_resolved(&styles.resolve(LayerId::Flow, None)).sanitized()
    }

    /// The committed viewport style after the Reference layer's width and
    /// opacity overrides. Background and RGB are never touched.
    pub fn reference_viewport_style(self: &State) -> ViewportStyle {
        self.reference_viewport_style_for(self.layer_styles())
    }

    /// [`State::reference_viewport_style`] for an explicit style record.
    pub(crate) fn reference_viewport_style_for(
        self: &State,
        styles: &LayerStyles,
    ) -> ViewportStyle {
        scaled_viewport_style(
            &self.viewport_style,
            &styles.resolve(LayerId::Reference, None),
        )
    }

    /// The arrow parameters with the Arrows layer's girth multiplier applied
    /// (issue #84 A2a). Head length and side count never move, and the stored
    /// [`State::arrow_params`] record stays untouched.
    pub fn effective_arrow_params(self: &State) -> ArrowParams {
        self.effective_arrow_params_for(self.layer_styles())
    }

    /// [`State::effective_arrow_params`] for an explicit style record.
    pub(crate) fn effective_arrow_params_for(self: &State, styles: &LayerStyles) -> ArrowParams {
        let params = self.arrow_params;
        let width = sanitized_width_for_style(self.arrow_layer_resolution(styles).width);
        ArrowParams {
            head_width: params.head_width * width,
            shaft_radius: params.shaft_radius * width,
            ..params
        }
    }

    /// The one physics family a teaching scene draws in Materials/Maxwell
    /// mode; `None` for every other mode. Slice B2 resolves the Teaching-scene
    /// layer with this family once per snapshot.
    fn teaching_family(self: &State) -> Option<PhysicsFamily> {
        match self.field_mode {
            FieldMode::Materials => Some(physics_family_of_material(material_family(
                self.material.case,
            ))),
            FieldMode::Maxwell => {
                Some(physics_family_of_maxwell(maxwell_family(self.maxwell.case)))
            }
            _ => None,
        }
    }

    /// The layer style that drives a mode's arrows. Materials and Maxwell use
    /// their single `(TeachingScenes, family)` record; every other mode keeps
    /// the Arrows layer.
    fn arrow_layer_resolution(self: &State, styles: &LayerStyles) -> ResolvedLayerStyle {
        match self.teaching_family() {
            Some(family) => styles.resolve(LayerId::TeachingScene, Some(family)),
            None => styles.resolve(LayerId::Arrows, None),
        }
    }

    /// Queue a lazy re-solve only when the active Ode RHS actually changes.
    /// Formula and domain edits call this; presentation edits never do, so
    /// playback is not restarted for a style change. Invalid formula edits
    /// still queue so the error is surfaced while the retained solution stays.
    fn mark_ode_current_field_dirty(self: &mut State) {
        if self.field_mode == FieldMode::Ode
            && self.ode.source == crate::ode::OdeSource::CurrentField
        {
            self.ode_dirty = true;
        }
    }

    /// Mirror the clock's time into the state. Returns whether `t` changed.
    ///
    /// Vector mode keeps its field-dependent behavior: a time-dependent field
    /// dirties the state. Scalar mode invalidates only a valid, active
    /// time-dependent draft; a static scalar is untouched, and an invalid edit
    /// of a previously time-dependent source freezes its retained mesh.
    pub fn set_time(self: &mut State, t: f32) -> bool {
        if self.time == t {
            return false;
        }
        self.time = t;
        if self.is_vector_mode() {
            match self.field_mode {
                FieldMode::Induction => {
                    if self.induction.is_animated() {
                        self.mark_dirty();
                    }
                }
                FieldMode::Formula => {
                    if self.field.is_time_dependent() {
                        self.mark_dirty();
                    }
                }
                // Ode owns a precomputed solution; playback time is mirrored
                // only and must never re-integrate or dirty the scene.
                FieldMode::Ode => {}
                // Materials, Sources and Maxwell are static: the clock never
                // dirties their scenes.
                FieldMode::Sources | FieldMode::Materials | FieldMode::Maxwell => {}
            }
        } else {
            self.scalar_time_changed();
        }
        true
    }

    /// Whether anything in the current view consumes simulated time: a live
    /// particle cloud or a time-dependent vector field in vector mode, or an
    /// active accepted time-dependent scalar expression.
    pub fn has_time_dependent_consumer(self: &State) -> bool {
        if self.is_vector_mode() {
            match self.field_mode {
                FieldMode::Sources | FieldMode::Materials | FieldMode::Maxwell => false,
                FieldMode::Induction => self.induction.is_animated(),
                // A stored solution plays on the shared clock; a pending lazy
                // solve keeps the clock alive long enough to produce it.
                FieldMode::Ode => self.ode_solution.is_some() || self.ode_dirty,
                FieldMode::Formula => {
                    self.particle_target().is_some() || self.field.is_time_dependent()
                }
            }
        } else {
            self.active_scalar_is_time_dependent()
        }
    }

    /// Sampling runs three-dimensional only when the mode is 3D and the active
    /// field actually carries three components (sources always do).
    pub fn is_3d(self: &State) -> bool {
        self.mode_3d && self.active_dimension() == 3
    }

    /// A fresh state for a 2D or 3D field at the defaults.
    pub fn new(mode_3d: bool) -> State {
        let formula = if mode_3d { FORMULA_3D } else { FORMULA_2D };
        let field = FieldExpr::parse(formula).expect("default formula must parse");
        let density = if mode_3d {
            DEFAULT_DENSITY_3D
        } else {
            DEFAULT_DENSITY_2D
        };
        let grid = if mode_3d && field.dimension == 3 {
            UVec3::splat(density)
        } else {
            UVec3::new(density, density, 1)
        };
        State {
            plot: crate::plot_state::PlotState {
                vector_mode_3d: mode_3d,
                ..Default::default()
            },
            mode_3d,
            formula: formula.to_string(),
            parameters: ParameterSet::default(),
            viewport_style: ViewportStyle::default(),
            layer_styles: LayerStyles::default(),
            field,
            parse_error: String::new(),
            density,
            domain: Domain::default(),
            arrow_scale: 1.0,
            arrow_opacity: MAX_ARROW_OPACITY,
            plane_dim_opacity: DIM_ALPHA,
            arrow_length_mode: ArrowLengthMode::Proportional,
            magnitude_scale: DEFAULT_MAGNITUDE_SCALE,
            proportional_exponent: DEFAULT_PROPORTIONAL_EXPONENT,
            colormap: Colormap::Viridis,
            color_ramp: None,
            color_mode: ScaleMode::Linear,
            color_quantity: ColorQuantity::Magnitude,
            auto_fit: true,
            color_min: 0.0,
            color_max: 1.0,
            arrows: true,
            show_grid: true,
            show_coordinate_system: true,
            arrow_style: ArrowStyle::Cone,
            arrow_style_2d: ArrowStyle2D::Flat,
            arrow_params: ArrowParams::default(),
            streamlines: false,
            seed_mode: StreamlineSeedMode::Auto,
            streamline_count: 0,
            seed_layout: SeedLayout::default(),
            probes: ProbeSet::default(),
            comparison_series: ComparisonSet::default(),
            gradient: None,
            scalar_grid_side: DEFAULT_SCALAR_GRID_SIDE,
            scalar_contour_levels: DEFAULT_SCALAR_CONTOUR_LEVELS,
            show_scalar_heatmap: false,
            scalar_text_color: DEFAULT_SCALAR_TEXT_COLOR,
            scalar_view_cache: ScalarViewCache::default(),
            scalar_preview_levels: None,
            scalar_analysis: ScalarAnalysisRuntime::default(),
            left_click_action: LeftClickAction::SeedStreamline,
            particles: false,
            particle_size: DEFAULT_PARTICLE_SIZE,
            particle_count: 0,
            particle_appearance: ParticleAppearance::default(),
            plane: PlaneView::middle(grid),
            field_mode: FieldMode::Formula,
            sources: SourceScene::default(),
            source_sampling: None,
            induction: crate::induction::InductionScene::default(),
            induction_prepared: None,
            induction_sampling: None,
            induction_error: None,
            source_notice: None,
            material: MaterialScene::default(),
            material_sampling: None,
            material_error: None,
            maxwell: MaxwellScene::default(),
            maxwell_sampling: None,
            maxwell_error: None,
            maxwell_bem: None,
            maxwell_bem_failed: false,
            maxwell_bem_solves: 0,
            maxwell_density_capped: false,
            ode: OdeScene::default(),
            ode_solution: None,
            ode_error: None,
            ode_epoch: 0,
            ode_chart: OdeChart::Phase,
            ode_variables: [true, true, false],
            ode_dirty: false,
            dirty: true,
            time: 0.0,
        }
    }

    /// The formula the field bar holds. Re-parsing it marks the scene dirty
    /// when the field actually changed; a formula that does not parse is
    /// reported in [`parse_error`] and leaves [`field`] at the last one that
    /// parsed, so the viewport keeps showing the previous field.
    pub fn set_formula(self: &mut State, formula: String) -> bool {
        if self.formula == formula {
            return false;
        }
        let parsed = FieldExpr::parse_with_parameters(formula.as_str(), &self.parameters);
        self.formula = formula;
        match parsed {
            Ok(field) => {
                let previous_grid = self.plane_grid();
                self.field = field;
                self.parse_error = String::new();
                self.sync_plane(previous_grid);
                self.mark_ode_current_field_dirty();
                self.mark_dirty()
            }
            Err(error) => {
                // Once per formula change, never per frame: the status bar
                // still shows the message; this is the stderr/console trail.
                log::debug!("field parse failed: {error}");
                self.parse_error = error.to_string();
                self.mark_ode_current_field_dirty();
                self.mark_dirty()
            }
        }
    }

    /// The active field's sampled dimension: sources are always three
    /// dimensional; the formula reports its own component count.
    pub fn active_dimension(self: &State) -> usize {
        match self.field_mode {
            FieldMode::Sources
            | FieldMode::Induction
            | FieldMode::Materials
            | FieldMode::Maxwell => 3,
            // The solved scene owns the dimension; before a lazy solve, the
            // selected scene does. Never hardcode the projection here.
            FieldMode::Ode => self
                .ode_solution
                .as_ref()
                .map(|solution| solution.dimension)
                .unwrap_or(self.ode.dimension),
            FieldMode::Formula => self.field.dimension,
        }
    }

    /// Switch between the formula, source, induction, ODE, materials and
    /// Maxwell adapters. The formula, parsed field, every durable scene and the
    /// current 2D/3D projection are preserved; only each adapter's cached
    /// sampling and notice are cleared. The stored density itself is never
    /// destroyed, only sampled under the active cap.
    pub fn set_field_mode(self: &mut State, mode: FieldMode) -> bool {
        if self.field_mode == mode {
            return false;
        }
        if matches!(
            mode,
            FieldMode::Ode | FieldMode::Materials | FieldMode::Maxwell
        ) && !self.is_vector_mode()
        {
            // Ode and Materials are vector-viewport concepts: a scalar kind
            // would hide their scenes. Restoring the vector kind also restores
            // the projection the vector view last held.
            self.set_plot_kind(crate::plot::PlotKind::VectorField);
        }
        let previous_grid = self.plane_grid();
        let previous_dimension = self.active_dimension();
        let previous_mode_3d = self.mode_3d;
        self.field_mode = mode;
        self.source_sampling = None;
        self.source_notice = None;
        self.induction_sampling = None;
        // A mode switch always invalidates the sampled material frame; the
        // durable scene (and any edit error) stays.
        self.material_sampling = None;
        // Maxwell samples transiently too; its durable scene and any matching
        // BEM solve survive so re-entry does not solve again.
        self.maxwell_sampling = None;
        if mode != FieldMode::Induction {
            // Leaving Induction drops the prepared geometry cache; the durable
            // scene stays so returning re-prepares at the current clock time.
            self.induction_prepared = None;
        }
        let dimension_changed = previous_dimension != self.active_dimension();
        let clamped = self.clamp_source_density();
        if clamped || self.mode_3d != previous_mode_3d || dimension_changed {
            self.sync_plane(previous_grid);
        }
        self.mark_dirty();
        if mode == FieldMode::Ode {
            // Ode has no Induction notices and never solves synchronously. A
            // preserved solution re-enters without a new epoch (playback time
            // stays); a fresh/current-field scene queues the first lazy solve.
            // The last Ode error stays until a successful solve or scene
            // commit clears it, so re-entry cannot hide an actionable failure.
            self.induction_error = None;
            self.ode_dirty = self.ode_solution.is_none()
                || self.ode.source == crate::ode::OdeSource::CurrentField;
        }
        true
    }

    /// Re-clamp the stored density when a mode makes the active sampling
    /// cubic or denser than its cap. This is the one path that can turn a
    /// legal 200² 2D density into 3D sampling, so the 40³ cap must apply here
    /// even though `set_density` already clamps at edit time. Materials uses
    /// the same caps as the general 2D/3D grid; Induction is capped at sampling
    /// time instead, so its stored density survives a mode visit untouched.
    fn clamp_source_density(self: &mut State) -> bool {
        let cap = match self.field_mode {
            FieldMode::Sources if self.is_3d() => Some(MAX_DENSITY_3D),
            FieldMode::Materials if self.is_3d() => Some(MATERIAL_MAX_DENSITY_3D),
            FieldMode::Materials => Some(MATERIAL_MAX_DENSITY_2D),
            FieldMode::Maxwell if self.is_3d() => Some(MAXWELL_MAX_DENSITY_3D),
            FieldMode::Maxwell => Some(MAXWELL_MAX_DENSITY_2D),
            _ => None,
        };
        if self.field_mode == FieldMode::Maxwell {
            self.maxwell_density_capped = false;
        }
        match cap {
            Some(cap) if self.density > cap => {
                self.density = cap;
                if self.field_mode == FieldMode::Maxwell {
                    self.maxwell_density_capped = true;
                }
                true
            }
            _ => false,
        }
    }

    /// Replace the durable induction scene. An invalid edit is rejected before
    /// it can replace anything: the previous valid scene, prepared cache and
    /// arrows all stay, and the actionable error is recorded. A valid edit
    /// clears that error and invalidates the sampled frame.
    pub fn set_induction_scene(self: &mut State, scene: crate::induction::InductionScene) -> bool {
        if let Err(error) = scene.validate() {
            self.induction_error = Some(error);
            return false;
        }
        self.induction_error = None;
        self.induction = scene;
        self.induction_sampling = None;
        self.mark_dirty();
        true
    }

    /// Global reset's induction half: drop the cache and restore the default
    /// valid scene. Formula/Sources durable state and the shared clock are
    /// untouched; the caller selects Formula mode.
    pub fn reset_induction(self: &mut State) -> bool {
        self.induction = crate::induction::InductionScene::default();
        self.induction_prepared = None;
        self.induction_sampling = None;
        self.induction_error = None;
        self.mark_dirty()
    }

    /// Replace the durable ODE scene after validating and solving it. On
    /// success the solution and scene switch together, the error clears and
    /// the epoch bumps so the app rebuilds lines once. On failure the previous
    /// scene/solution stay visible and the actionable error is recorded.
    pub fn set_ode_scene(self: &mut State, scene: OdeScene) -> Result<(), OdeError> {
        match solve_ode(self, &scene) {
            Ok(solution) => {
                self.ode = scene;
                self.ode_solution = Some(Arc::new(solution));
                self.ode_error = None;
                self.ode_epoch = self.ode_epoch.wrapping_add(1);
                self.ode_dirty = false;
                self.dirty = true;
                Ok(())
            }
            Err(error) => {
                self.ode_error = Some(error.to_string());
                Err(error)
            }
        }
    }

    /// Recompute the ODE solution only when Ode is active and a lazy solve is
    /// pending. A broken `CurrentField` draft sets the error, clears the flag
    /// and retains the last valid solution; typed scenes are never invalidated
    /// by unrelated formula or colour edits.
    pub fn refresh_ode_if_dirty(self: &mut State) {
        if self.field_mode != FieldMode::Ode || !self.ode_dirty {
            return;
        }
        self.ode_dirty = false;
        if self.ode.source == crate::ode::OdeSource::CurrentField && !self.parse_error.is_empty() {
            self.ode_error = Some(self.parse_error.clone());
            return;
        }
        match solve_ode(self, &self.ode) {
            Ok(solution) => {
                self.ode_solution = Some(Arc::new(solution));
                self.ode_error = None;
                self.ode_epoch = self.ode_epoch.wrapping_add(1);
                self.dirty = true;
            }
            Err(error) => {
                self.ode_error = Some(error.to_string());
            }
        }
    }

    /// Restore a persisted ODE scene without solving it.
    ///
    /// Persistence is not an ODE interaction: this validates the stored scene
    /// through the same [`prepare_ode`] path a real edit uses, stores the scene,
    /// chart and variables, clears any stale solution/error, marks the lazy
    /// solve pending and returns. The first Ode frame (or
    /// [`State::refresh_ode_if_dirty`]) performs the solve. A malformed scene
    /// falls back to [`OdeScene::default`], records the validation error and
    /// still leaves the default scene pending. Variables that are all false
    /// restore the default selection.
    pub fn restore_ode_fields(
        self: &mut State,
        scene: OdeScene,
        chart: OdeChart,
        variables: [bool; 3],
    ) -> Result<(), String> {
        let variables = if variables.iter().any(|enabled| *enabled) {
            variables
        } else {
            [true, true, false]
        };
        self.ode_chart = chart;
        self.ode_variables = variables;
        self.ode_solution = None;
        match prepare_ode(self, &scene) {
            Ok(_) => {
                self.ode = scene;
                self.ode_error = None;
                self.ode_dirty = true;
                Ok(())
            }
            Err(error) => {
                self.ode = OdeScene::default();
                self.ode_error = Some(error.to_string());
                self.ode_dirty = true;
                Err(error.to_string())
            }
        }
    }

    /// Global reset's ODE half: restore and synchronously solve the default
    /// scene, clear the error, bump the epoch and mark the scene dirty.
    /// Leaving Ode deliberately keeps the last solution; this reset is the
    /// explicit way to replace it. Chart and variable selections return to
    /// their defaults too. The caller owns the shared clock's loop period.
    pub fn reset_ode(self: &mut State) {
        let scene = OdeScene::default();
        match solve_ode(self, &scene) {
            Ok(solution) => {
                self.ode_solution = Some(Arc::new(solution));
                self.ode_error = None;
            }
            Err(error) => {
                self.ode_solution = None;
                self.ode_error = Some(error.to_string());
            }
        }
        self.ode = scene;
        self.ode_chart = OdeChart::Phase;
        self.ode_variables = [true, true, false];
        self.ode_epoch = self.ode_epoch.wrapping_add(1);
        self.ode_dirty = false;
        self.dirty = true;
    }

    /// Select the ODE chart. Returns whether it changed.
    pub fn set_ode_chart(self: &mut State, chart: OdeChart) -> bool {
        if self.ode_chart == chart {
            return false;
        }
        self.ode_chart = chart;
        true
    }

    /// Select the time-series variables. At least one must stay on; an
    /// all-false request is rejected so the chart never becomes meaningless.
    pub fn set_ode_variables(self: &mut State, variables: [bool; 3]) -> bool {
        if !variables.iter().any(|enabled| *enabled) {
            return false;
        }
        if self.ode_variables == variables {
            return false;
        }
        self.ode_variables = variables;
        true
    }

    /// Rebuild the induction cache. A changed geometry/domain/grid/plane key
    /// re-prepares the unit-current cache; a time-only change rescales it. The
    /// prepared `Arc` identity is deliberately preserved on time-only frames.
    pub fn refresh_induction(self: &mut State) {
        if self.field_mode != FieldMode::Induction {
            self.induction_sampling = None;
            return;
        }
        if let Err(error) = self.induction.validate() {
            self.induction_error = Some(error);
            self.induction_sampling = None;
            return;
        }
        let plane = self.plane_frame();
        let key = induction_key(
            &self.induction,
            &self.domain,
            self.grid(),
            plane.as_ref(),
            self.sources.softening,
            Default::default(),
        );
        let stale = self
            .induction_prepared
            .as_ref()
            .is_none_or(|prepared| prepared.key != key);
        if stale {
            match prepare_induction(
                &self.induction,
                &self.domain,
                self.grid(),
                plane.as_ref(),
                self.sources.softening,
                Default::default(),
            ) {
                Ok(prepared) => {
                    self.induction_prepared = Some(Arc::new(prepared));
                    self.induction_error = None;
                }
                Err(error) => {
                    self.induction_prepared = None;
                    self.induction_sampling = None;
                    self.induction_error = Some(error);
                    return;
                }
            }
        }
        let Some(prepared) = self.induction_prepared.as_ref() else {
            return;
        };
        self.induction_sampling = Some(sample_induction(
            prepared,
            &self.induction,
            self.time,
            &self.view_options(),
        ));
    }

    /// Replace the durable source scene and invalidate its cached sampling.
    pub fn set_source_scene(self: &mut State, scene: SourceScene) -> bool {
        if self.sources == scene {
            return false;
        }
        self.sources = scene;
        self.source_sampling = None;
        self.source_notice = None;
        self.mark_dirty()
    }

    /// Rebuild the active adapter's cache: formula mode holds no source
    /// sampling; source mode samples the scene at the current domain, grid and
    /// plane frame.
    pub fn refresh_source_sampling(self: &mut State) {
        if self.field_mode != FieldMode::Sources {
            self.source_sampling = None;
            return;
        }
        let plane = self.plane_frame();
        let sampling = sample_source_scene(
            &self.sources,
            &self.domain,
            self.grid(),
            plane.as_ref(),
            &self.view_options(),
        );
        self.source_sampling = Some(sampling);
    }

    /// The cached source sampling, if the source mode has sampled this scene.
    pub fn source_sampling(self: &State) -> Option<&SourceSampling> {
        self.source_sampling.as_ref()
    }

    /// Global reset's source half: return to the default empty scene. The
    /// formula mode itself is selected by the caller's reset path.
    pub fn reset_sources(self: &mut State) -> bool {
        self.sources.reset();
        self.source_sampling = None;
        self.source_notice = None;
        self.mark_dirty()
    }

    /// Replace the durable material scene. Validation runs first; an invalid
    /// edit leaves the previous scene and its last valid sampling untouched and
    /// records the readable error. A valid edit clears that error and
    /// invalidates the sampled frame.
    pub fn set_material_scene(self: &mut State, scene: MaterialScene) -> Result<(), MaterialError> {
        if let Err(error) = validate_material(&scene) {
            let message = material_error_text(&error);
            self.material_error = Some(message);
            return Err(error);
        }
        if scene == self.material {
            self.material_error = None;
            return Ok(());
        }
        self.material = scene;
        self.material_error = None;
        self.material_sampling = None;
        self.mark_dirty();
        Ok(())
    }

    /// Rebuild the Materials frame at the current domain, density and plane.
    /// A 3D view samples the capped cube; a 2D view samples its existing
    /// [`PlaneFrame`] slice. Validation failures and sampling warnings are
    /// carried by the returned sampling; the status bar reads both.
    pub fn refresh_material_sampling(self: &mut State) {
        if self.field_mode != FieldMode::Materials {
            self.material_sampling = None;
            return;
        }
        if let Err(error) = validate_material(&self.material) {
            self.material_error = Some(material_error_text(&error));
            self.material_sampling = None;
            return;
        }
        let plane = self.plane_frame();
        let n = self.capped_density();
        let grid = if plane.is_some() {
            UVec3::new(n, n, 1)
        } else {
            UVec3::splat(n)
        };
        self.material_sampling = Some(sample_materials(
            &self.material,
            &self.domain,
            grid,
            plane.as_ref(),
        ));
    }

    /// Global reset's Materials half: restore the default bounded scene and
    /// drop the sampled frame and any edit error. The caller selects Formula.
    pub fn reset_material(self: &mut State) -> bool {
        self.material = MaterialScene::default();
        self.material_sampling = None;
        self.material_error = None;
        self.mark_dirty()
    }

    /// Replace the durable Maxwell scene. Validation runs first; an invalid
    /// edit leaves the previous scene, sampling and BEM cache untouched and
    /// records the readable error. A valid changed scene invalidates the
    /// sampling and BEM cache and marks the frame dirty.
    pub fn set_maxwell_scene(self: &mut State, scene: MaxwellScene) -> bool {
        if let Err(error) = validate_maxwell(&scene) {
            self.maxwell_error = Some(maxwell_error_text(&error));
            return false;
        }
        if scene == self.maxwell {
            self.maxwell_error = None;
            return false;
        }
        self.maxwell = scene;
        self.maxwell_sampling = None;
        self.maxwell_bem = None;
        self.maxwell_bem_failed = false;
        self.maxwell_error = None;
        self.mark_dirty()
    }

    /// Rebuild the Maxwell frame at the current domain, capped density and
    /// plane. Only this method may solve the 2D conductor BEM, and only when no
    /// matching cached solution exists; plane, sampling and camera changes
    /// reuse it. A failed solve is not retried until a scene edit/reset.
    pub fn refresh_maxwell_sampling(self: &mut State) {
        if self.field_mode != FieldMode::Maxwell {
            self.maxwell_sampling = None;
            return;
        }
        if let Err(error) = validate_maxwell(&self.maxwell) {
            self.maxwell_error = Some(maxwell_error_text(&error));
            self.maxwell_sampling = None;
            return;
        }
        // Defensive cap for a direct field write as well as the setters: the
        // stored grid never exceeds the Maxwell view limit, and the reduction
        // is reported rather than silent.
        let density_cap = if self.is_3d() {
            MAXWELL_MAX_DENSITY_3D
        } else {
            MAXWELL_MAX_DENSITY_2D
        };
        if self.density > density_cap {
            self.density = density_cap;
            self.maxwell_density_capped = true;
            self.plane = self.plane.clamped(self.plane_grid());
        }
        let plane = self.plane_frame();
        let needs_bem =
            self.maxwell.case == crate::maxwell::MaxwellCase::Conductor2D && plane.is_some();
        if needs_bem {
            let matching = self
                .maxwell_bem
                .as_ref()
                .is_some_and(|solution| solution.matches_scene(&self.maxwell));
            if !matching {
                self.maxwell_bem = None;
                if !self.maxwell_bem_failed {
                    self.maxwell_bem_solves = self.maxwell_bem_solves.wrapping_add(1);
                    match solve_bem(&self.maxwell) {
                        Ok(solution) => {
                            self.maxwell_bem = Some(Arc::new(solution));
                            self.maxwell_bem_failed = false;
                            self.maxwell_error = None;
                        }
                        Err(error) => {
                            self.maxwell_bem = None;
                            self.maxwell_bem_failed = true;
                            self.maxwell_error = Some(maxwell_error_text(&error));
                        }
                    }
                }
            }
        }
        let bem = self.maxwell_bem.as_deref();
        let mut sampling = sample_maxwell_with_bem(
            &self.maxwell,
            &self.domain,
            self.grid(),
            plane.as_ref(),
            bem,
        );
        if needs_bem
            && bem.is_none()
            && let Some(error) = self.maxwell_error.as_ref()
        {
            // A failed solve is an unavailable field, not the generic
            // missing-solution hint; surface the actionable solver error.
            sampling.accuracy = crate::maxwell::MaxwellStop::Unavailable;
            sampling.warning = Some(error.clone());
        }
        if self.maxwell_density_capped {
            let warning = sampling.warning.take();
            sampling.warning = Some(match warning {
                Some(warning) => format!("{warning}; {MAXWELL_DENSITY_CAP_WARNING}"),
                None => MAXWELL_DENSITY_CAP_WARNING.to_string(),
            });
        }
        self.maxwell_sampling = Some(sampling);
    }

    /// Lazy restore seam used by [`crate::persistence::ViewRecord`]: store an
    /// already-validated durable scene, drop any sampling/BEM cache, clear the
    /// retained error and mark the first frame pending. It never samples and
    /// never solves the 2D BEM system.
    pub(crate) fn restore_maxwell_scene(self: &mut State, scene: MaxwellScene) {
        self.maxwell = scene;
        self.maxwell_sampling = None;
        self.maxwell_bem = None;
        self.maxwell_bem_failed = false;
        self.maxwell_error = None;
        self.maxwell_density_capped = false;
        self.mark_dirty();
    }

    /// Global reset's Maxwell half: restore the default scene and drop the
    /// sampling, BEM cache and any error. The caller selects Formula.
    pub fn reset_maxwell(self: &mut State) {
        self.maxwell = MaxwellScene::default();
        self.maxwell_sampling = None;
        self.maxwell_bem = None;
        self.maxwell_bem_failed = false;
        self.maxwell_error = None;
        self.maxwell_density_capped = false;
        self.mark_dirty();
    }

    /// The durable Maxwell scene.
    pub fn maxwell_scene(self: &State) -> &MaxwellScene {
        &self.maxwell
    }

    /// The last Maxwell sampling pass, if the active view has sampled.
    pub fn maxwell_sampling(self: &State) -> Option<&MaxwellSampling> {
        self.maxwell_sampling.as_ref()
    }

    /// The projection: an orthographic front view for 2D, the orbiting
    /// perspective camera for 3D.
    pub fn set_mode(self: &mut State, mode_3d: bool) -> bool {
        if !self.is_vector_mode() {
            return self.set_plot_kind(if mode_3d {
                crate::plot::PlotKind::Surface3D
            } else {
                crate::plot::PlotKind::Curve2D
            });
        }
        if self.mode_3d == mode_3d {
            return false;
        }
        let previous_grid = self.plane_grid();
        self.mode_3d = mode_3d;
        self.plot.vector_mode_3d = mode_3d;
        // Direction colouring is 2D-only; fold the reset into this same mode
        // change instead of leaving an unusable quantity for the 3D view.
        if mode_3d && self.color_quantity == ColorQuantity::Direction2d {
            self.color_quantity = ColorQuantity::Magnitude;
        }
        self.clamp_source_density();
        self.sync_plane(previous_grid);
        self.mark_dirty()
    }

    /// Samples per axis, clamped to what this mode can afford.
    pub fn set_density(self: &mut State, density: u32) -> bool {
        let max = match self.field_mode {
            FieldMode::Induction if self.is_3d() => MAX_INDUCTION_DENSITY_3D,
            FieldMode::Induction => MAX_INDUCTION_DENSITY_2D,
            FieldMode::Materials if self.is_3d() => MATERIAL_MAX_DENSITY_3D,
            FieldMode::Materials => MATERIAL_MAX_DENSITY_2D,
            FieldMode::Maxwell if self.is_3d() => MAXWELL_MAX_DENSITY_3D,
            FieldMode::Maxwell => MAXWELL_MAX_DENSITY_2D,
            _ if self.is_3d() => MAX_DENSITY_3D,
            _ => MAX_DENSITY_2D,
        };
        if self.field_mode == FieldMode::Maxwell {
            self.maxwell_density_capped = density > max;
        }
        let density = density.clamp(MIN_DENSITY, max);
        if self.density == density {
            return false;
        }
        let previous_grid = self.plane_grid();
        self.density = density;
        self.sync_plane(previous_grid);
        self.mark_dirty()
    }

    /// The domain to explore, with the corners normalised so that min is
    /// actually the minimum: dragging the sliders past each other swaps them
    /// rather than producing an empty domain.
    pub fn set_domain(self: &mut State, min: Vec3, max: Vec3) -> bool {
        let domain = domain_between(min, max);
        if self.domain == domain {
            return false;
        }
        let previous_grid = self.plane_grid();
        if self.domain.min.x != domain.min.x
            || self.domain.max.x != domain.max.x
            || self.domain.min.y != domain.min.y
            || self.domain.max.y != domain.max.y
        {
            self.plot.curve.samples = None;
            self.plot.curve.sampled_at = None;
            self.plot.surface.samples = None;
            self.plot.surface.sampled_at = None;
        }
        self.plot.generation += 1;
        self.domain = domain;
        self.sync_plane(previous_grid);
        self.mark_ode_current_field_dirty();
        self.mark_dirty()
    }

    /// How long to draw arrows, as a multiple of the grid spacing.
    pub fn set_arrow_scale(self: &mut State, scale: f32) -> bool {
        let scale = scale.clamp(MIN_ARROW_SCALE, MAX_ARROW_SCALE);
        if self.arrow_scale == scale {
            return false;
        }
        self.arrow_scale = scale;
        self.mark_dirty()
    }

    /// The alpha every arrow is painted with (ticket 36). Clamped into
    /// [`MIN_ARROW_OPACITY`]..=[`MAX_ARROW_OPACITY`]; a non-finite value falls
    /// back to fully opaque rather than poisoning the scene.
    pub fn set_arrow_opacity(self: &mut State, opacity: f32) -> bool {
        let opacity = if opacity.is_finite() {
            opacity.clamp(MIN_ARROW_OPACITY, MAX_ARROW_OPACITY)
        } else {
            MAX_ARROW_OPACITY
        };
        if self.arrow_opacity == opacity {
            return false;
        }
        self.arrow_opacity = opacity;
        self.mark_dirty()
    }

    /// How strongly plane-by-plane mode fades the arrows outside the selected
    /// slice (ticket 36). Clamped into
    /// [`MIN_PLANE_DIM_OPACITY`]..=[`MAX_PLANE_DIM_OPACITY`]; a non-finite
    /// value falls back to [`DIM_ALPHA`]. The plane mode itself decides whether
    /// the value has any effect.
    pub fn set_plane_dim_opacity(self: &mut State, opacity: f32) -> bool {
        let opacity = if opacity.is_finite() {
            opacity.clamp(MIN_PLANE_DIM_OPACITY, MAX_PLANE_DIM_OPACITY)
        } else {
            DIM_ALPHA
        };
        if self.plane_dim_opacity == opacity {
            return false;
        }
        self.plane_dim_opacity = opacity;
        self.mark_dirty()
    }

    /// Choose how magnitude becomes arrow length.
    pub fn set_arrow_length_mode(self: &mut State, mode: ArrowLengthMode) -> bool {
        if self.arrow_length_mode == mode {
            return false;
        }
        self.arrow_length_mode = mode;
        self.mark_dirty()
    }

    /// World length per unit of field magnitude, used only by
    /// [`ArrowLengthMode::Magnitude`]. The value is clamped into
    /// [`MIN_MAGNITUDE_SCALE`]..=[`MAX_MAGNITUDE_SCALE`] and a NaN is rejected
    /// (the previous scale stays), but the resulting arrow length is never
    /// clamped: the user chose the scale and the camera can still fit the
    /// domain around an extreme arrow.
    pub fn set_magnitude_scale(self: &mut State, scale: f32) -> bool {
        if scale.is_nan() {
            return false;
        }
        let scale = scale.clamp(MIN_MAGNITUDE_SCALE, MAX_MAGNITUDE_SCALE);
        if self.magnitude_scale == scale {
            return false;
        }
        self.magnitude_scale = scale;
        self.mark_dirty()
    }

    /// How strongly arrow length follows magnitude in
    /// [`ArrowLengthMode::Proportional`] (ticket 57): the power applied to the
    /// normalised magnitude. Clamped into
    /// [`MIN_PROPORTIONAL_EXPONENT`]..=[`MAX_PROPORTIONAL_EXPONENT`]; a
    /// non-finite value resets to the linear default instead of poisoning the
    /// scene.
    pub fn set_proportional_exponent(self: &mut State, exponent: f32) -> bool {
        let exponent = if exponent.is_finite() {
            exponent.clamp(MIN_PROPORTIONAL_EXPONENT, MAX_PROPORTIONAL_EXPONENT)
        } else {
            DEFAULT_PROPORTIONAL_EXPONENT
        };
        if self.proportional_exponent == exponent {
            return false;
        }
        self.proportional_exponent = exponent;
        self.mark_dirty()
    }

    /// The effective colour mapping (issue #83).
    ///
    /// A custom ramp overrides the builtin selection. The manual range is only
    /// present while auto-fit is off.
    pub fn color_mapping(self: &State) -> ColorMapping {
        ColorMapping {
            palette: match &self.color_ramp {
                Some(ramp) => ColorPalette::Custom(ramp.clone()),
                None => ColorPalette::Builtin(self.colormap),
            },
            mode: self.color_mode,
            quantity: self.color_quantity,
            manual_range: if self.auto_fit {
                None
            } else {
                Some((self.color_min, self.color_max))
            },
        }
    }

    /// Replaces the custom ramp override. `None` returns to the builtin
    /// colormap. An invalid ramp is rejected without touching the state.
    pub fn set_color_ramp(self: &mut State, ramp: Option<ColorRamp>) -> bool {
        if ramp.as_ref().is_some_and(|ramp| ramp.validate().is_err()) {
            return false;
        }
        if self.color_ramp == ramp {
            return false;
        }
        self.color_ramp = ramp;
        self.plot.generation += 1;
        self.mark_dirty()
    }

    /// Selects the value-to-ramp scaling. An invalid mode is rejected.
    pub fn set_color_mode(self: &mut State, mode: ScaleMode) -> bool {
        if mode.validate().is_err() {
            return false;
        }
        if self.color_mode == mode {
            return false;
        }
        self.color_mode = mode;
        self.mark_dirty()
    }

    /// Selects what the colour represents. Unavailable quantities are rejected.
    pub fn set_color_quantity(self: &mut State, quantity: ColorQuantity) -> bool {
        if quantity.available(self.is_3d()).is_err() {
            return false;
        }
        if self.color_quantity == quantity {
            return false;
        }
        self.color_quantity = quantity;
        self.mark_dirty()
    }

    /// Commits the current effective range into the manual range and turns
    /// auto-fit off (issue #83). Returns whether anything changed.
    pub fn freeze_color_range(self: &mut State, stats: &ColorStats) -> bool {
        if self.color_quantity == ColorQuantity::Direction2d {
            return false;
        }
        let (min, max) = self.color_mapping().effective_range(stats);
        if !min.is_finite() || !max.is_finite() || min >= max {
            return false;
        }
        if !self.auto_fit && self.color_min == min && self.color_max == max {
            return false;
        }
        self.color_min = min;
        self.color_max = max;
        self.auto_fit = false;
        self.mark_dirty()
    }

    /// The palette that maps magnitude to colour.
    pub fn set_colormap(self: &mut State, colormap: Colormap) -> bool {
        let cleared_override = self.color_ramp.take().is_some();
        if self.colormap == colormap && !cleared_override {
            return false;
        }
        self.colormap = colormap;
        self.plot.generation += 1;
        self.mark_dirty()
    }

    /// Whether to colour by the field's own magnitude range, or by one the
    /// user set.
    pub fn set_auto_fit(self: &mut State, auto_fit: bool) -> bool {
        if self.auto_fit == auto_fit {
            return false;
        }
        self.auto_fit = auto_fit;
        self.mark_dirty()
    }

    /// A magnitude range the user set, used when [`auto_fit`] is off. The
    /// bounds are swapped if given the wrong way round.
    pub fn set_color_range(self: &mut State, min: f32, max: f32) -> bool {
        let lo = min.min(max);
        let hi = min.max(max);
        if self.color_min == lo && self.color_max == hi {
            return false;
        }
        self.color_min = lo;
        self.color_max = hi;
        self.mark_dirty()
    }

    /// The field's arrows.
    pub fn set_arrows(self: &mut State, arrows: bool) -> bool {
        if self.arrows == arrows {
            return false;
        }
        self.arrows = arrows;
        self.mark_dirty()
    }

    /// Visibility of the grid and domain box in either view.
    pub fn set_show_grid(self: &mut State, show: bool) -> bool {
        if self.show_grid == show {
            return false;
        }
        self.show_grid = show;
        self.plot.generation += 1;
        self.mark_dirty()
    }

    /// Visibility of the coloured world axes in either view.
    pub fn set_show_coordinate_system(self: &mut State, show: bool) -> bool {
        if self.show_coordinate_system == show {
            return false;
        }
        self.show_coordinate_system = show;
        self.plot.generation += 1;
        self.mark_dirty()
    }

    /// Choose the 3D arrow shape family. The 2D family is independent.
    pub fn set_arrow_style(self: &mut State, style: ArrowStyle) -> bool {
        if self.arrow_style == style {
            return false;
        }
        self.arrow_style = style;
        self.mark_dirty()
    }

    /// Choose the 2D arrow family (ticket 61). The 3D family is independent.
    pub fn set_arrow_style_2d(self: &mut State, style: ArrowStyle2D) -> bool {
        if self.arrow_style_2d == style {
            return false;
        }
        self.arrow_style_2d = style;
        self.mark_dirty()
    }

    /// Set the head/shaft proportions, sanitized through the one clamp the
    /// persistence restore and the sidebar share.
    pub fn set_arrow_params(self: &mut State, params: ArrowParams) -> bool {
        let params = params.sanitized();
        if self.arrow_params == params {
            return false;
        }
        self.arrow_params = params;
        self.mark_dirty()
    }

    /// Stream tracing, not yet drawn.
    pub fn set_streamlines(self: &mut State, on: bool) -> bool {
        if self.streamlines == on {
            return false;
        }
        self.streamlines = on;
        self.mark_dirty()
    }

    /// Choose which seeds the on state paints (ticket 50). The mode only swaps
    /// what is drawn or stored; the Feature toggle still hides both.
    pub fn set_seed_mode(self: &mut State, mode: StreamlineSeedMode) -> bool {
        if self.seed_mode == mode {
            return false;
        }
        self.seed_mode = mode;
        self.mark_dirty()
    }

    /// The explicit streamline seed count, clamped to
    /// [`MAX_STREAMLINES`]. `0` means Auto: the historical density-derived
    /// lattice. Any other value seeds exactly that many lines from a lattice
    /// that does not follow the sampling density, so the scene dirty flag is
    /// the only rebuild trigger.
    pub fn set_streamline_count(self: &mut State, count: u32) -> bool {
        let count = count.min(MAX_STREAMLINES);
        if self.streamline_count == count {
            return false;
        }
        self.streamline_count = count;
        self.mark_dirty()
    }

    /// Choose what an explicit left click does (ticket 60). The keyboard
    /// shortcuts are independent of this setting.
    pub fn set_left_click_action(self: &mut State, action: LeftClickAction) -> bool {
        if self.left_click_action == action {
            return false;
        }
        self.left_click_action = action;
        self.mark_dirty()
    }

    /// Whether a viewport click may add to the app's click-seed list. The list
    /// itself is app-owned, but the gate belongs to the same Feature toggle
    /// that hides the click lines: off hides both and refuses new seeds, so
    /// re-enabling reproduces the previous clicks. Both modes accept clicks.
    pub fn accepts_click_seed(self: &State) -> bool {
        !matches!(
            self.field_mode,
            FieldMode::Ode | FieldMode::Materials | FieldMode::Maxwell
        ) && self.streamlines
    }

    /// Particle tracing, not yet drawn.
    pub fn set_particles(self: &mut State, on: bool) -> bool {
        if self.particles == on {
            return false;
        }
        self.particles = on;
        self.mark_dirty()
    }

    /// The particle dot size, clamped into
    /// [`MIN_PARTICLE_SIZE`]..=[`MAX_PARTICLE_SIZE`]. A non-finite size resets
    /// to the default instead of poisoning the dot shader. Dots are rebuilt
    /// from the live cloud every frame, so this never dirties the scene.
    pub fn set_particle_size(self: &mut State, size: f32) -> bool {
        let size = if size.is_finite() {
            size.clamp(MIN_PARTICLE_SIZE, MAX_PARTICLE_SIZE)
        } else {
            DEFAULT_PARTICLE_SIZE
        };
        if self.particle_size == size {
            return false;
        }
        self.particle_size = size;
        true
    }

    /// The explicit particle count, capped at
    /// [`MAX_PARTICLES`](crate::particles::MAX_PARTICLES). `0` means Auto
    /// (density x 16). Like the size, this paints from the live cloud and
    /// never dirties the scene.
    pub fn set_particle_count(self: &mut State, count: u32) -> bool {
        let count = count.min(MAX_PARTICLES as u32);
        if self.particle_count == count {
            return false;
        }
        self.particle_count = count;
        true
    }

    /// The particle appearance knobs (ticket 48), sanitized through the one
    /// clamp the sidebar and persistence share. Like size and count, these
    /// paint from the live cloud and never dirty the scene.
    pub fn set_particle_appearance(self: &mut State, appearance: ParticleAppearance) -> bool {
        let appearance = appearance.sanitized();
        if self.particle_appearance == appearance {
            return false;
        }
        self.particle_appearance = appearance;
        true
    }

    /// This frame's particle target: `Some(count)` while particles are on for
    /// a three-component field — in the 3D view or as a 2D plane slice — and
    /// `None` otherwise. Ordinary two-component fields have no depth or slice
    /// flow for a dot to ride, so the off state still spawns nothing.
    pub fn particle_target(self: &State) -> Option<usize> {
        if matches!(
            self.field_mode,
            FieldMode::Sources
                | FieldMode::Induction
                | FieldMode::Materials
                | FieldMode::Ode
                | FieldMode::Maxwell
        ) {
            return None;
        }
        (self.particles && self.active_dimension() == 3)
            .then(|| effective_particle_count(self.density, self.particle_count) as usize)
    }

    /// The plane-by-plane selection, clamped into the current grid. The index
    /// is a user choice once the grid has more than one slice, so toggling the
    /// mode keeps it; only a grid change recentres it (see [`sync_plane`]).
    pub fn set_plane(self: &mut State, plane: PlaneView) -> bool {
        let mut plane = plane.clamped(self.plane_grid());
        // A 2D slice of a three-component field shares the plane selection
        // with 3D, but the dim toggle itself is a 3D-only control: 2D may not
        // change it.
        if self.is_plane_slice() {
            plane.enabled = self.plane.enabled;
        }
        if self.plane == plane {
            return false;
        }
        self.plane = plane;
        self.mark_dirty()
    }

    /// Toggle plane-by-plane inspection.
    pub fn set_plane_enabled(self: &mut State, enabled: bool) -> bool {
        let plane = PlaneView {
            enabled,
            ..self.plane
        };
        self.set_plane(plane)
    }

    /// Choose the axis the selected plane is perpendicular to. The index is
    /// clamped into the new axis (a one-slice axis collapses to 0).
    pub fn set_plane_axis(self: &mut State, axis: PlaneAxis) -> bool {
        let plane = PlaneView { axis, ..self.plane };
        self.set_plane(plane)
    }

    /// Step the selected plane along its axis; out-of-range values clamp.
    pub fn set_plane_index(self: &mut State, index: u32) -> bool {
        let plane = PlaneView {
            index,
            ..self.plane
        };
        self.set_plane(plane)
    }

    /// Whether plane-by-plane dimming applies: 3D sampling, the mode is on,
    /// and the chosen axis really has more than one slice. 2D mode and a flat
    /// field in 3D mode are therefore untouched.
    pub fn plane_active(self: &State) -> bool {
        self.is_3d() && self.plane.active(self.grid())
    }

    /// Keep the plane index valid after the sampling grid changes shape:
    /// collapse a one-slice axis to 0, recentre a plane that no longer exists,
    /// and — leaving a one-slice axis — start at the middle plane, because
    /// index 0 on a single slice was never a choice. Called by every setter
    /// that changes the grid (density, mode, formula dimension, domain) with
    /// the plane grid that was in force before the change.
    fn sync_plane(self: &mut State, previous_grid: UVec3) {
        let grid = self.plane_grid();
        let count = self.plane.axis.count(grid);
        let previous_count = self.plane.axis.count(previous_grid);
        if count <= 1 {
            self.plane.index = 0;
        } else if previous_count <= 1 || self.plane.index >= count {
            self.plane.index = self.plane.axis.middle_index(grid);
        }
    }

    /// The half-height the 2D orthographic window covers: the visible scene
    /// domain's larger in-plane extent, padded so the field fills the window.
    pub fn plane_size(self: &State) -> f32 {
        let size = self.scene_domain().domain_size();
        size.x.max(size.y) * PLANE_MARGIN
    }

    /// The grid the plane selection indexes into. A three-component field has
    /// a real volume in both view modes, so its plane grid stays `n`×`n`×`n`
    /// even while 2D renders one `n`×`n` layer; a two-component field keeps
    /// the flat `n`×`n`×`1` grid it has always had.
    pub fn plane_grid(self: &State) -> UVec3 {
        let n = self.capped_density();
        if self.active_dimension() == 3 {
            UVec3::new(n, n, n)
        } else {
            UVec3::new(n, n, 1)
        }
    }

    /// Whether 2D is showing a slice of a three-component field.
    pub fn is_plane_slice(self: &State) -> bool {
        self.field_mode != FieldMode::Ode && self.active_dimension() == 3 && !self.mode_3d
    }

    /// The local frame of the selected slice, when 2D is showing one. The
    /// selection is re-clamped against the plane grid so a stale index written
    /// before the grid shrank can never build a frame past the last sample.
    /// Ode has no field slice and always returns `None`.
    pub fn plane_frame(self: &State) -> Option<PlaneFrame> {
        if self.field_mode == FieldMode::Ode {
            return None;
        }
        self.is_plane_slice().then(|| {
            let grid = self.plane_grid();
            PlaneFrame::new(&self.domain, self.plane.clamped(grid), grid)
        })
    }

    /// The plane frame a scalar view samples. A visible plane slice uses its
    /// own axes; every other view uses the XY plane through the domain centre.
    pub fn scalar_view_frame(self: &State) -> PlaneFrame {
        self.plane_frame().unwrap_or_else(|| {
            let grid = self.plane_grid();
            PlaneFrame::new(&self.domain, PlaneView::middle(grid), grid)
        })
    }

    /// Whether the scalar overlay applies to the current scene: a formula
    /// vector view whose surface expression the bounded sampler can evaluate.
    /// This is the single gate for both the Scalar section's unsupported reason
    /// and overlay composition, so the controls and painted geometry cannot
    /// drift apart (issue #92 slice B1).
    pub fn scalar_view_applicable(self: &State) -> bool {
        self.is_vector_mode()
            && self.field_mode == FieldMode::Formula
            && scalar_view_expr(&self.plot.surface.source, &self.parameters).is_ok()
    }

    /// The cache identity of the current scalar view without evaluating the
    /// bounded contour preview. The collapsed panel compares it with the
    /// retained record so geometry from a formula, plane, domain or clock that
    /// no longer applies is not composited.
    fn scalar_view_identity(self: &State) -> ScalarViewIdentity {
        self.scalar_view_identity_for(self.scalar_view_frame(), self.scalar_level_count())
    }

    /// [`Self::scalar_view_identity`] for an explicit frame and level count.
    fn scalar_view_identity_for(
        self: &State,
        frame: PlaneFrame,
        level_count: usize,
    ) -> ScalarViewIdentity {
        ScalarViewIdentity::new(
            &self.plot.surface.source,
            &self.parameters,
            frame,
            frame.local_domain(),
            f64::from(self.time),
            self.scalar_grid_side.clamp(MIN_GRID_SIDE, MAX_GRID_SIDE),
            level_count,
        )
    }

    /// The clamped explicit contour-level count.
    fn scalar_level_count(self: &State) -> usize {
        self.scalar_contour_levels.min(MAX_CONTOUR_LEVELS as u32) as usize
    }

    /// The request the scalar panel builds: the surface formula, committed
    /// parameters, current plane frame, domain and simulated time.
    ///
    /// The grid side follows the Scalar view control (bounded by the model) and
    /// the explicit contour list is generated evenly inside the finite value
    /// range of a bounded preview sample. `0` levels hides contours. An
    /// unsupported formula yields no levels; the cache stores the reason.
    pub fn scalar_view_request(self: &mut State) -> ScalarViewRequest {
        let frame = self.scalar_view_frame();
        let levels = self.scalar_preview_levels(&frame);
        ScalarViewRequest {
            source: self.plot.surface.source.clone(),
            parameters: self.parameters.clone(),
            frame,
            domain: frame.local_domain(),
            time: f64::from(self.time),
            grid_side: self.scalar_grid_side.clamp(MIN_GRID_SIDE, MAX_GRID_SIDE),
            levels,
        }
    }

    /// Explicit, strictly increasing contour levels inside the sampled value
    /// range. A tiny bounded preview (at most 32²) supplies the range without a
    /// second full-grid rebuild. A constant or unsupported field has none.
    ///
    /// The result is memoised under the same non-previewed identity the cache
    /// uses, so a visible but idle section does not re-evaluate the preview.
    fn scalar_preview_levels(self: &mut State, frame: &PlaneFrame) -> Vec<f32> {
        let count = self.scalar_level_count();
        if count == 0 {
            return Vec::new();
        }
        let identity = self.scalar_view_identity_for(*frame, count);
        if let Some((cached, levels)) = &self.scalar_preview_levels
            && cached == &identity
        {
            return levels.clone();
        }
        let levels = self.compute_scalar_preview_levels(frame, count);
        self.scalar_preview_levels = Some((identity, levels.clone()));
        levels
    }

    /// Evaluate the bounded preview and build the levels from its finite range.
    fn compute_scalar_preview_levels(self: &State, frame: &PlaneFrame, count: usize) -> Vec<f32> {
        let Ok(expr) = scalar_view_expr(&self.plot.surface.source, &self.parameters) else {
            return Vec::new();
        };
        let side = self
            .scalar_grid_side
            .clamp(MIN_GRID_SIDE, MAX_GRID_SIDE)
            .min(32);
        // The preview is part of the cache key through `levels`, so evaluate it
        // on the same 100 ms-quantised clock the key uses; otherwise an
        // animated formula would change the levels every frame and defeat the
        // throttle.
        let time = f64::from(self.time);
        let preview_time = if time.is_finite() {
            (time / SCALAR_RESAMPLE_INTERVAL).floor() * SCALAR_RESAMPLE_INTERVAL
        } else {
            0.0
        };
        let Ok(grid) = sample_grid(&expr, *frame, side, side, preview_time) else {
            return Vec::new();
        };
        let mut min = f32::INFINITY;
        let mut max = f32::NEG_INFINITY;
        for sample in &grid.samples {
            if let Some(value) = sample.value {
                min = min.min(value);
                max = max.max(value);
            }
        }
        if !min.is_finite() || !max.is_finite() {
            return Vec::new();
        }
        if min >= max {
            return Vec::new();
        }
        (1..=count)
            .map(|index| min + (max - min) * index as f32 / (count + 1) as f32)
            .collect()
    }

    /// The Scalar view section's grid side (issue #92 slice B1).
    pub fn scalar_grid_side(self: &State) -> u32 {
        self.scalar_grid_side
    }

    /// The Scalar view section's explicit contour-level count.
    pub fn scalar_contour_levels(self: &State) -> u32 {
        self.scalar_contour_levels
    }

    /// Whether the Scalar view paints the filled heatmap (issue #92 slice B2).
    pub fn show_scalar_heatmap(self: &State) -> bool {
        self.show_scalar_heatmap
    }

    /// Commit the heatmap toggle. A real change dirties the scene so the next
    /// snapshot adds or drops the GPU pass; assigning the same value is free.
    pub fn set_show_scalar_heatmap(self: &mut State, show: bool) -> bool {
        if self.show_scalar_heatmap == show {
            return false;
        }
        self.show_scalar_heatmap = show;
        self.mark_dirty();
        true
    }

    /// Startup restore seam: assign the persisted toggle without adding a
    /// dirty request beyond the enclosing restore's documented one.
    pub(crate) fn restore_show_scalar_heatmap(self: &mut State, show: bool) {
        self.show_scalar_heatmap = show;
    }

    /// The bounded heatmap for the current scene, or `None` when the toggle is
    /// off, the scalar view is unsupported, or the cached grid no longer
    /// matches the current formula/frame/clock. Colours reuse the active
    /// [`ColorMapping`]; the cache supplies the one evaluator.
    pub fn scalar_heatmap(self: &State) -> Option<Arc<HeatmapGPU>> {
        if !self.scalar_view_applicable() || !self.show_scalar_heatmap {
            return None;
        }
        if !self
            .scalar_view_cache
            .record_matches_grid(&self.scalar_view_identity())
        {
            return None;
        }
        let grid = &self.scalar_view_cache.record()?.grid;
        HeatmapGPU::from_grid(grid, &self.color_mapping()).map(Arc::new)
    }

    /// Legend facts for the heatmap currently on screen. Mirrors the exact
    /// [`Self::scalar_heatmap`] gate so the panel cannot describe a different
    /// field from the one painted.
    pub fn scalar_heatmap_legend(self: &State) -> Option<HeatmapLegend> {
        if !self.scalar_view_applicable() || !self.show_scalar_heatmap {
            return None;
        }
        if !self
            .scalar_view_cache
            .record_matches_grid(&self.scalar_view_identity())
        {
            return None;
        }
        let grid = &self.scalar_view_cache.record()?.grid;
        heatmap_legend(grid, &self.color_mapping())
    }

    /// Validate and commit the Scalar view grid side. A rejected value leaves
    /// the committed side untouched; the layout key picks the change up on the
    /// next visible refresh.
    pub fn set_scalar_grid_side(self: &mut State, side: u32) -> Result<(), ScalarViewError> {
        if !(MIN_SCALAR_GRID_SIDE..=MAX_GRID_SIDE).contains(&side) {
            return Err(ScalarViewError::GridSize {
                min: MIN_SCALAR_GRID_SIDE,
                max: MAX_GRID_SIDE,
                got: side,
            });
        }
        self.scalar_grid_side = side;
        Ok(())
    }

    /// Validate and commit the explicit contour-level count (`0..=MAX_CONTOUR_LEVELS`).
    /// A rejected value leaves the committed count and the dirty flag untouched.
    /// An accepted real change dirties the scene once and bumps the plot
    /// generation, because the count changes what the contour overlay samples.
    /// The display-only scalar toggles (`set_show_scalar_heatmap`,
    /// `set_scalar_text_color`) only mark dirty; this setter follows the
    /// sampled-geometry convention used by `set_domain` and the colour setters.
    pub fn set_scalar_contour_levels(self: &mut State, levels: u32) -> Result<(), ScalarViewError> {
        if levels as usize > MAX_CONTOUR_LEVELS {
            return Err(ScalarViewError::ContourLevels {
                reason: format!("at most {MAX_CONTOUR_LEVELS} levels are allowed"),
            });
        }
        if self.scalar_contour_levels == levels {
            return Ok(());
        }
        self.scalar_contour_levels = levels;
        self.plot.generation += 1;
        self.mark_dirty();
        Ok(())
    }

    /// The theme text colour the scalar overlay lines use (mirrored by the
    /// sidebar so captures match the panel text).
    pub fn scalar_text_color(self: &State) -> [f32; 4] {
        self.scalar_text_color
    }

    /// Mirror the active theme's text colour into the scene overlay lines.
    /// Only a real change dirties the scene, so a theme switch while the
    /// section is open reaches the next snapshot.
    pub fn set_scalar_text_color(self: &mut State, color: [f32; 4]) {
        if color.iter().all(|component| component.is_finite()) && self.scalar_text_color != color {
            self.scalar_text_color = color;
            self.mark_dirty();
        }
    }

    /// Replace the display-only comparison series after validating the whole
    /// set. A rejected set leaves the previous series and dirty flag untouched;
    /// an accepted real change dirties once. Arrows, colour mapping and probes
    /// are never touched.
    pub fn set_comparison_series(self: &mut State, next: ComparisonSet) -> Result<(), String> {
        next.validate()?;
        if next == self.comparison_series {
            return Ok(());
        }
        self.comparison_series = next;
        self.mark_dirty();
        Ok(())
    }

    /// Replace the potential-gradient request after validating it. `None`
    /// clears the request. A rejected spec leaves the previous request and
    /// dirty flag untouched; an accepted real change dirties once.
    pub fn set_gradient(self: &mut State, next: Option<GradientSpec>) -> Result<(), String> {
        if let Some(spec) = &next {
            spec.validate()?;
        }
        if next == self.gradient {
            return Ok(());
        }
        self.gradient = next;
        self.mark_dirty();
        Ok(())
    }

    /// Append one point probe through the shared cap and validation seam. The
    /// edit dirties the document; sampled values stay runtime state.
    pub fn add_point_probe(
        self: &mut State,
        position: Vec3,
        anchor: Option<SeedAnchor>,
    ) -> Result<usize, ScalarViewError> {
        let probe = PointProbe::new(position, anchor)?;
        self.probes.push_point(probe)?;
        let index = self.probes.points.len().saturating_sub(1);
        if let Ok(expr) = scalar_view_expr(&self.plot.surface.source, &self.parameters) {
            let frame = self.scalar_view_frame();
            if let Some(probe) = self.probes.points.get_mut(index) {
                probe.evaluate(&expr, &frame, f64::from(self.time));
            }
        }
        self.mark_dirty();
        Ok(index)
    }

    /// Remove one point probe by index. An out-of-range index is a no-op; a
    /// real removal is one dirty document edit.
    pub fn remove_point_probe(self: &mut State, index: usize) -> bool {
        if index >= self.probes.points.len() {
            return false;
        }
        self.probes.points.remove(index);
        self.mark_dirty();
        true
    }

    /// Contour and point-probe line instances for the scene. Contours use the
    /// cached record's plane frame and the established reference edge width;
    /// each probe is a small on-slice cross. Geometry is composed only while
    /// the record still matches the current non-previewed identity, so a
    /// collapsed section cannot paint contours or crosses from a formula,
    /// plane, domain or clock that has since changed. Non-finite geometry is
    /// skipped.
    pub fn scalar_overlay_lines(self: &State) -> Vec<LineInstanceGPU> {
        if !self.scalar_view_applicable() {
            return Vec::new();
        }
        let color = self.scalar_text_color;
        let width = crate::reference::EDGE_WIDTH;
        let stale = self.scalar_view_cache.record().is_some()
            && !self
                .scalar_view_cache
                .record_matches(&self.scalar_view_identity());
        if stale {
            return Vec::new();
        }
        let mut lines = Vec::new();
        if let Some(record) = self.scalar_view_cache.record() {
            let frame = record.grid.frame;
            for segment in &record.contours {
                if !segment.start.is_finite() || !segment.end.is_finite() {
                    continue;
                }
                let a = frame.to_world(Vec3::new(segment.start.x, segment.start.y, 0.0));
                let b = frame.to_world(Vec3::new(segment.end.x, segment.end.y, 0.0));
                if !a.is_finite() || !b.is_finite() {
                    continue;
                }
                lines.push(LineInstanceGPU {
                    a: [a.x, a.y, a.z, 1.0],
                    b: [b.x, b.y, b.z, width],
                    color,
                });
            }
        }

        let frame = self.scalar_view_frame();
        let extent = frame.half_u.max(frame.half_v) * 2.0;
        let half = (extent * 0.015).max(0.001);
        for probe in &self.probes.points {
            let position = probe.position;
            if !position.is_finite() {
                continue;
            }
            let right = frame.right * half;
            let up = frame.up * half;
            for delta in [right, up] {
                lines.push(LineInstanceGPU {
                    a: [
                        position.x - delta.x,
                        position.y - delta.y,
                        position.z - delta.z,
                        1.0,
                    ],
                    b: [
                        position.x + delta.x,
                        position.y + delta.y,
                        position.z + delta.z,
                        width,
                    ],
                    color,
                });
            }
        }
        // Issue #93 slice B: comparison curves and gradient arrows ride the
        // same line collection as contours/probes, so PNG/SVG/animation
        // captures include them without a new capture path. The cache is only
        // sampled by the visible panel; this composition never evaluates.
        lines.extend_from_slice(&self.scalar_analysis.curves);
        lines.extend_from_slice(&self.scalar_analysis.gradient);
        lines
    }

    /// Refresh the scalar-view cache while the panel is visible.
    ///
    /// Hidden panels do no sampling: the last record is returned untouched. A
    /// visible refresh also re-evaluates every point probe's runtime value. A
    /// changed key (formula, parameters, frame, domain, grid side, levels or the
    /// 100 ms-quantised clock) causes exactly one rebuild and marks the scene
    /// dirty so contour/probe geometry reaches the next snapshot. An unchanged
    /// key is a cache hit and does not dirty.
    pub fn refresh_scalar_view(self: &mut State, visible: bool) -> Option<&ScalarViewRecord> {
        if !visible {
            return self.scalar_view_cache.record();
        }
        let request = self.scalar_view_request();
        match scalar_view_expr(&request.source, &request.parameters) {
            Ok(expr) => {
                for probe in &mut self.probes.points {
                    probe.evaluate(&expr, &request.frame, request.time);
                }
            }
            Err(_) => {
                for probe in &mut self.probes.points {
                    probe.value = None;
                }
            }
        }
        let before = self.scalar_view_cache.resamples();
        let _ = self.scalar_view_cache.refresh(&request, true);
        if self.scalar_view_cache.resamples() != before {
            self.mark_dirty();
        }
        self.scalar_view_cache.record()
    }

    /// The runtime scalar-view cache (records, failure reason, resample count).
    pub fn scalar_view_cache(self: &State) -> &ScalarViewCache {
        &self.scalar_view_cache
    }

    /// Refresh the comparison-curve and gradient-arrow geometry while the
    /// Scalar analysis subsection is visible.
    ///
    /// A hidden panel performs no sampling and keeps the previous geometry.
    /// A visible refresh samples only when its identity changed: the series,
    /// gradient request, committed parameters, domain, plane frame, curve
    /// sample count, text colour or the 100 ms-quantised clock (the #92 10 Hz
    /// rule). An unchanged identity is a cache hit and does not dirty.
    pub fn refresh_scalar_analysis(self: &mut State, visible: bool) {
        if !visible || !self.scalar_view_applicable() {
            return;
        }
        let key = ScalarAnalysisKey::new(self);
        if self.scalar_analysis.key.as_ref() == Some(&key) {
            return;
        }
        self.scalar_analysis.key = Some(key);
        self.scalar_analysis.resamples += 1;

        let frame = self.scalar_view_frame();
        let time = self.time;
        let mut curves = Vec::new();
        let mut series_error = None;
        for series in self.comparison_series.iter() {
            match comparison_curve_lines(series, &frame, self.plot.settings.curve_samples, time) {
                Ok(lines) => curves.extend(lines),
                Err(reason) => {
                    series_error.get_or_insert(reason);
                }
            }
        }

        let (gradient, counts, error) = match &self.gradient {
            Some(spec) => {
                let base = if spec.role == SeriesRole::ElectricPotential {
                    fixed_color(crate::source_scene::ELECTRIC_COLOR)
                } else {
                    self.scalar_text_color
                };
                match sample_gradient(spec, &self.parameters, f64::from(time), &self.domain) {
                    Ok(samples) => {
                        let (lines, counts) = gradient_arrow_lines(&samples, &spec.frame, base);
                        (lines, counts, None)
                    }
                    Err(reason) => (Vec::new(), ScalarAnalysisCounts::default(), Some(reason)),
                }
            }
            None => (Vec::new(), ScalarAnalysisCounts::default(), None),
        };
        let error = error.or(series_error);

        let had_geometry =
            !self.scalar_analysis.curves.is_empty() || !self.scalar_analysis.gradient.is_empty();
        let has_geometry = !curves.is_empty() || !gradient.is_empty();
        self.scalar_analysis.curves = curves;
        self.scalar_analysis.gradient = gradient;
        self.scalar_analysis.counts = counts;
        self.scalar_analysis.error = error;
        // An empty overlay (no series, no gradient) needs no scene rebuild; a
        // real geometry change or a removal does.
        if had_geometry || has_geometry {
            self.mark_dirty();
        }
    }

    /// The gradient readout from the most recent visible refresh.
    pub fn scalar_analysis_counts(self: &State) -> ScalarAnalysisCounts {
        self.scalar_analysis.counts
    }

    /// The last unsupported/parse reason for the gradient source or a
    /// comparison series, if any. A gradient failure takes precedence.
    pub fn scalar_analysis_error(self: &State) -> Option<&str> {
        self.scalar_analysis.error.as_deref()
    }

    /// Number of visible analysis rebuilds; hidden panels never increment it.
    pub fn scalar_analysis_resamples(self: &State) -> u64 {
        self.scalar_analysis.resamples
    }

    /// The domain the visible scene covers. A plane slice uses its flat local
    /// domain; 3D uses the world domain; ordinary two-component 2D keeps the
    /// world X/Y box with z collapsed to its midpoint.
    pub fn scene_domain(self: &State) -> Domain {
        if self.field_mode == FieldMode::Ode {
            // The camera frames the precomputed trajectory bounds; before the
            // first lazy solve the durable field domain is the stable fallback.
            return self
                .ode_solution
                .as_ref()
                .map(|solution| solution.view_domain())
                .unwrap_or(self.domain);
        }
        if let Some(frame) = self.plane_frame() {
            frame.local_domain()
        } else if self.mode_3d {
            self.domain
        } else {
            let z = self.domain.center().z;
            Domain::new(
                Vec3::new(self.domain.min.x, self.domain.min.y, z),
                Vec3::new(self.domain.max.x, self.domain.max.y, z),
            )
        }
    }

    /// The flow the visible scene traces. A plane slice projects the world
    /// field onto its local frame; every other view traces the world field
    /// over [`scene_domain`](State::scene_domain).
    pub fn scene_flow(self: &State) -> SceneFlow<'_> {
        match self.plane_frame() {
            Some(frame) => SceneFlow::Plane(super::flow::PlaneFlow {
                field: &self.field,
                frame,
            }),
            None => SceneFlow::World(&self.field),
        }
    }

    /// The sampling grid for the density: `n`×`n` (one z slice) in 2D mode,
    /// `n`×`n`×`n` when sampling runs three-dimensional.
    pub fn grid(self: &State) -> UVec3 {
        let n = self.capped_density();
        if self.is_3d() {
            UVec3::new(n, n, n)
        } else {
            UVec3::new(n, n, 1)
        }
    }

    /// The stored density with the source-mode 3D ceiling applied defensively.
    /// Transition setters clamp `density` itself; this also covers any direct
    /// field write so sampling can never exceed 40³ in Source mode.
    fn capped_density(self: &State) -> u32 {
        let n = self.density.max(MIN_DENSITY);
        match self.field_mode {
            FieldMode::Sources if self.is_3d() => n.min(MAX_DENSITY_3D),
            FieldMode::Materials => {
                let cap = if self.is_3d() {
                    MATERIAL_MAX_DENSITY_3D
                } else {
                    MATERIAL_MAX_DENSITY_2D
                };
                n.min(cap)
            }
            FieldMode::Induction => {
                let cap = if self.is_3d() {
                    MAX_INDUCTION_DENSITY_3D
                } else {
                    MAX_INDUCTION_DENSITY_2D
                };
                n.min(cap)
            }
            FieldMode::Maxwell => {
                let cap = if self.is_3d() {
                    MAXWELL_MAX_DENSITY_3D
                } else {
                    MAXWELL_MAX_DENSITY_2D
                };
                n.min(cap)
            }
            _ => n,
        }
    }

    /// The colour settings projected onto what the sampler consumes: with
    /// auto-fit on the range is `None` so the sampler fits the sampled
    /// magnitudes itself.
    pub fn view_options(self: &State) -> ViewOptions {
        let color_mapping = self.color_mapping();
        let color_mapping = if color_mapping.is_legacy_equivalent(self.colormap) {
            None
        } else {
            Some(color_mapping)
        };
        ViewOptions {
            colormap: self.colormap,
            color_range: if self.auto_fit {
                None
            } else {
                Some((self.color_min, self.color_max))
            },
            color_mapping,
        }
    }

    /// Resample the field with this state's grid and colour settings. A 2D
    /// slice of a three-component field samples its flat local plane frame, so
    /// positions, directions and magnitudes are all local; every other view
    /// keeps sampling the world domain exactly as before.
    pub fn sample(self: &State) -> Vec<ArrowData> {
        if let Some(frame) = self.plane_frame() {
            return sample_plane_at(
                &self.field,
                &frame,
                self.grid().x,
                &self.view_options(),
                self.time,
            );
        }
        sample_field_at(
            &self.field,
            &self.domain,
            self.grid(),
            &self.view_options(),
            self.time,
        )
    }

    /// Size of one grid cell along the tightest axis: arrow lengths are
    /// multiples of this, so a denser grid keeps arrows the same relative
    /// size instead of piling them up.
    pub fn spacing(self: &State) -> f32 {
        let grid = self.grid();
        let size = self.scene_domain().domain_size();
        let cell = |extent, n| {
            if n <= 1 {
                extent
            } else {
                extent / (n - 1) as f32
            }
        };
        let tightest = cell(size.x, grid.x).min(cell(size.y, grid.y));
        if grid.z > 1 {
            tightest.min(cell(size.z, grid.z))
        } else {
            tightest
        }
    }

    /// The dot radius in world units: the base fraction of one grid cell
    /// scaled by the user's [`State::particle_size`] (ticket 30).
    pub fn particle_radius(self: &State, spacing: f32) -> f32 {
        DOT_SPACING_FRACTION * spacing * self.particle_size
    }

    /// The colour range the painted field geometry shares: the state's explicit
    /// bounds when auto-fit is off, otherwise the magnitudes of `sampled`. The
    /// streamlines built from `sampled` and the app's live dot cloud both ask
    /// for this, so dots always ramp the same colours as the arrows and
    /// streamline segments next to them.
    pub fn color_range(self: &State, sampled: &[ArrowData]) -> (f32, f32) {
        self.view_options()
            .color_range
            .unwrap_or_else(|| magnitude_range(sampled))
    }

    /// Convert sampled arrows into GPU instances at this state's arrow
    /// lengths. `arrow_scale` is the length multiplier for
    /// [`ArrowLengthMode::Proportional`] (longest arrow exactly `arrow_scale`
    /// grid cells, shorter ones following their magnitude) and
    /// [`ArrowLengthMode::Uniform`] (every arrow exactly `arrow_scale` cells).
    /// In Proportional mode `proportional_exponent` is the power applied to the
    /// normalised magnitude first, so the contrast between short and long
    /// arrows can change without touching the overall scale; at `1.0` the
    /// mapping is the plain linear one. With [`ArrowLengthMode::Magnitude`] the
    /// length is instead `magnitude_scale * mag` in world units — `arrow_scale`
    /// does not apply, and the result is deliberately not clamped, so a field
    /// with a huge magnitude produces a huge arrow and the camera does the
    /// fitting.
    ///
    /// Plane-by-plane mode (ticket 23) keeps the selected slice's colormap
    /// colour at the state's arrow opacity and lerps every other arrow's colour
    /// toward grey (the default off-plane alpha is [`DIM_ALPHA`]). Ticket 36
    /// scales that dimming by [`State::plane_dim_opacity`]: on-plane arrows get
    /// `arrow_opacity`, off-plane arrows `arrow_opacity * plane_dim_opacity`.
    /// Sampling still produced the whole cube, so stepping the plane only
    /// rebuilds these instances, never the field.
    pub fn arrow_instances(self: &State, arrows: &[ArrowData]) -> Vec<ArrowInstanceGPU> {
        self.arrow_instances_with_styles(arrows, self.layer_styles())
    }

    /// [`State::arrow_instances`] for an explicit style record, so a blueprint
    /// snapshot can ignore the committed Arrows overrides.
    fn arrow_instances_with_styles(
        self: &State,
        arrows: &[ArrowData],
        styles: &LayerStyles,
    ) -> Vec<ArrowInstanceGPU> {
        let unit = self.arrow_scale * self.spacing();
        let max_mag = arrows.iter().fold(0.0_f32, |m, a| m.max(a.mag));
        let plane_active = self.plane_active();
        let grid = self.grid();
        let domain = self.domain;
        let resolved = self.arrow_layer_resolution(styles);
        let layer_opacity = sanitized_opacity(resolved.opacity);
        let family_rgb = resolved.color.map(|color| color.0);
        arrows
            .iter()
            .map(|a| {
                let len = match self.arrow_length_mode {
                    ArrowLengthMode::Uniform => unit,
                    ArrowLengthMode::Proportional => {
                        if max_mag <= 0.0 {
                            unit
                        } else {
                            let normalized = a.mag / max_mag;
                            // `powf(1.0)` is not guaranteed to return the base
                            // bit for bit, so the linear default takes the
                            // pre-ticket-57 expression unchanged.
                            let factor = if self.proportional_exponent == 1.0 {
                                normalized
                            } else {
                                normalized.powf(self.proportional_exponent)
                            };
                            unit * factor
                        }
                    }
                    ArrowLengthMode::Magnitude => a.mag * self.magnitude_scale,
                };
                let base_rgb = family_rgb.unwrap_or(a.color);
                let (rgb, alpha) =
                    if plane_active && !on_plane(a.position, &domain, grid, &self.plane) {
                        (
                            dim_color(base_rgb),
                            (self.arrow_opacity * self.plane_dim_opacity * layer_opacity)
                                .clamp(0.0, 1.0),
                        )
                    } else {
                        (
                            base_rgb,
                            (self.arrow_opacity * layer_opacity).clamp(0.0, 1.0),
                        )
                    };
                ArrowInstanceGPU {
                    origin: [a.position.x, a.position.y, a.position.z, 1.0],
                    dir_len: [a.dir.x, a.dir.y, a.dir.z, len],
                    color: [
                        rgb[0] as f32 / 255.0,
                        rgb[1] as f32 / 255.0,
                        rgb[2] as f32 / 255.0,
                        alpha,
                    ],
                }
            })
            .collect()
    }

    /// Line instances for the 2D streamlines: a jittered seed grid (deterministic
    /// across rebuilds, salt-keyed to the sampling grid so a density change
    /// reseeds), each seed RK4-traced through the field, one instance per
    /// segment coloured like the arrows it crosses — same magnitude range as
    /// the arrows. [`STREAMLINE_MAX_LEN`] sets the per-line point budget and
    /// the grid spacing sets the step scale. Streamlines are a static trace,
    /// not clock consumers, so the transport speed never changes their length.
    /// Both views trace the same RK4 [`streamlines::trace`] through the same
    /// 3D-sampled field: the pan/zoom view uses the jittered grid of the view
    /// plane, while the orbit view uses the same grid at the domain's mid-depth
    /// so its lines run with the depth they cross. [`streamlines::ray_seed`]
    /// turns a click into a seed inside the domain for the orbit view's click-to-seed.
    pub fn streamlines_snapshot(self: &State, sampled: &[ArrowData]) -> Vec<LineInstanceGPU> {
        self.streamlines_snapshot_with_styles(sampled, self.layer_styles())
    }

    fn streamlines_snapshot_with_styles(
        self: &State,
        sampled: &[ArrowData],
        styles: &LayerStyles,
    ) -> Vec<LineInstanceGPU> {
        // Source/induction draw arrows only and Ode draws its own
        // trajectories; formula streamlines never leak into any of them.
        if matches!(
            self.field_mode,
            FieldMode::Sources
                | FieldMode::Induction
                | FieldMode::Materials
                | FieldMode::Ode
                | FieldMode::Maxwell
        ) {
            return Vec::new();
        }
        // Off hides both seed sources. Click hides only the automatic grid:
        // an explicit layout is document geometry and still paints there.
        if !self.streamlines
            || (self.seed_mode == StreamlineSeedMode::Click && self.seed_layout.is_empty())
        {
            return Vec::new();
        }
        // An explicit layout replaces both the density-derived grid and the
        // explicit count. Otherwise half the grid density seeds the plane:
        // enough lines to read the flow without tracing one per sampled point.
        // An explicit count instead uses a density-independent lattice; its
        // fixed salt keeps the seeds stable as the density changes.
        let grid = self.grid();
        let seeds: Vec<Vec3> = if !self.seed_layout.is_empty() {
            self.seed_layout
                .seeds
                .iter()
                .map(|seed| seed.position)
                .collect()
        } else if self.streamline_count == 0 {
            let axis = (self.density / 3).clamp(1, 24);
            let salt = grid.x as u64 ^ ((grid.y as u64) << 16);
            seed_grid(&self.scene_domain(), axis, STREAMLINE_JITTER, salt)
        } else {
            explicit_streamline_seeds(&self.scene_domain(), self.streamline_count, self.is_3d(), 0)
        };
        let opts = self.streamline_opts();
        let range = self.color_range(sampled);
        let mapping = self.view_options().color_mapping;
        let stats = mapping.as_ref().map(|_| {
            ColorStats::from_samples(
                &sampled
                    .iter()
                    .map(|arrow| ColorSample {
                        magnitude: arrow.mag,
                        dir: arrow.dir,
                    })
                    .collect::<Vec<_>>(),
            )
        });
        let coloring = mapping.as_ref().zip(stats.as_ref());
        let layer = self.flow_layer_style_for(styles);
        let mut lines: Vec<LineInstanceGPU> = Vec::new();
        for seed in seeds {
            self.push_line(&mut lines, seed, &opts, range, coloring, layer);
        }
        lines
    }

    /// The tracing parameters both seed sources share: a fixed point budget
    /// ([`STREAMLINE_MAX_LEN`], bounded inside the tracer's own 400-point cap)
    /// and a step scale from the grid spacing. Streamlines are static, so no
    /// transport or particle setting changes these.
    fn streamline_opts(self: &State) -> StreamlineOpts {
        StreamlineOpts {
            max_len: STREAMLINE_MAX_LEN,
            step_scale: 0.5 * self.spacing(),
            both_directions: true,
        }
    }

    /// Trace one seed and push its coloured segments: one instance per segment,
    /// coloured at the segment's start by the same [`color_range`] the arrows
    /// use. The trace and the colour vector both come from
    /// [`scene_flow`](State::scene_flow), so a slice stays in its local frame
    /// and colours by the projected vector. `coloring` carries the active
    /// mapping and its arrow-derived stats; `None` keeps the legacy colormap
    /// path byte-identical. Shared by the grid seeds and the click bursts so
    /// both paint identically.
    fn push_line(
        self: &State,
        lines: &mut Vec<LineInstanceGPU>,
        seed: Vec3,
        opts: &StreamlineOpts,
        range: (f32, f32),
        coloring: Option<(&ColorMapping, &ColorStats)>,
        layer: FlowLayerStyle,
    ) {
        let (lo, hi) = range;
        let flow = self.scene_flow();
        let path = trace_at(&flow, seed, &self.scene_domain(), opts, self.time);
        for i in 0..path.len().saturating_sub(1) {
            let pa = path[i];
            let pb = path[i + 1];
            let vector = flow.eval(pa, self.time);
            let rgb = match coloring {
                Some((mapping, stats)) => match vector {
                    Some(v) if v.length().is_finite() && v.length() != 0.0 => {
                        let magnitude = v.length();
                        mapping
                            .sample(
                                &ColorSample {
                                    magnitude,
                                    dir: v / magnitude,
                                },
                                stats,
                            )
                            .rgb
                    }
                    _ => STREAMLINE_MID_GRAY,
                },
                None => {
                    let mag = match vector {
                        Some(v) => v.length(),
                        None => 0.0,
                    };
                    self.colormap.colormap(mag, lo, hi)
                }
            };
            lines.push(LineInstanceGPU {
                a: [pa.x, pa.y, pa.z, 1.0],
                b: [pb.x, pb.y, pb.z, STREAMLINE_WIDTH * layer.width],
                color: [
                    rgb[0] as f32 / 255.0,
                    rgb[1] as f32 / 255.0,
                    rgb[2] as f32 / 255.0,
                    sanitized_opacity(layer.opacity),
                ],
            });
        }
    }

    /// Line instances for the click-seeded streamlines: every click expands to
    /// a deterministic [`burst_seeds`] cluster (3D depth in the orbit view, on
    /// the clicked plane in 2D), each seed traced exactly like a grid seed.
    /// Both seed modes accept clicks, but the Feature toggle gates them like
    /// the grid: off hides every click line and the app stores no new seeds.
    /// Pure; the app owns the (bounded) list of clicks.
    pub fn click_streamlines_snapshot(
        self: &State,
        color_range: (f32, f32),
        clicks: &[Vec3],
    ) -> Vec<LineInstanceGPU> {
        self.click_streamlines_snapshot_with_styles(color_range, clicks, self.layer_styles())
    }

    /// [`State::click_streamlines_snapshot`] for an explicit style record.
    pub(crate) fn click_streamlines_snapshot_with_styles(
        self: &State,
        color_range: (f32, f32),
        clicks: &[Vec3],
        styles: &LayerStyles,
    ) -> Vec<LineInstanceGPU> {
        if matches!(
            self.field_mode,
            FieldMode::Sources | FieldMode::Induction | FieldMode::Materials | FieldMode::Ode
        ) || !self.streamlines
        {
            return Vec::new();
        }
        let opts = self.streamline_opts();
        let dimension = if self.mode_3d { 3 } else { 2 };
        let mapping = self.view_options().color_mapping;
        // Clicks carry no sampled-arrow slice of their own; reuse the view's
        // sampled arrows so both seed sources share one colour scale.
        let stats = mapping.as_ref().map(|_| {
            let sampled = self.sample();
            ColorStats::from_samples(
                &sampled
                    .iter()
                    .map(|arrow| ColorSample {
                        magnitude: arrow.mag,
                        dir: arrow.dir,
                    })
                    .collect::<Vec<_>>(),
            )
        });
        let coloring = mapping.as_ref().zip(stats.as_ref());
        let layer = self.flow_layer_style_for(styles);
        let mut lines: Vec<LineInstanceGPU> = Vec::new();
        for click in clicks {
            for seed in burst_seeds(*click, &self.scene_domain(), CLICK_BURST_SEEDS, dimension) {
                self.push_line(&mut lines, seed, &opts, color_range, coloring, layer);
            }
        }
        lines
    }

    /// The whole GPU scene for this state: arrows plus streamlines —
    /// particle tasks add `dots` later. Pure: no GPU resources touched, so
    /// the UI layer just stores the result; the field is sampled once and
    /// reused for the arrows and the streamline colour range.
    pub fn snapshot(self: &State) -> SceneGPU {
        self.snapshot_with_styles(self.layer_styles())
    }

    /// The unstyled line blueprint used by SVG/vector exports: the same scene
    /// geometry as [`State::snapshot`] but with every [`LayerStyles`] override
    /// neutral. Raster output keeps the styled [`State::snapshot`].
    pub fn snapshot_blueprint(self: &State) -> SceneGPU {
        self.snapshot_with_styles(&LayerStyles::default())
    }

    /// Build the scene with an explicit layer-style record. `snapshot` passes
    /// the committed styles; `snapshot_blueprint` passes the neutral default.
    pub(crate) fn snapshot_with_styles(self: &State, styles: &LayerStyles) -> SceneGPU {
        if self.field_mode == FieldMode::Ode {
            // Ode owns precomputed trajectory lines and per-frame markers in
            // the app; the pure field snapshot never samples the formula.
            return SceneGPU {
                arrows: Arc::new(Vec::new()),
                lines: Arc::new(Vec::new()),
                trails: Arc::new(Vec::new()),
                dots: Arc::new(Vec::new()),
                combined: Arc::new(Vec::new()),
                heatmap: None,
                arrow_style: self.arrow_style,
                arrow_style_2d: self.arrow_style_2d,
                arrow_params: self.effective_arrow_params_for(styles),
                mode_3d: self.mode_3d,
            };
        }
        if self.field_mode == FieldMode::Induction {
            // Arrows-only demo: no streamlines, click lines, trails or dots.
            let arrows = if self.arrows {
                self.induction_sampling
                    .as_ref()
                    .map(|sampling| self.arrow_instances_with_styles(&sampling.arrows, styles))
                    .unwrap_or_default()
            } else {
                Vec::new()
            };
            return SceneGPU {
                arrows: Arc::new(arrows),
                lines: Arc::new(Vec::new()),
                trails: Arc::new(Vec::new()),
                dots: Arc::new(Vec::new()),
                combined: Arc::new(Vec::new()),
                heatmap: None,
                arrow_style: self.arrow_style,
                arrow_style_2d: self.arrow_style_2d,
                arrow_params: self.effective_arrow_params_for(styles),
                mode_3d: self.mode_3d,
            };
        }
        if self.field_mode == FieldMode::Sources {
            let arrows = if self.arrows {
                match &self.source_sampling {
                    Some(sampling) => self.arrow_instances_with_styles(&sampling.arrows, styles),
                    None => Vec::new(),
                }
            } else {
                Vec::new()
            };
            return SceneGPU {
                arrows: Arc::new(arrows),
                lines: Arc::new(Vec::new()),
                trails: Arc::new(Vec::new()),
                dots: Arc::new(Vec::new()),
                combined: Arc::new(Vec::new()),
                heatmap: None,
                arrow_style: self.arrow_style,
                arrow_style_2d: self.arrow_style_2d,
                arrow_params: self.effective_arrow_params_for(styles),
                mode_3d: self.mode_3d,
            };
        }
        if self.field_mode == FieldMode::Maxwell {
            // Arrows plus the case outline only. No particles, streamlines,
            // click seeds, source glyphs or formula sampling.
            let arrows = if self.arrows {
                self.maxwell_sampling
                    .as_ref()
                    .map(|sampling| self.arrow_instances_with_styles(&sampling.arrows, styles))
                    .unwrap_or_default()
            } else {
                Vec::new()
            };
            let plane = self.plane_frame();
            let domain = self.scene_domain();
            let size = domain.domain_size();
            let extent = size.x.abs().max(size.y.abs()).max(size.z.abs());
            let width = 0.004 * extent;
            let width = if width.is_finite() && width > 0.0 {
                width
            } else {
                crate::reference::EDGE_WIDTH
            };
            let resolved = styles.resolve(
                LayerId::TeachingScene,
                Some(physics_family_of_maxwell(maxwell_family(self.maxwell.case))),
            );
            let opacity = sanitized_opacity(resolved.opacity);
            let width_multiplier = sanitized_width_for_style(resolved.width);
            let lines = maxwell_outline(&self.maxwell, &domain, plane.as_ref())
                .into_iter()
                .map(|segment| LineInstanceGPU {
                    a: [segment.a[0], segment.a[1], segment.a[2], 1.0],
                    b: [
                        segment.b[0],
                        segment.b[1],
                        segment.b[2],
                        styled_teaching_line_width(width, width_multiplier),
                    ],
                    color: styled_teaching_line_color(segment.color, opacity, resolved.color),
                })
                .collect::<Vec<_>>();
            return SceneGPU {
                arrows: Arc::new(arrows),
                lines: Arc::new(lines),
                trails: Arc::new(Vec::new()),
                dots: Arc::new(Vec::new()),
                combined: Arc::new(Vec::new()),
                heatmap: None,
                arrow_style: self.arrow_style,
                arrow_style_2d: self.arrow_style_2d,
                arrow_params: self.effective_arrow_params_for(styles),
                mode_3d: self.mode_3d,
            };
        }
        if self.field_mode == FieldMode::Materials {
            // Arrows plus only the selected case's simple outline. No
            // particles, streamlines, click seeds, glyphs or scalar plot.
            let arrows = if self.arrows {
                self.material_sampling
                    .as_ref()
                    .map(|sampling| self.arrow_instances_with_styles(&sampling.arrows, styles))
                    .unwrap_or_default()
            } else {
                Vec::new()
            };
            let plane = self.plane_frame();
            let resolved = styles.resolve(
                LayerId::TeachingScene,
                Some(physics_family_of_material(material_family(
                    self.material.case,
                ))),
            );
            let opacity = sanitized_opacity(resolved.opacity);
            let width_multiplier = sanitized_width_for_style(resolved.width);
            let lines = material_outline(&self.material, &self.scene_domain(), plane.as_ref())
                .into_iter()
                .map(|segment| LineInstanceGPU {
                    a: [segment.a[0], segment.a[1], segment.a[2], 1.0],
                    b: [
                        segment.b[0],
                        segment.b[1],
                        segment.b[2],
                        styled_teaching_line_width(crate::reference::EDGE_WIDTH, width_multiplier),
                    ],
                    color: styled_teaching_line_color(segment.color, opacity, resolved.color),
                })
                .collect::<Vec<_>>();
            return SceneGPU {
                arrows: Arc::new(arrows),
                lines: Arc::new(lines),
                trails: Arc::new(Vec::new()),
                dots: Arc::new(Vec::new()),
                combined: Arc::new(Vec::new()),
                heatmap: None,
                arrow_style: self.arrow_style,
                arrow_style_2d: self.arrow_style_2d,
                arrow_params: self.effective_arrow_params_for(styles),
                mode_3d: self.mode_3d,
            };
        }
        let sampled = self.sample();
        let arrows = if self.arrows {
            self.arrow_instances_with_styles(&sampled, styles)
        } else {
            Vec::new()
        };
        let mut lines = self.streamlines_snapshot_with_styles(&sampled, styles);
        // Issue #92 slice B1: contour segments and point-probe crosses ride the
        // normal line collection, so PNG/SVG/animation captures include them
        // without a new capture path.
        lines.extend(self.scalar_overlay_lines());
        SceneGPU {
            arrows: Arc::new(arrows),
            lines: Arc::new(lines),
            // Ticket 48: trails come from the live cloud in the app's particle
            // block, never from this dirty-flag snapshot.
            trails: Arc::new(Vec::new()),
            dots: Arc::new(Vec::new()),
            // Ticket 62: the app builds and sorts the per-frame combined
            // arrow + particle stream, because dots advect every frame. The
            // viewmodel's arrows stay frame-invariant in sampling order.
            combined: Arc::new(Vec::new()),
            heatmap: self.scalar_heatmap(),
            arrow_style: self.arrow_style,
            arrow_style_2d: self.arrow_style_2d,
            arrow_params: self.effective_arrow_params_for(styles),
            mode_3d: self.mode_3d,
        }
    }

    /// Rebuild the scene only when the state has been marked dirty since the
    /// last rebuild: `Some` means the caller must replace its scene, `None`
    /// means the last snapshot is still current.
    pub fn snapshot_if_dirty(self: &mut State) -> Option<SceneGPU> {
        if !self.dirty {
            return None;
        }
        match self.field_mode {
            FieldMode::Induction => self.refresh_induction(),
            FieldMode::Materials => self.refresh_material_sampling(),
            FieldMode::Maxwell => self.refresh_maxwell_sampling(),
            _ => self.refresh_source_sampling(),
        }
        self.dirty = false;
        Some(self.snapshot())
    }
}

/// Defensive width repair for the public style record: a non-finite value is
/// neutral, a finite one clamps into the A1 bounds.
fn sanitized_width_for_style(width: f32) -> f32 {
    if width.is_finite() {
        width.clamp(
            crate::layer_style::MIN_WIDTH_MULTIPLIER,
            crate::layer_style::MAX_WIDTH_MULTIPLIER,
        )
    } else {
        1.0
    }
}

/// The Teaching-scene family axis for a Materials case.
fn physics_family_of_material(family: MaterialFamily) -> PhysicsFamily {
    match family {
        MaterialFamily::Electric => PhysicsFamily::Electric,
        MaterialFamily::Magnetic => PhysicsFamily::Magnetic,
    }
}

/// The Teaching-scene family axis for a Maxwell case.
fn physics_family_of_maxwell(family: MaxwellFamily) -> PhysicsFamily {
    match family {
        MaxwellFamily::Electric => PhysicsFamily::Electric,
        MaxwellFamily::Magnetic => PhysicsFamily::Magnetic,
    }
}

/// Teaching outlines multiply their base width by the resolved multiplier and
/// keep the historical fallback when the product is unusable.
fn styled_teaching_line_width(base: f32, multiplier: f32) -> f32 {
    let width = base * multiplier;
    if width.is_finite() && width > 0.0 {
        width
    } else {
        crate::reference::EDGE_WIDTH
    }
}

/// Teaching outlines keep their base alpha times the sanitized opacity and
/// take the fixed family colour when one is set.
fn styled_teaching_line_color(
    base: [f32; 4],
    opacity: f32,
    override_rgb: Option<crate::color_ramp::Rgb>,
) -> [f32; 4] {
    let [red, green, blue] = match override_rgb {
        Some(crate::color_ramp::Rgb([red, green, blue])) => [
            red as f32 / 255.0,
            green as f32 / 255.0,
            blue as f32 / 255.0,
        ],
        None => [base[0], base[1], base[2]],
    };
    [red, green, blue, (base[3] * opacity).clamp(0.0, 1.0)]
}

/// A readable message for a rejected material scene.
fn material_error_text(error: &MaterialError) -> String {
    match error {
        MaterialError::InvalidScene(message) | MaterialError::Unsupported(message) => {
            message.clone()
        }
    }
}

/// A readable message for a rejected Maxwell scene or sampling failure.
fn maxwell_error_text(error: &MaxwellError) -> String {
    match error {
        MaxwellError::InvalidScene(message)
        | MaxwellError::Unsupported(message)
        | MaxwellError::Unavailable(message) => message.clone(),
    }
}

/// The axis-aligned box between two corners, swapping per axis so `min`
/// really is the minimum (brief Step 2: "swap if violated").
pub fn domain_between(min: Vec3, max: Vec3) -> Domain {
    Domain::new(
        Vec3::new(min.x.min(max.x), min.y.min(max.y), min.z.min(max.z)),
        Vec3::new(min.x.max(max.x), min.y.max(max.y), min.z.max(max.z)),
    )
}

/// Clamp one domain extent into [`MIN_DOMAIN_EXTENT`]..=[`MAX_DOMAIN_EXTENT`].
///
/// This is the single bound the domain editor and its per-frame handoff use,
/// so the widgets and the seam cannot drift apart. Infinities clamp to the
/// nearer edge; NaN has no direction and becomes `0`. The call site keeps the
/// *previous* extent for NaN instead (a dragged box should not jump to zero),
/// but the pure function still promises a finite, in-range result.
pub fn clamp_domain_extent(value: f32) -> f32 {
    if value.is_nan() {
        0.0
    } else {
        value.clamp(MIN_DOMAIN_EXTENT, MAX_DOMAIN_EXTENT)
    }
}

/// Radians of orbit per point of left-drag in the 3D view.
pub const ORBIT_SENSITIVITY: f32 = 0.005;

/// One frame of viewport mouse input, in logical points and egui's zoom
/// convention (`zoom > 1` means the user asked to get closer). `drag` is the
/// orbit channel, `pan` the pan channel; [`drag_channel`] maps the held buttons
/// onto them (ticket 18), then each mode maps the channels onto its own
/// navigation (see [`apply_camera_input`]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ViewGesture {
    /// Orbit-channel drag delta, points (3D: orbit; 2D: pan).
    pub drag: Vec2,
    /// Pan-channel drag delta, points (pans both views).
    pub pan: Vec2,
    /// Wheel/pinch zoom factor; 1.0 is no change.
    pub zoom: f32,
    /// Viewport height in points, used to scale a pan like the picture
    /// under the pointer.
    pub viewport_height: f32,
}

impl Default for ViewGesture {
    fn default() -> Self {
        Self {
            drag: Vec2::ZERO,
            pan: Vec2::ZERO,
            zoom: 1.0,
            viewport_height: 1.0,
        }
    }
}

/// Pure camera action for one frame of mouse input.
///
/// 3D: a primary drag orbits ([`ORBIT_SENSITIVITY`] rad/point), a secondary or
/// middle drag pans the orbit target in the view plane, and the wheel/pinch
/// factor scales the orbit distance (clamped by the camera).
///
/// 2D: a drag pans the fixed front view along **world X/Y** (the view is
/// looking down -Z, so screen axes are world axes); the wheel/pinch factor
/// multiplies the ortho zoom (clamped). Panning moves the target exactly with
/// the pointer: one screen point of drag is one screen point of world motion at
/// the current visible height, so the scene sticks to the cursor.
///
/// Ticket 14 supersedes ticket 02's "2D ignores gestures" rule: the 2D view is
/// now navigable. Returns the new camera.
pub fn apply_camera_input(
    camera: OrbitCamera,
    mode_3d: bool,
    plane_size: f32,
    gesture: ViewGesture,
) -> OrbitCamera {
    let mut camera = camera;
    let zoom = if gesture.zoom.is_finite() && gesture.zoom > 0.0 {
        gesture.zoom
    } else {
        1.0
    };
    let height = if gesture.viewport_height.is_finite() && gesture.viewport_height > 0.0 {
        gesture.viewport_height
    } else {
        1.0
    };
    let plane_size = if plane_size.is_finite() && plane_size > 0.0 {
        plane_size
    } else {
        1.0
    };

    if mode_3d {
        camera.orbit(
            gesture.drag.x * ORBIT_SENSITIVITY,
            gesture.drag.y * ORBIT_SENSITIVITY,
        );
        // egui reports >1 for "closer"; the orbit camera's zoom is a distance
        // multiplier, so invert it.
        camera.zoom(1.0 / zoom);
        let world_per_point = 2.0 * camera.distance * (camera.fov_y * 0.5).tan() / height;
        camera.pan(
            -gesture.pan.x * world_per_point,
            gesture.pan.y * world_per_point,
        );
    } else {
        // The 2D view is a fixed front view (camera::OrbitCamera::front_view),
        // so its screen basis is world X (right) and world Y (up) regardless of
        // the orbit angles. A NaN zoom falls back to 1 before scaling the pan
        // (review M-3).
        let ortho_zoom = if camera.ortho_zoom.is_finite() && camera.ortho_zoom > 0.0 {
            camera.ortho_zoom
        } else {
            1.0
        };
        let world_per_point = (plane_size / ortho_zoom) / height;
        // Both buttons pan in 2D: a left drag is the 2D navigation gesture.
        let pan = gesture.drag + gesture.pan;
        camera.target.x -= pan.x * world_per_point;
        camera.target.y += pan.y * world_per_point;
        camera.zoom_2d(zoom);
    }
    camera
}

/// Which pointer buttons are dragging this frame, plus the Shift modifier.
/// Plain data so the button→gesture mapping is testable without a window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DragButtons {
    pub primary: bool,
    pub secondary: bool,
    pub middle: bool,
    pub shift: bool,
}

/// The camera channel a drag feeds. [`ViewGesture`] carries one delta per
/// channel, so the mapping decides which field gets the pointer delta.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DragChannel {
    /// Orbit the 3D view.
    Orbit,
    /// Pan the view (both modes).
    Pan,
}

/// Map the held pointer buttons onto a camera channel (ticket 18).
///
/// 3D: left-drag and right-drag orbit; middle-drag and Shift+right-drag pan
/// (the trackpad/no-middle-button fallback). 2D: every button pans — the fixed
/// front view has nothing to orbit. A middle button is the most explicit pan,
/// then the secondary button, then the primary one.
///
/// The wasm build (ticket 16) reuses this mapping and only has to suppress the
/// browser context menu on the secondary button.
pub fn drag_channel(mode_3d: bool, buttons: DragButtons) -> Option<DragChannel> {
    if buttons.middle {
        return Some(DragChannel::Pan);
    }
    if buttons.secondary {
        return Some(if mode_3d && !buttons.shift {
            DragChannel::Orbit
        } else {
            DragChannel::Pan
        });
    }
    if buttons.primary {
        return Some(if mode_3d {
            DragChannel::Orbit
        } else {
            DragChannel::Pan
        });
    }
    None
}

/// One frame of multi-touch pan input, points.
///
/// `active` says the touch gesture owns the camera this frame, which the caller
/// uses to suppress the pointer-drag orbit channel so the first finger cannot
/// orbit while two fingers pan. `pan` is the delta to add to the existing
/// [`ViewGesture::pan`] channel.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TouchPan {
    /// True while a two-finger (or more) gesture owns the camera this frame.
    pub active: bool,
    /// Pan-channel delta, points; zero when inactive or non-finite.
    pub pan: Vec2,
}

/// Map one frame of `egui::MultiTouchInfo` onto the existing 3D pan channel.
///
/// Two-finger pan is a 3D gesture: one finger keeps orbiting in 3D and panning
/// in 2D through the pointer path, so this is inactive for `mode_3d == false`
/// and for fewer than two touches. Pinch zoom does not pass through here —
/// `top_bar::wheel_zoom` already folds in `MultiTouchInfo::zoom_delta`.
///
/// A zero translation is an active gesture with a no-op pan; a non-finite
/// translation keeps the gesture active (so the pointer channel stays
/// suppressed) but contributes no camera motion.
pub fn two_finger_pan(mode_3d: bool, num_touches: usize, translation: Vec2) -> TouchPan {
    let active = mode_3d && num_touches >= 2;
    let pan = if active && translation.is_finite() {
        translation
    } else {
        Vec2::ZERO
    };
    TouchPan { active, pan }
}

/// What a completed pointer gesture in the viewport should do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClickOutcome {
    /// The gesture never became a drag and was a single click: seed the flow.
    Seed,
    /// A double-click: reset the view, never seed.
    ResetView,
    /// No click (a drag, a hover, or nothing).
    None,
}

/// Decide what one viewport gesture means. egui reports `clicked` for every
/// completed click including the second half of a double-click, so the
/// double-click check comes first: the reset wins and the click never seeds.
pub fn click_outcome(clicked: bool, double_clicked: bool, dragged: bool) -> ClickOutcome {
    if double_clicked {
        ClickOutcome::ResetView
    } else if clicked && !dragged {
        ClickOutcome::Seed
    } else {
        ClickOutcome::None
    }
}

/// What a captured (pointer-locked) viewport gesture does. Capture is
/// FPS-modal, so there is deliberately no reset variant: every click seeds at
/// the crosshair, and the second click of a double-click seeds again. A drag
/// never seeds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapturedClickOutcome {
    /// Seed at the viewport centre (NDC `(0, 0)`).
    SeedCentre,
    /// No click (a drag, a hover, or nothing).
    Ignore,
}

/// Decide a captured viewport gesture. Unlike [`click_outcome`], a double
/// click cannot become [`ClickOutcome::ResetView`]: reset belongs to orbit
/// mode, so a captured second click seeds again.
pub fn captured_click_outcome(clicked: bool, dragged: bool) -> CapturedClickOutcome {
    if clicked && !dragged {
        CapturedClickOutcome::SeedCentre
    } else {
        CapturedClickOutcome::Ignore
    }
}

/// How long after a single click a second click still counts as a double-click,
/// in seconds. Matches egui's default `max_double_click_delay`: the app uses it
/// to take back the first click's seed when a double-click resets the view.
pub const DOUBLE_CLICK_WINDOW: f64 = 0.3;

/// The seed the last single click planted, so a double-click can remove it
/// again: a double-click resets the view and must not seed the flow.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LastClickSeed {
    pub seed: Vec3,
    /// Particle count after any ambient spawn but before the click's burst.
    pub particles_before: usize,
    /// egui's frame time when the click happened.
    pub time: f64,
    /// Whether this click stored a streamline seed. An `AddParticles` click
    /// stores no seed but still records its burst, so a double-click truncates
    /// the particles without deleting a coincident retained seed.
    pub stored: bool,
}

/// What [`undo_click_seed`] did: the seed list after the undo, whether the
/// click's seed was removed, and the particle count to truncate back to when
/// the click's burst should go too.
#[derive(Debug, Clone, PartialEq)]
pub struct ClickSeedUndo {
    pub seeds: Vec<Vec3>,
    pub removed: bool,
    pub particles: Option<usize>,
}

/// Take back the burst the previous single click planted when it turns out to
/// be the first half of a double-click. Pure: the app owns the seed list, the
/// particle cloud and the recompose step.
///
/// A click older than [`DOUBLE_CLICK_WINDOW`] is not a double-click and is left
/// alone. Inside the window, for a stored click exactly the last occurrence of
/// its seed is removed and `particles` reports the pre-click count (when the
/// cloud still has at least that many points, so a truncation is safe).
///
/// An unstored click (`LastClickSeed::stored == false`, i.e. an `AddParticles`
/// click) removes nothing: it never stored a seed, so a double-click must not
/// delete a retained seed that happens to sit at the same point. Its particle
/// burst is still rolled back when safe.
pub fn undo_click_seed(
    seeds: &[Vec3],
    last: Option<LastClickSeed>,
    now: f64,
    particle_count: usize,
) -> ClickSeedUndo {
    let unchanged = |seeds: &[Vec3]| ClickSeedUndo {
        seeds: seeds.to_vec(),
        removed: false,
        particles: None,
    };
    let Some(last) = last else {
        return unchanged(seeds);
    };
    if !now.is_finite() || !last.time.is_finite() || now - last.time > DOUBLE_CLICK_WINDOW {
        return unchanged(seeds);
    }
    let rollback = if particle_count >= last.particles_before {
        Some(last.particles_before)
    } else {
        None
    };
    if !last.stored {
        return ClickSeedUndo {
            seeds: seeds.to_vec(),
            removed: false,
            particles: rollback,
        };
    }
    let mut seeds = seeds.to_vec();
    let Some(index) = seeds.iter().rposition(|seed| *seed == last.seed) else {
        return unchanged(&seeds);
    };
    seeds.remove(index);
    ClickSeedUndo {
        seeds,
        removed: true,
        particles: rollback,
    }
}
