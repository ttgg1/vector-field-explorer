//! Persisted application settings (ticket 14): the appearance record (ticket
//! 07), the layout record, and the view state that determines what the
//! viewport shows.
//!
//! One envelope, one storage key (`eframe::APP_KEY`), so style, layout and
//! view state can never drift apart across restarts. Compatibility matters:
//! ticket 07 stored a *bare* [`AppStyle`] under the same key, so [`load`] falls
//! back to that shape when the envelope decode fails. Absent or corrupt
//! storage always yields defaults (cosmetic/session state must never brick
//! startup; camera presets stay fail-fast for data integrity).
//!
//! Camera state is deliberately *not* persisted: it resets to the default on
//! launch and camera presets are the way to restore a viewpoint.

use serde::{Deserialize, Serialize};

use crate::arrow_style::{ArrowParams, ArrowStyle, ArrowStyle2D};
use crate::colormap::Colormap;
use crate::induction::{InductionCase, InductionScene};
use crate::material::{
    MaterialCase, MaterialError, MaterialScene, MaterialView, validate_material,
};
use crate::ode::{OdeChart, OdeScene, OdeSeed, OdeSource};
use crate::particles::{
    DEFAULT_PARTICLE_FIXED_COLOR, DEFAULT_PARTICLE_OPACITY, DEFAULT_STAGNATION_SECONDS,
    DEFAULT_TRAIL_FADE, DEFAULT_TRAIL_WIDTH, MAX_PARTICLES, ParticleAppearance, ParticleColorMode,
};
use crate::plane::PlaneAxis;
use crate::plot::PlotKind;
use crate::source_scene::{
    FieldMode, MAX_SOFTENING, MIN_SOFTENING, SourceDisplay, SourceRecord, SourceScene,
};
use crate::ui::layout::LayoutState;
use crate::ui::sidebar::{self, Controls};
use crate::ui::style_panel::AppStyle;
use crate::viewmodel::{ArrowLengthMode, LeftClickAction, State, StreamlineSeedMode};
use glam::{DVec3, Vec3};

/// Everything the app remembers across restarts. `style` is a required field
/// on purpose: a ticket-07 record (a bare `AppStyle`) must *fail* this decode
/// so [`load`] can detect and migrate it, rather than silently defaulting the
/// appearance. The other two records are defaulted for forward/backward
/// compatibility.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct AppSettings {
    pub style: AppStyle,
    #[serde(default)]
    pub layout: LayoutState,
    #[serde(default)]
    pub view: ViewRecord,
}

impl AppSettings {
    /// Clamp corrupt numbers from storage into usable ranges: style into the
    /// Settings slider bounds, layout widths into the draggable range.
    pub fn sanitized(mut self) -> Self {
        self.style = self.style.sanitized();
        self.layout = self.layout.sanitized();
        self
    }
}

