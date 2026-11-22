//! Scalar editing and lazy sampling, independent of the vector snapshot.
use std::sync::Arc;

use glam::UVec2;

use super::State;
use crate::{
    field::normalize_formula_input,
    parameters::ParameterSet,
    plot::{PlotKind, curve_edges_at, sample_curve_at, sample_surface_at},
    plot_examples::PlotExample,
    plot_scene::{PlotScene, build_plot_scene},
    plot_state::{PlotSettings, SampledPlot, ScalarDraft},
    scalar::{ScalarContext, ScalarExpr},
    scalar_view::scalar_view_expr,
};

/// Parse a scalar draft through the shipped parser for its context.
///
/// A surface draft is parsed by [`scalar_view_expr`], the same
/// parameter-substituting surface parser the scalar overlay uses. A curve draft
/// keeps its curve-only binding because that parser surface accepts `y` as an
/// independent variable.
fn parse_scalar_source(
    source: &str,
    context: ScalarContext,
    parameters: &ParameterSet,
) -> Result<ScalarExpr, String> {
    match context {
        ScalarContext::Surface => {
            scalar_view_expr(source, parameters).map_err(|error| error.to_string())
        }
        ScalarContext::Curve => {
            ScalarExpr::parse(source, context).map_err(|error| error.to_string())
        }
    }
}

impl State {
    fn scalar_draft(&self, context: ScalarContext) -> &ScalarDraft {
        match context {
            ScalarContext::Curve => &self.plot.curve,
            ScalarContext::Surface => &self.plot.surface,
        }
    }

    fn scalar_draft_mut(&mut self, context: ScalarContext) -> &mut ScalarDraft {
        match context {
            ScalarContext::Curve => &mut self.plot.curve,
            ScalarContext::Surface => &mut self.plot.surface,
        }
    }

    fn plot_changed(&mut self) -> bool {
        self.plot.generation += 1;
        self.mark_dirty()
    }

    pub fn is_vector_mode(&self) -> bool {
        self.plot.kind == PlotKind::VectorField
    }

    pub fn set_plot_kind(&mut self, kind: PlotKind) -> bool {
        if self.plot.kind == kind {
            return false;
        }
        let previous_grid = self.plane_grid();
        if self.is_vector_mode() {
            self.plot.vector_mode_3d = self.mode_3d;
        }
        self.plot.kind = kind;
        self.plot.transition_epoch += 1;
        self.mode_3d = match kind {
            PlotKind::VectorField => self.plot.vector_mode_3d,
            PlotKind::Curve2D => false,
            PlotKind::Surface3D => true,
        };
        self.sync_plane(previous_grid);
        let context = match kind {
            PlotKind::VectorField => None,
            PlotKind::Curve2D => Some(ScalarContext::Curve),
            PlotKind::Surface3D => Some(ScalarContext::Surface),
        };
        if let Some(context) = context {
            let source = self.scalar_draft(context).source.clone();
            let parsed = parse_scalar_source(&source, context, &self.parameters);
            let draft = self.scalar_draft_mut(context);
            draft.accepted = None;
            draft.samples = None;
            draft.sampled_at = None;
            match parsed {
                Ok(expr) => {
                    draft.accepted = Some(expr);
                    draft.error = None;
                }
                Err(error) => draft.error = Some(error),
            }
        }
        self.plot_changed()
    }

    pub fn set_scalar_source(&mut self, context: ScalarContext, source: String) -> bool {
        self.update_scalar_source(context, source, false)
    }

    fn update_scalar_source(
        &mut self,
        context: ScalarContext,
        source: String,
        force: bool,
    ) -> bool {
        let source = normalize_formula_input(&source);
        if !force && self.scalar_draft(context).source == source {
            return false;
        }
        let parsed = parse_scalar_source(&source, context, &self.parameters);
        let draft = self.scalar_draft_mut(context);
        draft.source = source;
        match parsed {
            Ok(expr) => {
                draft.accepted = Some(expr);
                draft.samples = None;
                draft.sampled_at = None;
                draft.error = None;
            }
            Err(error) => draft.error = Some(error),
        }
        self.plot_changed()
    }

    /// Only the active surface needs a refresh: entering Surface reparses the
    /// saved draft, and curve expressions do not bind named parameters.
    pub(super) fn surface_bindings_changed(&self, next: &ParameterSet) -> bool {
        self.plot.kind == PlotKind::Surface3D
            && (self.parameters.entries.len() != next.entries.len()
                || self.parameters.entries.iter().any(|entry| {
                    next.value(&entry.name).map(f64::to_bits) != Some(entry.value.to_bits())
                }))
    }

