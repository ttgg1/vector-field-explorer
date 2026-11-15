//! Pure, bounded scalar plot geometry.

use glam::{DVec3, UVec2, Vec3};
use serde::{Deserialize, Serialize};

use crate::{domain::Domain, scalar::ScalarExpr};

const MAX_CURVE_SLOTS: usize = 4097;
const MAX_PROBE_LEVEL: usize = 6;
const MAX_SURFACE_AXIS: u32 = 96;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlotKind {
    #[default]
    VectorField,
    Curve2D,
    Surface3D,
}

#[derive(Clone, Copy, Debug)]
pub struct PlotBounds {
    pub min: DVec3,
    pub max: DVec3,
}

#[derive(Clone, Debug, Default)]
pub struct SampleDiagnostics {
    pub invalid_samples: usize,
    pub unrepresentable_samples: usize,
    pub rejected_edges: usize,
    pub rejected_cells: usize,
    pub probes: usize,
    pub invalid_domain: bool,
}

#[derive(Clone, Debug, Default)]
pub struct CurveEdges {
    pub segments: Vec<[Vec3; 2]>,
    pub diagnostics: SampleDiagnostics,
}

#[derive(Clone, Debug, Default)]
pub struct SurfaceMesh {
    pub positions: Vec<Vec3>,
    pub normals: Vec<Vec3>,
    pub indices: Vec<u32>,
    pub diagnostics: SampleDiagnostics,
}

/// Inclusive XY graph, capped to 96 vertices per axis before allocation.
/// Domain Z is validated but never clips heights. Missing corners or an invalid
/// center omit the entire cell; a single center probe is not a continuity proof.
/// The open, CCW mesh has no walls/caps and retains only used vertices, with
/// area-weighted normals accumulated in f64 from surviving faces alone.
pub fn sample_surface(expr: &ScalarExpr, domain: &Domain, resolution: UVec2) -> SurfaceMesh {
    let mut mesh = SurfaceMesh::default();
    if !domain.min.is_finite() || !domain.max.is_finite() || !domain.min.cmple(domain.max).all() {
        mesh.diagnostics.invalid_domain = true;
        return mesh;
    }
    let nx = resolution.x.min(MAX_SURFACE_AXIS) as usize;
    let ny = resolution.y.min(MAX_SURFACE_AXIS) as usize;
    if nx < 2 || ny < 2 || domain.min.x == domain.max.x || domain.min.y == domain.max.y {
        return mesh;
    }
    let slots = nx.checked_mul(ny).expect("bounded surface grid");
    let index_cap = (nx - 1)
        .checked_mul(ny - 1)
        .and_then(|cells| cells.checked_mul(6))
        .expect("bounded surface indices");
    let axis = |i: usize, n: usize, lo: f32, hi: f32| {
        let t = i as f64 / (n - 1) as f64;
        (1.0 - t) * f64::from(lo) + t * f64::from(hi)
    };
    let mut grid = vec![None; slots];
    let mut positions = Vec::with_capacity(slots);
    for j in 0..ny {
        let y = axis(j, ny, domain.min.y, domain.max.y);
        for i in 0..nx {
            let x = axis(i, nx, domain.min.x, domain.max.x);
            if let Some(position) = surface_position(expr, x, y, &mut mesh.diagnostics) {
                grid[j * nx + i] = Some(positions.len());
                positions.push(position);
            }
        }
    }
    let mut normals = vec![DVec3::ZERO; positions.len()];
    let mut indices = Vec::with_capacity(index_cap);
    for j in 0..ny - 1 {
        for i in 0..nx - 1 {
            let corners = [
                grid[j * nx + i],
                grid[j * nx + i + 1],
                grid[(j + 1) * nx + i],
                grid[(j + 1) * nx + i + 1],
            ];
            let [Some(a), Some(b), Some(c), Some(d)] = corners else {
                mesh.diagnostics.rejected_cells += 1;
                continue;
            };
            mesh.diagnostics.probes += 1;
            let x = (axis(i, nx, domain.min.x, domain.max.x)
                + axis(i + 1, nx, domain.min.x, domain.max.x))
                * 0.5;
            let y = (axis(j, ny, domain.min.y, domain.max.y)
                + axis(j + 1, ny, domain.min.y, domain.max.y))
                * 0.5;
            if surface_position(expr, x, y, &mut mesh.diagnostics).is_none() {
                mesh.diagnostics.rejected_cells += 1;
                continue;
            }
            let face = |a: usize, b: usize, c: usize| {
                let a = positions[a].as_dvec3();
                (positions[b].as_dvec3() - a).cross(positions[c].as_dvec3() - a)
            };
            let first = face(a, b, c);
            let second = face(b, d, c);
            // Check GPU-rounded geometry before normalization: distinct samples
            // can collapse to the same f32 coordinate on a very narrow domain.
            if [first, second]
                .iter()
                .any(|n| !n.is_finite() || n.length_squared() == 0.0)
            {
                mesh.diagnostics.rejected_cells += 1;
                continue;
            }
            for (triangle, normal) in [([a, b, c], first), ([b, d, c], second)] {
                for index in triangle {
                    normals[index] += normal;
                    indices.push(index);
                }
            }
        }
    }
    let mut remap = vec![None; positions.len()];
    mesh.indices.reserve(indices.len());
    for index in indices {
        let compact = *remap[index].get_or_insert_with(|| {
            let compact = mesh.positions.len() as u32;
            mesh.positions.push(positions[index]);
            // Graph faces all point toward +Z, so a used vertex's finite area
            // sum cannot cancel. f64 also keeps extreme f32 cross-products safe.
            mesh.normals.push(normals[index].normalize().as_vec3());
            compact
        });
        mesh.indices.push(compact);
    }
    mesh
}