/// The view state the Controls panel edits, in a serde-friendly shape (JSON/RON
/// arrays instead of glam types). These are the *effective* values from
/// [`State`], so a restored record reproduces exactly what was on screen.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ViewRecord {
    pub formula: String,
    pub mode_3d: bool,
    pub plot_kind: PlotKind,
    pub curve_source: String,
    pub surface_source: String,
    pub curve_samples: usize,
    pub surface_resolution: u32,
    pub plot_line_width: f32,
    pub plot_auto_height: bool,
    pub plot_height_min: f32,
    pub plot_height_max: f32,
    #[serde(default)]
    pub last_vector_mode_3d: Option<bool>,
    pub domain_min: [f32; 3],
    pub domain_max: [f32; 3],
    pub density: u32,
    pub arrow_scale: f32,
    /// Global arrow opacity (ticket 36). Older records lack the key; the serde
    /// default keeps them fully opaque.
    #[serde(default = "default_arrow_opacity")]
    pub arrow_opacity: f32,
    /// Plane-mode off-plane opacity (ticket 36). Missing on older records; the
    /// serde default is the historical `DIM_ALPHA`.
    #[serde(default = "default_plane_dim_opacity")]
    pub plane_dim_opacity: f32,
    /// Ticket-14's normalization flag. Only read: a record written before
    /// ticket 33 still carries a bare `normalize: true|false`, and
    /// [`ViewRecord::restore`] maps it onto [`ArrowLengthMode`]. New records
    /// never write the key, so it stays `None` and the old flat bool decodes
    /// through [`deserialize_legacy_normalize`] rather than `Option`'s RON
    /// `Some(...)` spelling.
    #[serde(
        default,
        skip_serializing,
        deserialize_with = "deserialize_legacy_normalize"
    )]
    pub normalize: Option<bool>,
    /// The three-way replacement for `normalize` (ticket 33). Missing on a
    /// legacy record; the container-level default then supplies Proportional.
    pub arrow_length_mode: ArrowLengthMode,
    /// World units per unit of magnitude, used only in Magnitude mode.
    /// Missing on a legacy record; the container-level default supplies 1.0.
    pub magnitude_scale: f32,
    /// The Proportional contrast exponent (ticket 57). Missing on records
    /// written before ticket 57; the serde default loads them as linear.
    #[serde(default = "default_proportional_exponent")]
    pub proportional_exponent: f32,
    pub colormap: Colormap,
    pub auto_fit: bool,
    pub color_min: f32,
    pub color_max: f32,
    /// Display the 2D lattice or 3D face grids and wireframe domain box.
    /// Missing in older records: reference geometry was always visible.
    #[serde(default = "default_reference_visible")]
    pub show_grid: bool,
    /// Display only the coloured X/Y (2D) or X/Y/Z (3D) axes.
    /// Independent of the grid and domain box.
    #[serde(default = "default_reference_visible")]
    pub show_coordinate_system: bool,
    /// Show arrows (ticket 45). Older records did not carry it, so the
    /// container default (true) keeps them showing arrows.
    #[serde(default = "default_arrows")]
    pub arrows: bool,
    /// The 3D arrow family (ticket 45).
    #[serde(default)]
    pub arrow_style: ArrowStyle,
    /// The 2D arrow family (ticket 61). Missing on pre-61 records; the serde
    /// default loads them as [`ArrowStyle2D::Flat`]. Old `Line` spellings also
    /// decode as Flat (ticket 64).
    #[serde(default)]
    pub arrow_style_2d: ArrowStyle2D,
    /// The 3D arrow head/shaft proportions (ticket 45).
    #[serde(default)]
    pub arrow_params: ArrowParams,
    pub streamlines: bool,
    /// Which seeds the streamline Feature paints (ticket 50). Older records
    /// did not carry it; the container default loads them as Auto.
    #[serde(default)]
    pub seed_mode: StreamlineSeedMode,
    /// The explicit streamline seed count; `0` is Auto. Missing on records
    /// written before the count existed; the serde default loads them as Auto.
    #[serde(default)]
    pub streamline_count: u32,
    /// What one explicit left click does (ticket 60). Older records did not
    /// carry it; they load as SeedStreamline. An unknown spelling also falls
    /// back to the default instead of failing the whole envelope.
    #[serde(default, deserialize_with = "deserialize_left_click_action")]
    pub left_click_action: LeftClickAction,
    pub particles: bool,
    /// The particle dot size multiplier (ticket 30). Missing on older
    /// records; the serde default loads them at the base fraction.
    #[serde(default = "default_particle_size")]
    pub particle_size: f32,
    /// The explicit particle count; `0` is Auto (ticket 30). Missing on older
    /// records; the serde default loads them as Auto.
    #[serde(default)]
    pub particle_count: u32,
    /// Particle appearance (ticket 48), stored flat so an older record can add
    /// each key independently. All eight default from the fresh-state record.
    #[serde(default = "default_particle_opacity")]
    pub particle_opacity: f32,
    #[serde(default)]
    pub particle_color_mode: ParticleColorMode,
    #[serde(default = "default_particle_fixed_color")]
    pub particle_fixed_color: [u8; 4],
    #[serde(default)]
    pub particle_trail_len: u32,
    #[serde(default = "default_particle_trail_fade")]
    pub particle_trail_fade: f32,
    #[serde(default = "default_particle_trail_width")]
    pub particle_trail_width: f32,
    #[serde(default = "default_particle_loop_on_stagnation")]
    pub particle_loop_on_stagnation: bool,
    #[serde(default = "default_particle_stagnation_seconds")]
    pub particle_stagnation_seconds: f32,
    /// Which adapter the viewport shows. A pre-#25 record has no key and opens
    /// in Formula mode with an empty source scene.
    #[serde(default)]
    pub field_mode: FieldMode,
    /// The durable source scene in list order. Each record is validated again
    /// on restore; malformed entries are skipped.
    #[serde(default, deserialize_with = "deserialize_source_entries")]
    pub source_entries: Vec<SourceRecord>,
    /// Which field families the source scene draws.
    #[serde(default)]
    pub source_display: SourceDisplay,
    /// Scene-wide softening. Out-of-range or non-finite values fall back to
    /// the default in [`ViewRecord::restore`].
    #[serde(default = "default_source_softening")]
    pub source_softening: f64,
    /// Whether accuracy-not-met estimates may be drawn. Only an exact `true`
    /// restores the opt-in; every other value becomes false.
    #[serde(default, deserialize_with = "deserialize_bool_or_false")]
    pub source_allow_inaccurate: bool,
    /// The durable induction case token: `current_loop` or `solenoid`. Missing
    /// on a legacy record; the default is the valid current-loop scene.
    #[serde(default = "default_induction_case")]
    pub induction_case: String,
    /// Induction conductor center (design §8). Validated on restore.
    #[serde(default = "default_induction_center")]
    pub induction_center: [f64; 3],
    /// Induction conductor axis; must be finite and unit length.
    #[serde(default = "default_induction_axis")]
    pub induction_axis: [f64; 3],
    #[serde(default = "default_induction_radius")]
    pub induction_radius: f64,
    #[serde(default)]
    pub induction_length: f64,
    #[serde(default = "default_induction_turns")]
    pub induction_turns: u32,
    #[serde(default = "default_induction_amplitude")]
    pub induction_amplitude: f64,
    #[serde(default = "default_induction_frequency")]
    pub induction_frequency: f64,
    #[serde(default)]
    pub induction_display: SourceDisplay,
    #[serde(default, deserialize_with = "deserialize_bool_or_false")]
    pub induction_allow_inaccurate: bool,
    /// Durable ODE source token: `equations` or `current_field`. Missing on a
    /// legacy record; the default is the typed harmonic scene.
    #[serde(default = "default_ode_source")]
    pub ode_source: String,
    /// ODE scene dimension (2 or 3). Validated together with the rest of the
    /// scene on restore; malformed values fall back to the default scene.
    #[serde(default = "default_ode_dimension")]
    pub ode_dimension: u8,
    /// The three durable typed right-hand-side slots; unused slots may be any
    /// string because validation parses only the first `ode_dimension`.
    #[serde(default = "default_ode_equations")]
    pub ode_equations: [String; 3],
    /// Initial conditions as plain `[x, y, z]` arrays so `glam` never appears
    /// in the persisted schema.
    #[serde(default = "default_ode_seeds")]
    pub ode_seeds: Vec<[f64; 3]>,
    /// The ODE scene duration, validated to `(0, 25]` on restore.
    #[serde(default = "default_ode_duration")]
    pub ode_duration: f64,
    /// ODE chart token: `phase` or `time_series`.
    #[serde(default = "default_ode_chart")]
    pub ode_chart: String,
    /// Time-series variable selection; at least one is restored on.
    #[serde(default = "default_ode_variables")]
    pub ode_variables: [bool; 3],
    /// Durable Materials case token. Missing on a legacy record; the default is
    /// the valid dielectric sphere.
    #[serde(default = "default_material_case")]
    pub material_case: String,
    #[serde(default = "default_material_background_epsilon_r")]
    pub material_background_epsilon_r: f64,
    #[serde(default = "default_material_background_mu_r")]
    pub material_background_mu_r: f64,
    #[serde(default = "default_material_material_epsilon_r")]
    pub material_material_epsilon_r: f64,
    #[serde(default = "default_material_material_mu_r")]
    pub material_material_mu_r: f64,
    #[serde(default = "default_material_radius")]
    pub material_radius: f64,
    /// `x`, `y` or `z`; validated on restore.
    #[serde(default = "default_material_interface_axis")]
    pub material_interface_axis: String,
    #[serde(default)]
    pub material_interface_offset: f64,
    /// Applied field as plain `[x, y, z]` so `glam` never appears in storage.
    #[serde(default = "default_material_applied")]
    pub material_applied: [f64; 3],
    /// `total`, `applied` or `response`.
    #[serde(default = "default_material_view")]
    pub material_view: String,
    /// Obsolete pre-ticket-26 trace speed. Read-only compatibility: old
    /// envelopes still carry the key, but the simulation clock owns speed now
    /// and streamlines use a fixed budget. New records never write it.
    #[serde(default, skip_serializing)]
    pub speed: f32,
}

