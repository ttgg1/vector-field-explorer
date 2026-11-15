//! Immutable renderer-ready scalar scenes built from already sampled data.
use std::sync::Arc;

use glam::Vec3;

use crate::{
    colormap::Colormap,
    domain::Domain,
    plot::{PlotBounds, PlotKind},
    plot_state::{PlotSettings, SampledPlot},
    reference::reference_lines_visible,
    renderer::{LineInstanceGPU, SceneGPU},
};

#[derive(Clone)]
pub enum ViewportScene {
    Vector(SceneGPU),
    Plot(Arc<PlotScene>),
}

#[derive(Clone)]
pub enum PlotGeometry {
    Curve(Arc<Vec<LineInstanceGPU>>),
    Surface(Arc<Vec<PlotVertex>>, Arc<Vec<u32>>),
}

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct PlotVertex {
    pub position: [f32; 4],
    pub normal: [f32; 4],
    pub color: [f32; 4],
}

#[derive(Clone)]
pub struct PlotScene {
    pub kind: PlotKind,
    pub geometry: PlotGeometry,
    pub references: Arc<Vec<LineInstanceGPU>>,
    pub bounds: Option<PlotBounds>,
    pub height_range: (f32, f32),
    pub generation: u64,
    pub status: String,
}

fn rgba(palette: Colormap, value: f32, range: (f32, f32)) -> [f32; 4] {
    // Normalize in f64 so finite extreme heights cannot overflow f32 spans.
    let rgb = if range.0 == range.1 {
        [128; 3]
    } else {
        let t = ((f64::from(value) - f64::from(range.0))
            / (f64::from(range.1) - f64::from(range.0)))
        .clamp(0.0, 1.0) as f32;
        palette.colormap(t, 0.0, 1.0)
    };
    [
        rgb[0] as f32 / 255.0,
        rgb[1] as f32 / 255.0,
        rgb[2] as f32 / 255.0,
        1.0,
    ]
}

fn range(settings: &PlotSettings, bounds: Option<PlotBounds>, kind: PlotKind) -> (f32, f32) {
    if settings.auto_height {
        bounds
            .map(|b| {
                if kind == PlotKind::Curve2D {
                    (b.min.y as f32, b.max.y as f32)
                } else {
                    (b.min.z as f32, b.max.z as f32)
                }
            })
            .unwrap_or((0.0, 0.0))
    } else if settings.height_min.is_finite() && settings.height_max.is_finite() {
        (
            settings.height_min.min(settings.height_max),
            settings.height_min.max(settings.height_max),
        )
    } else {
        (0.0, 1.0)
    }
}

fn include(bounds: &mut Option<PlotBounds>, p: Vec3) {
    let p = p.as_dvec3();
    match bounds {
        Some(b) => {
            b.min = b.min.min(p);
            b.max = b.max.max(p);
        }
        None => *bounds = Some(PlotBounds { min: p, max: p }),
    }
}

#[allow(clippy::too_many_arguments)]
pub fn build_plot_scene(
    kind: PlotKind,
    samples: Option<&SampledPlot>,
    domain: &Domain,
    settings: &PlotSettings,
    palette: Colormap,
    show_grid: bool,
    show_axes: bool,
    generation: u64,
) -> PlotScene {
    let mut bounds = None;
    let mut segments = Vec::new();
    let mut surface = None;
    match (kind, samples) {
        (PlotKind::Curve2D, Some(SampledPlot::Curve { edges, .. })) => {
            for &[a, b] in &edges.segments {
                if a.is_finite() && b.is_finite() && a != b {
                    include(&mut bounds, a);
                    include(&mut bounds, b);
                    segments.push([a, b]);
                }
            }
        }
        (PlotKind::Surface3D, Some(SampledPlot::Surface(mesh))) => {
            let mut indices = Vec::new();
            for tri in mesh.indices.as_chunks::<3>().0 {
                let valid = tri.iter().all(|&i| {
                    mesh.positions
                        .get(i as usize)
                        .is_some_and(|p| p.is_finite())
                        && mesh.normals.get(i as usize).is_some_and(|n| n.is_finite())
                });
                let nondegenerate = valid && {
                    let a = mesh.positions[tri[0] as usize].as_dvec3();
                    let b = mesh.positions[tri[1] as usize].as_dvec3();
                    let c = mesh.positions[tri[2] as usize].as_dvec3();
                    (b - a).cross(c - a).length_squared() > 0.0
                };
                if nondegenerate {
                    for &i in tri {
                        include(&mut bounds, mesh.positions[i as usize]);
                    }
                    indices.extend_from_slice(tri);
                }
            }
            surface = Some((mesh, indices));
        }
        _ => {}
    }
    let height_range = range(settings, bounds, kind);
    let geometry = if kind == PlotKind::Surface3D {
        let mut vertices = Vec::new();
        let mut out_indices = Vec::new();
        if let Some((mesh, indices)) = surface {
            let mut remap = vec![None; mesh.positions.len()];
            for i in indices {
                let idx = i as usize;
                let next = *remap[idx].get_or_insert_with(|| {
                    let p = mesh.positions[idx];
                    let n = mesh.normals[idx];
                    let index = vertices.len() as u32;
                    vertices.push(PlotVertex {
                        position: [p.x, p.y, p.z, 1.0],
                        normal: [n.x, n.y, n.z, 0.0],
                        color: rgba(palette, p.z, height_range),
                    });
                    index
                });
                out_indices.push(next);
            }
        }
        PlotGeometry::Surface(Arc::new(vertices), Arc::new(out_indices))
    } else {
        let width = if settings.line_width.is_finite() && settings.line_width > 0.0 {
            settings.line_width
        } else {
            0.01
        };
        PlotGeometry::Curve(Arc::new(
            segments
                .into_iter()
                .map(|[a, b]| LineInstanceGPU {
                    a: [a.x, a.y, a.z, 1.0],
                    b: [b.x, b.y, b.z, width],
                    color: rgba(
                        palette,
                        ((f64::from(a.y) + f64::from(b.y)) * 0.5) as f32,
                        height_range,
                    ),
                })
                .collect(),
        ))
    };
    let references = if kind != PlotKind::VectorField
        && domain.min.is_finite()
        && domain.max.is_finite()
        && domain.min.cmple(domain.max).all()
    {
        reference_lines_visible(domain, kind == PlotKind::Surface3D, show_grid, show_axes)
            .into_iter()
            .filter(|l| l.a.iter().chain(l.b.iter()).all(|x| x.is_finite()) && l.a[..3] != l.b[..3])
            .collect()
    } else {
        Vec::new()
    };
    PlotScene {
        kind,
        geometry,
        references: Arc::new(references),
        bounds,
        height_range,
        generation,
        status: if bounds.is_some() {
            String::new()
        } else {
            "No drawable plot samples".into()
        },
    }
}