fn surface_position(
    expr: &ScalarExpr,
    x: f64,
    y: f64,
    diagnostics: &mut SampleDiagnostics,
) -> Option<Vec3> {
    let Some(z) = expr.eval(x, y) else {
        diagnostics.invalid_samples += 1;
        return None;
    };
    let position = Vec3::new(x as f32, y as f32, z as f32);
    if !position.is_finite() {
        diagnostics.unrepresentable_samples += 1;
        return None;
    }
    Some(position)
}

/// Inclusive X samples `(x, f(x), 0)`, including absent slots at invalid heights.
/// Y/Z never clip heights. One requested sample lies at the X midpoint.
pub fn sample_curve(expr: &ScalarExpr, domain: &Domain, samples: usize) -> Vec<Option<Vec3>> {
    sample_curve_with_diagnostics(expr, domain, samples).0
}

pub fn sample_curve_with_diagnostics(
    expr: &ScalarExpr,
    domain: &Domain,
    samples: usize,
) -> (Vec<Option<Vec3>>, SampleDiagnostics) {
    let samples = samples.min(MAX_CURVE_SLOTS);
    let mut diagnostics = SampleDiagnostics::default();
    if !valid_curve_domain(domain) {
        diagnostics.invalid_domain = true;
        return (vec![None; samples], diagnostics);
    }
    let lo = f64::from(domain.min.x);
    let hi = f64::from(domain.max.x);
    let slots = (0..samples)
        .map(|i| {
            let t = if samples == 1 {
                0.5
            } else {
                i as f64 / (samples - 1) as f64
            };
            // Weighted f64 interpolation avoids f32 span overflow and cancellation
            // of a small inclusive endpoint against a very large opposite endpoint.
            let x = (1.0 - t) * lo + t * hi;
            checked_height(expr, x, &mut diagnostics)
                .map(|height| Vec3::new(x as f32, height as f32, 0.0))
        })
        .collect();
    (slots, diagnostics)
}

/// Only original adjacent slots can yield segments; probes never add geometry.
/// This conservative display heuristic can omit steep continuous edges and does
/// not prove continuity. Root level 0 through level 6 costs at most 127 probes.
pub fn curve_edges(expr: &ScalarExpr, domain: &Domain, slots: &[Option<Vec3>]) -> CurveEdges {
    let mut result = CurveEdges::default();
    if !valid_curve_domain(domain) {
        result.diagnostics.invalid_domain = true;
        return result;
    }
    let threshold = 8.0 * (f64::from(domain.max.y) - f64::from(domain.min.y)).max(1.0);
    for pair in slots[..slots.len().min(MAX_CURVE_SLOTS)].windows(2) {
        let [Some(a), Some(b)] = pair else { continue };
        if !a.is_finite() || !b.is_finite() || a == b {
            result.diagnostics.rejected_edges += 1;
            continue;
        }
        if probe_edge(
            expr,
            [f64::from(a.x), f64::from(a.y)],
            [f64::from(b.x), f64::from(b.y)],
            threshold,
            0,
            &mut result.diagnostics,
        ) {
            result.segments.push([*a, *b]);
        } else {
            result.diagnostics.rejected_edges += 1;
        }
    }
    result
}

// Z is irrelevant for curve sampling and for the discontinuity threshold.
fn valid_curve_domain(domain: &Domain) -> bool {
    domain.min.x.is_finite()
        && domain.max.x.is_finite()
        && domain.min.y.is_finite()
        && domain.max.y.is_finite()
        && domain.min.x <= domain.max.x
        && domain.min.y <= domain.max.y
}

fn checked_height(expr: &ScalarExpr, x: f64, diagnostics: &mut SampleDiagnostics) -> Option<f64> {
    let Some(height) = expr.eval(x, 0.0).filter(|_| x.is_finite()) else {
        diagnostics.invalid_samples += 1;
        return None;
    };
    if !(x as f32).is_finite() || !(height as f32).is_finite() {
        diagnostics.unrepresentable_samples += 1;
        return None;
    }
    Some(height)
}

fn probe_edge(
    expr: &ScalarExpr,
    a: [f64; 2],
    b: [f64; 2],
    threshold: f64,
    level: usize,
    diagnostics: &mut SampleDiagnostics,
) -> bool {
    diagnostics.probes += 1;
    let x = (a[0] + b[0]) * 0.5;
    let Some(y) = checked_height(expr, x, diagnostics) else {
        return false;
    };
    let spread = a[1].max(b[1]).max(y) - a[1].min(b[1]).min(y);
    if spread <= threshold {
        return true;
    }
    if level == MAX_PROBE_LEVEL {
        return false;
    }
    let mid = [x, y];
    probe_edge(expr, a, mid, threshold, level + 1, diagnostics)
        && probe_edge(expr, mid, b, threshold, level + 1, diagnostics)
}