impl Default for ViewRecord {
    fn default() -> Self {
        Self::capture(&State::new(false))
    }
}

impl ViewRecord {
    /// Snapshot the effective view state (the state's clamped/swapped values,
    /// not the widgets' in-flight mirror).
    pub fn capture(state: &State) -> Self {
        Self {
            formula: state.formula.clone(),
            mode_3d: state.mode_3d,
            plot_kind: state.plot.kind,
            curve_source: state.plot.curve.source.clone(),
            surface_source: state.plot.surface.source.clone(),
            curve_samples: state.plot.settings.curve_samples,
            surface_resolution: state.plot.settings.surface_resolution,
            plot_line_width: state.plot.settings.line_width,
            plot_auto_height: state.plot.settings.auto_height,
            plot_height_min: state.plot.settings.height_min,
            plot_height_max: state.plot.settings.height_max,
            last_vector_mode_3d: Some(if state.is_vector_mode() {
                state.mode_3d
            } else {
                state.plot.vector_mode_3d
            }),
            domain_min: state.domain.min.to_array(),
            domain_max: state.domain.max.to_array(),
            density: state.density,
            arrow_scale: state.arrow_scale,
            arrow_opacity: state.arrow_opacity,
            plane_dim_opacity: state.plane_dim_opacity,
            normalize: None,
            arrow_length_mode: state.arrow_length_mode,
            magnitude_scale: state.magnitude_scale,
            proportional_exponent: state.proportional_exponent,
            colormap: state.colormap,
            auto_fit: state.auto_fit,
            color_min: state.color_min,
            color_max: state.color_max,
            show_grid: state.show_grid,
            show_coordinate_system: state.show_coordinate_system,
            arrows: state.arrows,
            arrow_style: state.arrow_style,
            arrow_style_2d: state.arrow_style_2d,
            arrow_params: state.arrow_params,
            streamlines: state.streamlines,
            seed_mode: state.seed_mode,
            streamline_count: state.streamline_count,
            left_click_action: state.left_click_action,
            particles: state.particles,
            particle_size: state.particle_size,
            particle_count: state.particle_count,
            particle_opacity: state.particle_appearance.opacity,
            particle_color_mode: state.particle_appearance.color_mode,
            particle_fixed_color: state.particle_appearance.fixed_color,
            particle_trail_len: state.particle_appearance.trail_len,
            particle_trail_fade: state.particle_appearance.trail_fade,
            particle_trail_width: state.particle_appearance.trail_width,
            particle_loop_on_stagnation: state.particle_appearance.loop_on_stagnation,
            particle_stagnation_seconds: state.particle_appearance.stagnation_seconds,
            field_mode: state.field_mode,
            source_entries: state.sources.to_records(),
            source_display: state.sources.display,
            source_softening: state.sources.softening,
            source_allow_inaccurate: state.sources.allow_inaccurate,
            induction_case: induction_case_label(state.induction.case).to_string(),
            induction_center: state.induction.center.to_array(),
            induction_axis: state.induction.axis.to_array(),
            induction_radius: state.induction.radius,
            induction_length: state.induction.length,
            induction_turns: state.induction.turns,
            induction_amplitude: state.induction.amplitude,
            induction_frequency: state.induction.frequency,
            induction_display: state.induction.display,
            induction_allow_inaccurate: state.induction.allow_inaccurate,
            ode_source: ode_source_label(state.ode.source).to_string(),
            ode_dimension: state.ode.dimension as u8,
            ode_equations: state.ode.equations.clone(),
            ode_seeds: state.ode.seeds.iter().map(|seed| seed.position).collect(),
            ode_duration: state.ode.duration,
            ode_chart: ode_chart_label(state.ode_chart).to_string(),
            ode_variables: state.ode_variables,
            material_case: material_case_label(state.material.case).to_string(),
            material_background_epsilon_r: state.material.background_epsilon_r,
            material_background_mu_r: state.material.background_mu_r,
            material_material_epsilon_r: state.material.material_epsilon_r,
            material_material_mu_r: state.material.material_mu_r,
            material_radius: state.material.radius,
            material_interface_axis: material_axis_label(state.material.interface_axis).to_string(),
            material_interface_offset: state.material.interface_offset,
            material_applied: state.material.applied,
            material_view: material_view_label(state.material.view).to_string(),
            // Never written; kept at the serde default for old-record
            // compatibility only.
            speed: 0.0,
        }
    }