    pub(super) fn rebind_surface_parameters(&mut self) {
        let source = self.plot.surface.source.clone();
        self.update_scalar_source(ScalarContext::Surface, source, true);
    }

    /// Validate and atomically install a named-parameter set together with the
    /// surface expression that consumes it.
    ///
    /// Both the parameter set and the source are checked through the shipped
    /// parameter-aware surface parser before either the committed parameters,
    /// the draft source, the accepted expression or the sampled mesh changes.
    /// On error nothing changes, so the caller can report success only after
    /// this returns `Ok`.
    pub fn apply_parameterized_surface(
        &mut self,
        parameters: ParameterSet,
        source: String,
    ) -> Result<(), String> {
        parameters.validate()?;
        let source = normalize_formula_input(&source);
        let expr = parse_scalar_source(&source, ScalarContext::Surface, &parameters)?;
        // Both inputs already validated, so the parameter commit cannot reject
        // them; only after it succeeds is the draft replaced.
        // The old draft need not be rebound; the validated replacement below
        // invalidates the scene exactly once.
        self.commit_parameters_inner(parameters, false)?;
        let draft = &mut self.plot.surface;
        draft.source = source;
        draft.accepted = Some(expr);
        draft.samples = None;
        draft.sampled_at = None;
        draft.error = None;
        self.plot_changed();
        Ok(())
    }

    pub fn set_active_source(&mut self, source: String) -> bool {
        match self.plot.kind {
            PlotKind::Curve2D => self.set_scalar_source(ScalarContext::Curve, source),
            PlotKind::Surface3D => self.set_scalar_source(ScalarContext::Surface, source),
            PlotKind::VectorField => self.set_formula(source),
        }
    }

    pub fn active_source(&self) -> &str {
        match self.plot.kind {
            PlotKind::Curve2D => &self.plot.curve.source,
            PlotKind::Surface3D => &self.plot.surface.source,
            PlotKind::VectorField => &self.formula,
        }
    }

    pub fn set_plot_samples(&mut self, count: usize) -> bool {
        let count = count.clamp(2, 4097);
        if self.plot.settings.curve_samples == count {
            return false;
        }
        self.plot.settings.curve_samples = count;
        self.plot.curve.samples = None;
        self.plot.curve.sampled_at = None;
        self.plot_changed()
    }
    pub fn set_plot_resolution(&mut self, resolution: u32) -> bool {
        let resolution = resolution.clamp(2, 96);
        if self.plot.settings.surface_resolution == resolution {
            return false;
        }
        self.plot.settings.surface_resolution = resolution;
        self.plot.surface.samples = None;
        self.plot.surface.sampled_at = None;
        self.plot_changed()
    }
    pub fn set_plot_width(&mut self, width: f32) -> bool {
        let width = if width.is_finite() && width > 0.0 {
            width.clamp(0.001, 0.2)
        } else {
            0.01
        };
        if self.plot.settings.line_width == width {
            return false;
        }
        self.plot.settings.line_width = width;
        self.plot_changed()
    }
    pub fn set_plot_auto_height(&mut self, auto: bool) -> bool {
        if self.plot.settings.auto_height == auto {
            return false;
        }
        self.plot.settings.auto_height = auto;
        self.plot_changed()
    }
    pub fn set_plot_height_range(&mut self, min: f32, max: f32) -> bool {
        let (min, max) = if min.is_finite() && max.is_finite() {
            (min.min(max), min.max(max))
        } else {
            (0.0, 1.0)
        };
        if self.plot.settings.height_min == min && self.plot.settings.height_max == max {
            return false;
        }
        self.plot.settings.height_min = min;
        self.plot.settings.height_max = max;
        self.plot_changed()
    }
    pub fn select_plot_example(&mut self, example: &PlotExample) -> bool {
        let changed = self.set_plot_kind(example.kind);
        let edited = self.set_active_source(example.source.into());
        if !changed {
            self.plot.transition_epoch += 1;
        }
        changed || edited
    }
    pub fn reset_plot_section(&mut self) -> bool {
        let defaults = crate::plot_state::PlotState::default();
        let source = match self.plot.kind {
            PlotKind::Curve2D => Some(defaults.curve.source),
            PlotKind::Surface3D => Some(defaults.surface.source),
            PlotKind::VectorField => None,
        };
        let mut changed = source.is_some_and(|s| self.set_active_source(s));
        let settings = PlotSettings::default();
        changed |= self.set_plot_samples(settings.curve_samples);
        changed |= self.set_plot_resolution(settings.surface_resolution);
        changed |= self.set_plot_width(settings.line_width);
        changed |= self.set_plot_auto_height(settings.auto_height);
        changed |= self.set_plot_height_range(settings.height_min, settings.height_max);
        changed
    }
    pub fn restore_plot_drafts(&mut self, curve: String, surface: String) {
        for context in [ScalarContext::Curve, ScalarContext::Surface] {
            let draft = self.scalar_draft_mut(context);
            draft.accepted = None;
            draft.samples = None;
            draft.sampled_at = None;
        }
        self.update_scalar_source(ScalarContext::Curve, curve, true);
        self.update_scalar_source(ScalarContext::Surface, surface, true);
    }
    /// Whether the active scalar plot's accepted expression reads time.
    ///
    /// Deliberately ignores `error`: while a previously time-dependent source
    /// is mid-edit and invalid, the plot keeps its clock consumer visible so
    /// the clock can run while the retained mesh stays frozen.
    pub(super) fn active_scalar_is_time_dependent(&self) -> bool {
        match self.plot.kind {
            PlotKind::Curve2D => self
                .plot
                .curve
                .accepted
                .as_ref()
                .is_some_and(ScalarExpr::is_time_dependent),
            PlotKind::Surface3D => self
                .plot
                .surface
                .accepted
                .as_ref()
                .is_some_and(ScalarExpr::is_time_dependent),
            PlotKind::VectorField => false,
        }
    }

