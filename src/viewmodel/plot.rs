//! Scalar editing and lazy sampling, independent of the vector snapshot.
use std::sync::Arc;

use glam::UVec2;

use super::State;
use crate::{
    plot::{PlotKind, curve_edges, sample_curve, sample_surface},
    plot_examples::PlotExample,
    plot_scene::{PlotScene, build_plot_scene},
    plot_state::{PlotSettings, SampledPlot, ScalarDraft},
    scalar::{ScalarContext, ScalarExpr},
};

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
        let previous_grid = self.grid();
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
            let draft = self.scalar_draft_mut(context);
            draft.accepted = None;
            draft.samples = None;
            match ScalarExpr::parse(&draft.source, context) {
                Ok(expr) => {
                    draft.accepted = Some(expr);
                    draft.error = None;
                }
                Err(err) => draft.error = Some(err.to_string()),
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
        let draft = self.scalar_draft_mut(context);
        if !force && draft.source == source {
            return false;
        }
        draft.source = source;
        match ScalarExpr::parse(&draft.source, context) {
            Ok(expr) => {
                draft.accepted = Some(expr);
                draft.samples = None;
                draft.error = None;
            }
            Err(err) => draft.error = Some(err.to_string()),
        }
        self.plot_changed()
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
        self.plot_changed()
    }
    pub fn set_plot_resolution(&mut self, resolution: u32) -> bool {
        let resolution = resolution.clamp(2, 96);
        if self.plot.settings.surface_resolution == resolution {
            return false;
        }
        self.plot.settings.surface_resolution = resolution;
        self.plot.surface.samples = None;
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
        }
        self.update_scalar_source(ScalarContext::Curve, curve, true);
        self.update_scalar_source(ScalarContext::Surface, surface, true);
    }
    pub fn plot_scene(&mut self) -> Arc<PlotScene> {
        let context = match self.plot.kind {
            PlotKind::Curve2D => Some(ScalarContext::Curve),
            PlotKind::Surface3D => Some(ScalarContext::Surface),
            PlotKind::VectorField => None,
        };
        if let Some(context) = context
            && self.scalar_draft(context).samples.is_none()
        {
            let samples = self
                .scalar_draft(context)
                .accepted
                .as_ref()
                .map(|expr| match context {
                    ScalarContext::Curve => {
                        let slots =
                            sample_curve(expr, &self.domain, self.plot.settings.curve_samples);
                        let edges = curve_edges(expr, &self.domain, &slots);
                        SampledPlot::Curve { slots, edges }
                    }
                    ScalarContext::Surface => SampledPlot::Surface(sample_surface(
                        expr,
                        &self.domain,
                        UVec2::splat(self.plot.settings.surface_resolution),
                    )),
                });
            self.scalar_draft_mut(context).samples = samples.map(Arc::new);
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