    /// Apply the record through the viewmodel setters (they own clamping,
    /// swapping and the dirty flag), then refresh the widget mirror with
    /// [`sidebar::apply`]. Non-finite domain components are left for that
    /// handoff — NaN keeps the previous effective extent and infinities clamp
    /// — while the other non-finite numbers fall back to defaults so a corrupt
    /// record cannot poison the sampling grid.
    pub fn restore(&self, state: &mut State, controls: &mut Controls) {
        let defaults = Controls::new();
        state.set_plot_kind(PlotKind::VectorField);
        state.set_mode(self.last_vector_mode_3d.unwrap_or(self.mode_3d));
        state.set_formula(self.formula.clone());

        controls.density = self.density;
        controls.arrow_scale = finite_or(self.arrow_scale, defaults.arrow_scale);
        controls.arrow_opacity = finite_or(self.arrow_opacity, defaults.arrow_opacity);
        controls.plane_dim_opacity = finite_or(self.plane_dim_opacity, defaults.plane_dim_opacity);
        // Legacy records carried a `normalize` bool instead of a mode; it wins
        // when present so old settings land in the matching mode, and new
        // records (where it is absent) use the stored three-way mode.
        controls.arrow_length_mode = self
            .normalize
            .map(ArrowLengthMode::from_normalize)
            .unwrap_or(self.arrow_length_mode);
        controls.magnitude_scale = finite_or(self.magnitude_scale, defaults.magnitude_scale);
        controls.proportional_exponent =
            finite_or(self.proportional_exponent, defaults.proportional_exponent);
        controls.colormap = self.colormap;
        controls.auto_fit = self.auto_fit;
        controls.color_min = finite_or(self.color_min, defaults.color_min);
        controls.color_max = finite_or(self.color_max, defaults.color_max);
        controls.show_grid = self.show_grid;
        controls.show_coordinate_system = self.show_coordinate_system;
        controls.arrows = self.arrows;
        controls.arrow_style = self.arrow_style;
        controls.arrow_style_2d = self.arrow_style_2d;
        controls.arrow_params = self.arrow_params.sanitized();
        controls.streamlines = self.streamlines;
        controls.seed_mode = self.seed_mode;
        controls.streamline_count = self.streamline_count;
        controls.left_click_action = self.left_click_action;
        controls.particles = self.particles;
        controls.particle_size = finite_or(self.particle_size, defaults.particle_size);
        controls.particle_count = self.particle_count.min(MAX_PARTICLES as u32);
        controls.particle_appearance = ParticleAppearance {
            opacity: finite_or(self.particle_opacity, defaults.particle_appearance.opacity),
            color_mode: self.particle_color_mode,
            fixed_color: self.particle_fixed_color,
            trail_len: self.particle_trail_len,
            trail_fade: finite_or(
                self.particle_trail_fade,
                defaults.particle_appearance.trail_fade,
            ),
            trail_width: finite_or(
                self.particle_trail_width,
                defaults.particle_appearance.trail_width,
            ),
            loop_on_stagnation: self.particle_loop_on_stagnation,
            stagnation_seconds: finite_or(
                self.particle_stagnation_seconds,
                defaults.particle_appearance.stagnation_seconds,
            ),
        };
        // `self.speed` is read-only legacy data and deliberately ignored: the
        // transport clock owns speed and streamlines use a fixed budget.
        // Raw components: `sidebar::apply` is the one place that bounds the
        // extents, swaps a reversed pair and keeps the previous value for NaN.
        controls.domain_min = Vec3::from_array(self.domain_min);
        controls.domain_max = Vec3::from_array(self.domain_max);
        sidebar::apply(state, controls);
        state.set_plot_samples(self.curve_samples);
        state.set_plot_resolution(self.surface_resolution);
        state.set_plot_width(self.plot_line_width);
        state.set_plot_auto_height(self.plot_auto_height);
        state.set_plot_height_range(self.plot_height_min, self.plot_height_max);
        state.restore_plot_drafts(self.curve_source.clone(), self.surface_source.clone());
        state.set_plot_kind(self.plot_kind);

        // Sources restore last so a legacy formula above is never overwritten
        // by opening the saved scene. Every record is revalidated; a bad one is
        // skipped rather than failing the envelope.
        let mut scene = SourceScene::from_records(&self.source_entries);
        let softening = if self.source_softening.is_finite()
            && (MIN_SOFTENING..=MAX_SOFTENING).contains(&self.source_softening)
        {
            self.source_softening
        } else {
            default_source_softening()
        };
        let _ = scene.set_softening(softening);
        scene.set_display(self.source_display);
        scene.set_allow_inaccurate(self.source_allow_inaccurate);
        let valid_entries = scene.entries.len();
        let requested_sources = self.field_mode == FieldMode::Sources;
        let requested_induction = self.field_mode == FieldMode::Induction;
        let requested_ode = self.field_mode == FieldMode::Ode;
        let requested_materials = self.field_mode == FieldMode::Materials;
        let material_valid = self.restored_material_scene().is_ok();
        let restored_induction = self.restored_induction_scene();
        let induction_valid = restored_induction.is_ok();
        state.set_source_scene(scene);
        // ODE restores lazily: validate through the normal prepare path, store
        // the durable scene/chart/variables, clear any stale solution and mark
        // the first Ode frame pending. Malformed values keep the explicitly
        // stored Ode mode but use the valid default scene and report the error.
        let ode_chart = restored_ode_chart(&self.ode_chart);
        let ode_variables = if self.ode_variables.iter().any(|enabled| *enabled) {
            self.ode_variables
        } else {
            [true, true, false]
        };
        match self.restored_ode_scene() {
            Ok(scene) => {
                let _ = state.restore_ode_fields(scene, ode_chart, ode_variables);
            }
            Err(error) => {
                let _ = state.restore_ode_fields(OdeScene::default(), ode_chart, ode_variables);
                state.ode_error = Some(error);
            }
        }
        // A saved Sources mode without any usable entry cannot draw anything;
        // an invalid saved induction scene cannot either. Both fall back to
        // Formula instead of presenting an empty field. Ode stays selected when
        // it was stored explicitly; its lazy solve owns the error state.
        // Materials is selected only from an explicitly stored mode and a
        // valid record, so a malformed payload can never promote the mode.
        self.restore_material_fields(state);
        state.set_field_mode(if requested_sources && valid_entries > 0 {
            FieldMode::Sources
        } else if requested_induction && induction_valid {
            FieldMode::Induction
        } else if requested_ode {
            FieldMode::Ode
        } else if requested_materials && material_valid {
            FieldMode::Materials
        } else {
            FieldMode::Formula
        });
        state.source_sampling = None;
        if valid_entries != self.source_entries.len() {
            state.source_notice = Some("Some saved sources were invalid and were skipped".into());
        }
        // The induction scene restores after the mode so its validation result
        // owns `induction_error`; a bad payload falls back to the valid default
        // scene and never fails startup.
        match restored_induction {
            Ok(scene) => {
                let _ = state.set_induction_scene(scene);
                state.induction_prepared = None;
                state.induction_sampling = None;
            }
            Err(error) => {
                state.induction = InductionScene::default();
                state.induction_prepared = None;
                state.induction_sampling = None;
                state.induction_error = Some(error);
            }
        }
        state.mark_dirty();
    }