    /// Invalidate the active scalar plot after a time change, but only when its
    /// current edit is valid and the accepted expression is time-dependent.
    /// Returns whether generation advanced.
    pub(super) fn scalar_time_changed(&mut self) -> bool {
        let context = match self.plot.kind {
            PlotKind::Curve2D => Some(ScalarContext::Curve),
            PlotKind::Surface3D => Some(ScalarContext::Surface),
            PlotKind::VectorField => None,
        };
        let Some(context) = context else {
            return false;
        };
        let draft = self.scalar_draft(context);
        if draft.error.is_some()
            || !draft
                .accepted
                .as_ref()
                .is_some_and(ScalarExpr::is_time_dependent)
        {
            return false;
        }
        self.plot.generation += 1;
        self.mark_dirty()
    }

    pub fn plot_scene(&mut self) -> Arc<PlotScene> {
        let context = match self.plot.kind {
            PlotKind::Curve2D => Some(ScalarContext::Curve),
            PlotKind::Surface3D => Some(ScalarContext::Surface),
            PlotKind::VectorField => None,
        };
        if let Some(context) = context {
            let draft = self.scalar_draft(context);
            let needs_sample = draft.samples.is_none()
                || (draft.error.is_none()
                    && draft
                        .accepted
                        .as_ref()
                        .is_some_and(ScalarExpr::is_time_dependent)
                    && draft.sampled_at != Some(self.time));
            if needs_sample {
                let time = self.time;
                let samples = draft.accepted.as_ref().map(|expr| match context {
                    ScalarContext::Curve => {
                        let slots = sample_curve_at(
                            expr,
                            &self.domain,
                            self.plot.settings.curve_samples,
                            time,
                        );
                        let edges = curve_edges_at(expr, &self.domain, &slots, time);
                        SampledPlot::Curve { slots, edges }
                    }
                    ScalarContext::Surface => SampledPlot::Surface(sample_surface_at(
                        expr,
                        &self.domain,
                        UVec2::splat(self.plot.settings.surface_resolution),
                        time,
                    )),
                });
                let draft = self.scalar_draft_mut(context);
                let sampled_at = samples.as_ref().map(|_| time);
                draft.samples = samples.map(Arc::new);
                draft.sampled_at = sampled_at;
            }
        }
        let draft = context.map(|c| self.scalar_draft(c));
        let mut scene = build_plot_scene(
            self.plot.kind,
            draft.and_then(|d| d.samples.as_deref()),
            &self.domain,
            &self.plot.settings,
            self.colormap,
            self.show_grid,
            self.show_coordinate_system,
            self.plot.generation,
        );
        if let Some(error) = draft.and_then(|d| d.error.as_ref()) {
            scene.status = error.clone();
        }
        Arc::new(scene)
    }
}
