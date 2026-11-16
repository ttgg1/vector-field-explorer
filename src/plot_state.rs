//! Transient scalar drafts, sampled geometry and plot controls.
use std::sync::Arc;

use glam::Vec3;

use crate::{
    plot::{CurveEdges, PlotKind, SurfaceMesh},
    scalar::ScalarExpr,
};

#[derive(Clone, Debug, Default)]
pub struct ScalarDraft {
    pub source: String,
    pub error: Option<String>,
    pub accepted: Option<ScalarExpr>,
    pub samples: Option<Arc<SampledPlot>>,
}

#[derive(Clone, Debug)]
pub enum SampledPlot {
    Curve {
        slots: Vec<Option<Vec3>>,
        edges: CurveEdges,
    },
    Surface(SurfaceMesh),
}

#[derive(Clone, Debug)]
pub struct PlotSettings {
    pub curve_samples: usize,
    pub surface_resolution: u32,
    pub line_width: f32,
    pub auto_height: bool,
    pub height_min: f32,
    pub height_max: f32,
}

impl Default for PlotSettings {
    fn default() -> Self {
        Self {
            curve_samples: 513,
            surface_resolution: 64,
            line_width: 0.01,
            auto_height: true,
            height_min: 0.0,
            height_max: 1.0,
        }
    }
}

#[derive(Clone, Debug)]
pub struct PlotState {
    pub kind: PlotKind,
    pub curve: ScalarDraft,
    pub surface: ScalarDraft,
    pub settings: PlotSettings,
    pub vector_mode_3d: bool,
    pub generation: u64,
    pub transition_epoch: u64,
}

impl Default for PlotState {
    fn default() -> Self {
        Self {
            kind: PlotKind::VectorField,
            curve: ScalarDraft {
                source: "0.5*x^2".into(),
                ..Default::default()
            },
            surface: ScalarDraft {
                source: "exp(-(x^2+y^2))".into(),
                ..Default::default()
            },
            settings: PlotSettings::default(),
            vector_mode_3d: true,
            generation: 0,
            transition_epoch: 0,
        }
    }
}