    /// Restore the durable Materials scene from the record without sampling.
    ///
    /// Validation is the same [`validate_material`] gate the editor uses. A
    /// malformed payload always falls back to the default dielectric sphere;
    /// only an explicitly stored Materials mode surfaces the error, so an old
    /// or damaged record can never promote Materials. The returned `Some` is
    /// the stored error for the caller/tests, already mirrored on the state.
    pub fn restore_material_fields(&self, state: &mut State) -> Option<String> {
        let requested_materials = self.field_mode == FieldMode::Materials;
        match self.restored_material_scene() {
            Ok(scene) => {
                state.material = scene;
                state.material_error = None;
            }
            Err(error) => {
                state.material = MaterialScene::default();
                state.material_error = requested_materials.then_some(error);
            }
        }
        state.material_sampling = None;
        state.material_error.clone()
    }

    /// Rebuild the persisted Materials scene. Every field is serde-defaulted,
    /// so an old payload yields the valid default scene; unknown case/view/axis
    /// tokens and invalid numbers are reported instead of silently promoting
    /// the mode.
    fn restored_material_scene(&self) -> Result<MaterialScene, String> {
        let case = match self.material_case.as_str() {
            "dielectric_sphere" => MaterialCase::DielectricSphere,
            "dielectric_boundary" => MaterialCase::DielectricBoundary,
            "permeable_sphere" => MaterialCase::PermeableSphere,
            "permeable_boundary" => MaterialCase::PermeableBoundary,
            other => return Err(format!("material case is not recognized: {other}")),
        };
        let view = match self.material_view.as_str() {
            "total" => MaterialView::Total,
            "applied" => MaterialView::Applied,
            "response" => MaterialView::Response,
            other => return Err(format!("material view is not recognized: {other}")),
        };
        let interface_axis = match self.material_interface_axis.as_str() {
            "x" => PlaneAxis::X,
            "y" => PlaneAxis::Y,
            "z" => PlaneAxis::Z,
            other => {
                return Err(format!(
                    "material interface axis is not recognized: {other}"
                ));
            }
        };
        let scene = MaterialScene {
            case,
            background_epsilon_r: self.material_background_epsilon_r,
            background_mu_r: self.material_background_mu_r,
            material_epsilon_r: self.material_material_epsilon_r,
            material_mu_r: self.material_material_mu_r,
            radius: self.material_radius,
            interface_axis,
            interface_offset: self.material_interface_offset,
            applied: self.material_applied,
            view,
        };
        validate_material(&scene)
            .map_err(|error| format!("material scene: {}", material_validation_text(&error)))?;
        Ok(scene)
    }

    /// Rebuild the persisted induction scene. Every field is serde-defaulted,
    /// so an old payload yields the valid default scene; unknown case tokens
    /// and invalid numbers are reported instead of silently poisoning the
    /// sampler.
    fn restored_induction_scene(&self) -> Result<InductionScene, String> {
        let case = match self.induction_case.as_str() {
            "current_loop" => InductionCase::CurrentLoop,
            "solenoid" => InductionCase::Solenoid,
            other => return Err(format!("induction case is not recognized: {other}")),
        };
        let scene = InductionScene {
            case,
            center: DVec3::from_array(self.induction_center),
            axis: DVec3::from_array(self.induction_axis),
            radius: self.induction_radius,
            length: self.induction_length,
            turns: self.induction_turns,
            amplitude: self.induction_amplitude,
            frequency: self.induction_frequency,
            display: self.induction_display,
            allow_inaccurate: self.induction_allow_inaccurate,
        };
        scene.validate()?;
        Ok(scene)
    }

    /// Rebuild the persisted ODE scene. Every field is serde-defaulted, so an
    /// old payload yields the valid default scene. Unknown source tokens are
    /// reported; all numeric/equation validity is decided by
    /// [`State::restore_ode_fields`] through the normal prepare path.
    fn restored_ode_scene(&self) -> Result<OdeScene, String> {
        let source = match self.ode_source.as_str() {
            "equations" => OdeSource::Equations,
            "current_field" => OdeSource::CurrentField,
            other => return Err(format!("ODE source is not recognized: {other}")),
        };
        Ok(OdeScene {
            source,
            dimension: usize::from(self.ode_dimension),
            equations: self.ode_equations.clone(),
            seeds: self
                .ode_seeds
                .iter()
                .map(|position| OdeSeed {
                    position: *position,
                })
                .collect(),
            duration: self.ode_duration,
        })
    }
}

/// Map the persisted chart token; unknown tokens restore the phase portrait.
fn restored_ode_chart(token: &str) -> OdeChart {
    match token {
        "time_series" => OdeChart::TimeSeries,
        _ => OdeChart::Phase,
    }
}

fn ode_chart_label(chart: OdeChart) -> &'static str {
    match chart {
        OdeChart::Phase => "phase",
        OdeChart::TimeSeries => "time_series",
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
        MaterialView::Total => "total",
        MaterialView::Applied => "applied",
        MaterialView::Response => "response",
    }
}

fn material_axis_label(axis: PlaneAxis) -> &'static str {
    match axis {
        PlaneAxis::X => "x",
        PlaneAxis::Y => "y",
        PlaneAxis::Z => "z",
    }
}

fn material_validation_text(error: &MaterialError) -> String {
    match error {
        MaterialError::InvalidScene(message) | MaterialError::Unsupported(message) => {
            message.clone()
        }
    }
}

fn default_material_case() -> String {
    material_case_label(MaterialScene::default().case).to_string()
}

fn default_material_background_epsilon_r() -> f64 {
    MaterialScene::default().background_epsilon_r
}

fn default_material_background_mu_r() -> f64 {
    MaterialScene::default().background_mu_r
}

fn default_material_material_epsilon_r() -> f64 {
    MaterialScene::default().material_epsilon_r
}

fn default_material_material_mu_r() -> f64 {
    MaterialScene::default().material_mu_r
}

fn default_material_radius() -> f64 {
    MaterialScene::default().radius
}

fn default_material_interface_axis() -> String {
    material_axis_label(MaterialScene::default().interface_axis).to_string()
}

fn default_material_applied() -> [f64; 3] {
    MaterialScene::default().applied
}

fn default_material_view() -> String {
    material_view_label(MaterialScene::default().view).to_string()
}

fn default_ode_source() -> String {
    ode_source_label(OdeScene::default().source).to_string()
}

fn default_ode_dimension() -> u8 {
    OdeScene::default().dimension as u8
}

fn default_ode_equations() -> [String; 3] {
    OdeScene::default().equations
}

fn default_ode_seeds() -> Vec<[f64; 3]> {
    OdeScene::default()
        .seeds
        .into_iter()
        .map(|seed| seed.position)
        .collect()
}

fn default_ode_duration() -> f64 {
    OdeScene::default().duration
}

fn default_ode_chart() -> String {
    ode_chart_label(OdeChart::default()).to_string()
}

fn default_ode_variables() -> [bool; 3] {
    [true, true, false]
}

/// `value` when finite, `fallback` otherwise.
pub fn finite_or(value: f32, fallback: f32) -> f32 {
    if value.is_finite() { value } else { fallback }
}

/// The `arrows` field's serde default: older records predate the Show arrows
/// toggle, and arrows were always drawn then.
fn default_arrows() -> bool {
    true
}

/// Older records omit both reference toggles; each missing field stays visible
/// independently (a plain bool serde default would instead hide it).
fn default_reference_visible() -> bool {
    true
}

/// The `arrow_opacity` serde default (ticket 36): records written before the
/// control existed were fully opaque.
fn default_arrow_opacity() -> f32 {
    crate::viewmodel::MAX_ARROW_OPACITY
}

/// The `plane_dim_opacity` serde default (ticket 36): the historical off-plane
/// alpha, reused directly so the two cannot drift.
fn default_plane_dim_opacity() -> f32 {
    crate::plane::DIM_ALPHA
}

/// The `proportional_exponent` serde default (ticket 57): records written
/// before the contrast existed used the linear mapping.
fn default_proportional_exponent() -> f32 {
    crate::viewmodel::DEFAULT_PROPORTIONAL_EXPONENT
}

/// The `source_softening` serde default: the evaluator's historical `0.1`.
fn default_source_softening() -> f64 {
    0.1
}

/// The induction scene defaults (design §8): a valid, unsampled current loop.
fn default_induction_case() -> String {
    "current_loop".to_string()
}

fn default_induction_center() -> [f64; 3] {
    [0.0, 0.0, 0.0]
}

fn default_induction_axis() -> [f64; 3] {
    [0.0, 0.0, 1.0]
}

fn default_induction_radius() -> f64 {
    1.0
}

fn default_induction_turns() -> u32 {
    1
}

fn default_induction_amplitude() -> f64 {
    1.0
}

fn default_induction_frequency() -> f64 {
    0.25
}

fn induction_case_label(case: InductionCase) -> &'static str {
    match case {
        InductionCase::CurrentLoop => "current_loop",
        InductionCase::Solenoid => "solenoid",
    }
}

/// Decode the source list element by element so one wrong-typed or corrupt
/// record cannot fail the whole settings envelope. A skipped element becomes
/// the empty default record, which [`SourceRecord::to_source`] rejects; the
/// restore path still sees the original length and can raise its notice.
fn deserialize_source_entries<'de, D>(deserializer: D) -> Result<Vec<SourceRecord>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(serde::Deserialize)]
    #[serde(untagged)]
    enum LenientRecord {
        Record(SourceRecord),
        Invalid(serde::de::IgnoredAny),
    }

    let records = Vec::<LenientRecord>::deserialize(deserializer)?;
    Ok(records
        .into_iter()
        .map(|record| match record {
            LenientRecord::Record(record) => record,
            LenientRecord::Invalid(_) => SourceRecord::default(),
        })
        .collect())
}

/// Decode an accuracy opt-in leniently: only a literal `true` enables it, and
/// every other value (wrong type, corrupt, missing) becomes false without
/// failing the whole settings envelope.
fn deserialize_bool_or_false<'de, D>(deserializer: D) -> Result<bool, D::Error>
where
    D: serde::Deserializer<'de>,
{
    /// Matches a bool, or swallows any other value as `false`.
    #[derive(serde::Deserialize)]
    #[serde(untagged)]
    enum BoolOrFalse {
        Bool(bool),
        Other(serde::de::IgnoredAny),
    }

    Ok(match BoolOrFalse::deserialize(deserializer)? {
        BoolOrFalse::Bool(value) => value,
        BoolOrFalse::Other(_) => false,
    })
}

/// The `particle_size` serde default (ticket 30): records written before the
/// size control used the base grid-spacing fraction.
fn default_particle_size() -> f32 {
    crate::viewmodel::DEFAULT_PARTICLE_SIZE
}

fn default_particle_opacity() -> f32 {
    DEFAULT_PARTICLE_OPACITY
}

fn default_particle_fixed_color() -> [u8; 4] {
    DEFAULT_PARTICLE_FIXED_COLOR
}

fn default_particle_trail_fade() -> f32 {
    DEFAULT_TRAIL_FADE
}

fn default_particle_trail_width() -> f32 {
    DEFAULT_TRAIL_WIDTH
}

fn default_particle_loop_on_stagnation() -> bool {
    true
}

fn default_particle_stagnation_seconds() -> f32 {
    DEFAULT_STAGNATION_SECONDS
}

/// Decode a ticket-60 `left_click_action`, falling back to the default for an
/// unknown variant instead of failing the whole settings envelope.
///
/// The derived enum decoder rejects an unknown unit variant; a corrupt or
/// future record must still load every other setting. RON gives the variant
/// name through the enum access; matching it by hand keeps the fallback local.
fn deserialize_left_click_action<'de, D>(deserializer: D) -> Result<LeftClickAction, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::{EnumAccess, VariantAccess, Visitor};

    struct VariantName(String);

    impl<'de> serde::Deserialize<'de> for VariantName {
        fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where
            D: serde::Deserializer<'de>,
        {
            struct NameVisitor;

            impl<'de> Visitor<'de> for NameVisitor {
                type Value = VariantName;

                fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                    formatter.write_str("a left-click action name")
                }

                fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
                where
                    E: serde::de::Error,
                {
                    Ok(VariantName(value.to_owned()))
                }
            }

            deserializer.deserialize_identifier(NameVisitor)
        }
    }

    struct ActionVisitor;

    impl<'de> Visitor<'de> for ActionVisitor {
        type Value = LeftClickAction;

        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("a left-click action")
        }

        fn visit_enum<A>(self, data: A) -> Result<Self::Value, A::Error>
        where
            A: EnumAccess<'de>,
        {
            let (name, variant) = data.variant::<VariantName>()?;
            variant.unit_variant()?;
            Ok(match name.0.as_str() {
                "AddParticles" => LeftClickAction::AddParticles,
                _ => LeftClickAction::SeedStreamline,
            })
        }
    }

    deserializer.deserialize_enum(
        "LeftClickAction",
        &["SeedStreamline", "AddParticles"],
        ActionVisitor,
    )
}

/// Decode the ticket-14 `normalize` boolean, which was stored as a bare
/// `bool`, into the `Option` the legacy-aware [`ViewRecord`] carries. Ticket-33
/// records omit the key entirely, so this only runs for old settings.
fn deserialize_legacy_normalize<'de, D>(deserializer: D) -> Result<Option<bool>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    bool::deserialize(deserializer).map(Some)
}

/// Load the settings envelope, migrating a ticket-07 bare `AppStyle` record and
/// falling back to defaults for absent or corrupt storage. Never panics.
pub fn load(storage: Option<&dyn eframe::Storage>) -> AppSettings {
    let Some(storage) = storage else {
        return AppSettings::default();
    };
    if let Some(settings) = eframe::get_value::<AppSettings>(storage, eframe::APP_KEY) {
        return settings.sanitized();
    }
    if let Some(style) = eframe::get_value::<AppStyle>(storage, eframe::APP_KEY) {
        log::warn!("persisted settings use the ticket-07 shape; migrating appearance only");
        return AppSettings {
            style,
            ..AppSettings::default()
        };
    }
    if storage.get_string(eframe::APP_KEY).is_some() {
        log::warn!("persisted settings are corrupt; falling back to defaults");
    }
    AppSettings::default()
}

/// Persist the settings envelope under eframe's app key.
pub fn save(storage: &mut dyn eframe::Storage, settings: &AppSettings) {
    eframe::set_value(storage, eframe::APP_KEY, settings);
}
