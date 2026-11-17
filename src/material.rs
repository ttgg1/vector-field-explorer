//! Bounded analytic material cases for the teaching explorer.
//!
//! Four canonical cases are supported: a dielectric sphere, a planar
//! dielectric interface, a permeable sphere and a planar permeable interface.
//! Each case exposes `Total`, `Applied` and `Response` views; the response is
//! always exactly `total - applied`, and the electric and magnetic families are
//! never mixed. Everything here is pure CPU code with no renderer, clock,
//! source or persistence dependency.

use glam::{UVec3, Vec3};

use crate::domain::Domain;
use crate::plane::{PlaneAxis, PlaneFrame};
use crate::sampling::ArrowData;

/// Default sphere radius for a new material scene.
pub const MATERIAL_DEFAULT_RADIUS: f64 = 1.0;
/// Default planar-interface offset for a new material scene.
pub const MATERIAL_DEFAULT_INTERFACE_OFFSET: f64 = 0.0;
/// Documented fractional interface tolerance exposed to the UI.
///
/// The exact-hit guard used by sampling is the domain-scaled `1e-9 * max(extent,
/// 1.0)`; this constant records the design's applicability scale without
/// turning the guard into an omitted band.
pub const MATERIAL_INTERFACE_TOLERANCE_FACTOR: f64 = 1e-6;
/// Largest accepted relative permittivity/permeability.
pub const MATERIAL_MAX_RELATIVE: f64 = 100.0;
/// Smallest accepted relative permittivity/permeability.
pub const MATERIAL_MIN_RELATIVE: f64 = 1.0;

/// Fixed electric-family arrow colour (amber).
pub const MATERIAL_ELECTRIC_COLOR: [u8; 3] = [255, 176, 64];
/// Fixed magnetic-family arrow colour (cyan).
pub const MATERIAL_MAGNETIC_COLOR: [u8; 3] = [64, 196, 255];

/// Exact interface warning text; never suppressible.
pub const MATERIAL_INTERFACE_WARNING: &str = "Samples exactly on the interface are omitted because the analytic field is discontinuous there.";
/// Exact zero-response warning text.
pub const MATERIAL_RESPONSE_EMPTY_WARNING: &str = "Response is identically zero for this view.";
/// Warning when the domain or grid cannot produce a sample.
pub const MATERIAL_EMPTY_DOMAIN_WARNING: &str = "The sampling domain or grid produces no samples.";
/// Warning when a 2D slice lies exactly on a planar interface.
pub const MATERIAL_COINCIDENT_SLICE_WARNING: &str =
    "Slice lies on the interface; the field is discontinuous here.";

/// The exact-hit tolerance is scaled by the domain extent, not a fixed band.
const EXACT_HIT_FACTOR: f64 = 1e-9;
/// Vectors below this magnitude are degenerate and are not drawn.
const DEGENERATE_MAGNITUDE: f32 = 1e-6;
/// Great-circle/circle segment count for outlines.
const OUTLINE_SEGMENTS: usize = 64;
/// Outline alpha for the reference geometry.
const OUTLINE_ALPHA: f32 = 0.35;

/// One of the four bounded analytic cases.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MaterialCase {
    DielectricSphere,
    DielectricBoundary,
    PermeableSphere,
    PermeableBoundary,
}

/// Which part of the decomposition is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MaterialView {
    Total,
    Applied,
    Response,
}

/// The physical family a case belongs to. The families are never summed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MaterialFamily {
    Electric,
    Magnetic,
}

/// One editable material scene. Constants are relative; the applied field is in
/// display units (V/m for electric cases, T for magnetic cases).
#[derive(Debug, Clone, PartialEq)]
pub struct MaterialScene {
    pub case: MaterialCase,
    pub background_epsilon_r: f64,
    pub background_mu_r: f64,
    pub material_epsilon_r: f64,
    pub material_mu_r: f64,
    pub radius: f64,
    pub interface_axis: PlaneAxis,
    pub interface_offset: f64,
    pub applied: [f64; 3],
    pub view: MaterialView,
}

impl Default for MaterialScene {
    fn default() -> Self {
        Self {
            case: MaterialCase::DielectricSphere,
            background_epsilon_r: 1.0,
            background_mu_r: 1.0,
            material_epsilon_r: 4.0,
            material_mu_r: 1.0,
            radius: MATERIAL_DEFAULT_RADIUS,
            interface_axis: PlaneAxis::Z,
            interface_offset: MATERIAL_DEFAULT_INTERFACE_OFFSET,
            applied: [1.0, 0.0, 0.0],
            view: MaterialView::Total,
        }
    }
}

/// Why a scene or point cannot be evaluated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MaterialError {
    InvalidScene(String),
    Unsupported(String),
}

/// Renderer-independent outline geometry. Task 2 converts it to the GPU line
/// instance type.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MaterialLine {
    pub a: [f32; 3],
    pub b: [f32; 3],
    pub color: [f32; 4],
}

/// One sampled material view plus its provenance and warnings.
#[derive(Debug, Clone, PartialEq)]
pub struct MaterialSampling {
    pub arrows: Vec<ArrowData>,
    pub family: MaterialFamily,
    pub case: MaterialCase,
    pub view: MaterialView,
    pub background_epsilon_r: f64,
    pub background_mu_r: f64,
    pub material_epsilon_r: f64,
    pub material_mu_r: f64,
    pub applied: [f64; 3],
    pub omitted_interface: usize,
    pub response_empty: bool,
    pub assumptions: &'static str,
    pub warning: Option<String>,
}

/// Electric cases draw `E`; magnetic cases draw `B`.
pub fn material_family(case: MaterialCase) -> MaterialFamily {
    match case {
        MaterialCase::DielectricSphere | MaterialCase::DielectricBoundary => {
            MaterialFamily::Electric
        }
        MaterialCase::PermeableSphere | MaterialCase::PermeableBoundary => MaterialFamily::Magnetic,
    }
}

/// Static applicability text for the selected case.
pub fn material_assumptions(case: MaterialCase) -> &'static str {
    match case {
        MaterialCase::DielectricSphere => {
            "Homogeneous, linear, isotropic, nondispersive, nonconducting, quasistatic dielectric sphere in an infinite homogeneous background. The applied E field is uniform at infinity; the sampling domain is only a visualization window and no free charge lies on the interface."
        }
        MaterialCase::DielectricBoundary => {
            "Homogeneous, linear, isotropic, nondispersive, nonconducting, quasistatic dielectric half-space in an infinite homogeneous background. The applied E field is uniform at infinity; the sampling domain is only a visualization window and no free charge lies on the interface."
        }
        MaterialCase::PermeableSphere => {
            "Homogeneous, linear, isotropic, nondispersive, nonconducting, quasistatic permeable sphere in an infinite homogeneous background. The applied B field is uniform at infinity; the sampling domain is only a visualization window and no free current lies on the interface."
        }
        MaterialCase::PermeableBoundary => {
            "Homogeneous, linear, isotropic, nondispersive, nonconducting, quasistatic permeable half-space in an infinite homogeneous background. The applied B field is uniform at infinity; the sampling domain is only a visualization window and no free current lies on the interface."
        }
    }
}

/// Shared bounded-catalogue warning.
pub fn material_warning(_case: MaterialCase) -> &'static str {
    "Bounded analytic case; no general material solver."
}

/// Validate every editable value before any physics runs.
pub fn validate_material(scene: &MaterialScene) -> Result<(), MaterialError> {
    for (label, value) in [
        ("background epsilon_r", scene.background_epsilon_r),
        ("background mu_r", scene.background_mu_r),
        ("material epsilon_r", scene.material_epsilon_r),
        ("material mu_r", scene.material_mu_r),
    ] {
        if !value.is_finite() || !(MATERIAL_MIN_RELATIVE..=MATERIAL_MAX_RELATIVE).contains(&value) {
            return Err(MaterialError::InvalidScene(format!(
                "{label} must be finite and within 1.0..=100.0"
            )));
        }
    }

    let [ax, ay, az] = scene.applied;
    let magnitude = (ax * ax + ay * ay + az * az).sqrt();
    if !ax.is_finite()
        || !ay.is_finite()
        || !az.is_finite()
        || !magnitude.is_finite()
        || magnitude <= 0.0
    {
        return Err(MaterialError::InvalidScene(
            "applied field must be finite and nonzero".into(),
        ));
    }

    match scene.case {
        MaterialCase::DielectricSphere | MaterialCase::PermeableSphere => {
            if !scene.radius.is_finite() || scene.radius <= 0.0 {
                return Err(MaterialError::InvalidScene(
                    "sphere radius must be finite and greater than zero".into(),
                ));
            }
        }
        MaterialCase::DielectricBoundary | MaterialCase::PermeableBoundary => {
            if !scene.interface_offset.is_finite() {
                return Err(MaterialError::InvalidScene(
                    "interface offset must be finite".into(),
                ));
            }
        }
    }
    Ok(())
}

/// The uniform applied field in display units.
pub fn material_applied(scene: &MaterialScene) -> Vec3 {
    applied_vec(scene)
}

/// The total field at `point`, or [`MaterialError::Unsupported`] exactly on an
/// interface.
pub fn material_field(scene: &MaterialScene, point: Vec3) -> Result<Vec3, MaterialError> {
    validate_material(scene)?;
    if !point.is_finite() {
        return Err(MaterialError::Unsupported(
            "sample point must be finite".into(),
        ));
    }
    match scene.case {
        MaterialCase::DielectricSphere => sphere_field(scene, point, true),
        MaterialCase::PermeableSphere => sphere_field(scene, point, false),
        MaterialCase::DielectricBoundary => boundary_field(scene, point, true),
        MaterialCase::PermeableBoundary => boundary_field(scene, point, false),
    }
}

/// The bound/induced response `total - applied`.
pub fn material_response(scene: &MaterialScene, point: Vec3) -> Result<Vec3, MaterialError> {
    Ok(material_field(scene, point)? - material_applied(scene))
}

/// Renderer-independent outline for the selected case.
///
/// `None` is the 3D world outline. `Some(frame)` is the 2D slice: only the
/// slice intersection is returned, in the frame's local `(u, v, 0)` coordinates.
pub fn material_outline(
    scene: &MaterialScene,
    domain: &Domain,
    plane: Option<&PlaneFrame>,
) -> Vec<MaterialLine> {
    let family = material_family(scene.case);
    let color = family_line_color(family);
    match (scene.case, plane) {
        (MaterialCase::DielectricSphere | MaterialCase::PermeableSphere, None) => {
            sphere_world_outline(scene, color)
        }
        (MaterialCase::DielectricSphere | MaterialCase::PermeableSphere, Some(frame)) => {
            sphere_slice_outline(scene, frame, color)
        }
        (MaterialCase::DielectricBoundary | MaterialCase::PermeableBoundary, None) => {
            boundary_world_outline(scene, domain, color)
        }
        (MaterialCase::DielectricBoundary | MaterialCase::PermeableBoundary, Some(frame)) => {
            boundary_slice_outline(scene, domain, frame, color)
        }
    }
}

/// Sample one material view over the domain grid (3D) or its active plane
/// slice (2D).
pub fn sample_materials(
    scene: &MaterialScene,
    domain: &Domain,
    grid: UVec3,
    plane: Option<&PlaneFrame>,
) -> MaterialSampling {
    let family = material_family(scene.case);
    let assumptions = material_assumptions(scene.case);
    let mut sampling = MaterialSampling {
        arrows: Vec::new(),
        family,
        case: scene.case,
        view: scene.view,
        background_epsilon_r: scene.background_epsilon_r,
        background_mu_r: scene.background_mu_r,
        material_epsilon_r: scene.material_epsilon_r,
        material_mu_r: scene.material_mu_r,
        applied: scene.applied,
        omitted_interface: 0,
        response_empty: false,
        assumptions,
        warning: None,
    };

    if let Err(error) = validate_material(scene) {
        sampling.warning = Some(error_text(&error));
        return sampling;
    }

    let effective_domain = match plane {
        Some(frame) => frame.local_domain(),
        None => *domain,
    };
    let size = effective_domain.domain_size();
    let effective_grid = match plane {
        Some(_) => UVec3::new(grid.x, grid.y, 1),
        None => grid,
    };
    let dims_ok = effective_grid.x > 0 && effective_grid.y > 0 && effective_grid.z > 0;
    let extents_ok = size.x > 0.0 && size.y > 0.0 && (plane.is_some() || size.z > 0.0);
    let finite_domain = effective_domain.min.is_finite()
        && effective_domain.max.is_finite()
        && size.is_finite()
        && plane.is_none_or(|frame| {
            frame.origin.is_finite() && frame.right.is_finite() && frame.up.is_finite()
        });
    let can_sample = dims_ok && extents_ok && finite_domain;

    let extent = size.x.abs().max(size.y.abs()).max(size.z.abs()).max(1.0) as f64;
    let tolerance = EXACT_HIT_FACTOR * extent;

    let mut finite_response_seen = false;
    let mut any_nonzero_response = false;

    if can_sample {
        for raw_local in effective_domain.grid_points(effective_grid) {
            let (position, world) = match plane {
                Some(frame) => (raw_local, frame.to_world(raw_local)),
                None => (raw_local, raw_local),
            };
            if !position.is_finite() || !world.is_finite() {
                continue;
            }
            if on_interface(scene, world, tolerance) {
                sampling.omitted_interface += 1;
                continue;
            }
            let Ok(total) = material_field(scene, world) else {
                continue;
            };
            let applied = material_applied(scene);
            let response = total - applied;
            if response.is_finite() {
                finite_response_seen = true;
                if response.length() >= DEGENERATE_MAGNITUDE {
                    any_nonzero_response = true;
                }
            }
            let selected = match scene.view {
                MaterialView::Total => total,
                MaterialView::Applied => applied,
                MaterialView::Response => response,
            };
            let vector = match plane {
                Some(frame) => frame.project(selected),
                None => selected,
            };
            let mag = vector.length();
            if !vector.is_finite() || !mag.is_finite() || mag < DEGENERATE_MAGNITUDE {
                continue;
            }
            let dir = vector / mag;
            if !dir.is_finite() {
                continue;
            }
            let color = match family {
                MaterialFamily::Electric => MATERIAL_ELECTRIC_COLOR,
                MaterialFamily::Magnetic => MATERIAL_MAGNETIC_COLOR,
            };
            sampling.arrows.push(ArrowData {
                position,
                dir,
                mag,
                color,
            });
        }
    }

    if scene.view == MaterialView::Response && finite_response_seen && !any_nonzero_response {
        sampling.response_empty = true;
    }

    let mut warnings = Vec::new();
    if sampling.omitted_interface > 0 {
        warnings.push(MATERIAL_INTERFACE_WARNING);
    }
    if sampling.response_empty {
        warnings.push(MATERIAL_RESPONSE_EMPTY_WARNING);
    }
    if !can_sample {
        warnings.push(MATERIAL_EMPTY_DOMAIN_WARNING);
    }
    if plane.is_some() && coincident_interface_slice(scene, plane) {
        warnings.push(MATERIAL_COINCIDENT_SLICE_WARNING);
    }
    if !warnings.is_empty() {
        sampling.warning = Some(warnings.join("; "));
    }
    sampling
}

fn applied_vec(scene: &MaterialScene) -> Vec3 {
    Vec3::new(
        scene.applied[0] as f32,
        scene.applied[1] as f32,
        scene.applied[2] as f32,
    )
}

fn error_text(error: &MaterialError) -> String {
    match error {
        MaterialError::InvalidScene(message) | MaterialError::Unsupported(message) => {
            message.clone()
        }
    }
}

fn sphere_field(scene: &MaterialScene, point: Vec3, electric: bool) -> Result<Vec3, MaterialError> {
    let radius = scene.radius;
    let p = point.as_dvec3();
    let r = p.length();
    let tolerance = EXACT_HIT_FACTOR * radius.max(1.0);
    if (r - radius).abs() <= tolerance {
        return Err(MaterialError::Unsupported(
            MATERIAL_INTERFACE_WARNING.into(),
        ));
    }

    let applied = applied_vec(scene).as_dvec3();
    let (background, material) = if electric {
        (scene.background_epsilon_r, scene.material_epsilon_r)
    } else {
        (scene.background_mu_r, scene.material_mu_r)
    };

    let value = if r < radius {
        // `r == 0` takes the same uniform inside value.
        let coefficient = if electric {
            3.0 * background / (material + 2.0 * background)
        } else {
            3.0 * material / (material + 2.0 * background)
        };
        applied * coefficient
    } else {
        let rhat = p / r;
        let beta = (material - background) / (material + 2.0 * background);
        let dot = applied.dot(rhat);
        let dipole = (3.0 * dot * rhat - applied) * (beta * radius.powi(3) / r.powi(3));
        applied + dipole
    };

    Ok(Vec3::new(value.x as f32, value.y as f32, value.z as f32))
}

fn boundary_field(
    scene: &MaterialScene,
    point: Vec3,
    electric: bool,
) -> Result<Vec3, MaterialError> {
    let normal = axis_vector(scene.interface_axis);
    let p = point.as_dvec3();
    let n = normal.as_dvec3();
    let offset = scene.interface_offset;
    let tolerance = EXACT_HIT_FACTOR * offset.abs().max(1.0);
    let coordinate = p.dot(n);
    if (coordinate - offset).abs() <= tolerance {
        return Err(MaterialError::Unsupported(
            MATERIAL_INTERFACE_WARNING.into(),
        ));
    }

    let applied = applied_vec(scene).as_dvec3();
    let normal_part = n * applied.dot(n);
    let tangential = applied - normal_part;
    let value = if coordinate < offset {
        applied
    } else if electric {
        let scale = scene.background_epsilon_r / scene.material_epsilon_r;
        normal_part * scale + tangential
    } else {
        let scale = scene.material_mu_r / scene.background_mu_r;
        normal_part + tangential * scale
    };

    Ok(Vec3::new(value.x as f32, value.y as f32, value.z as f32))
}

fn on_interface(scene: &MaterialScene, world: Vec3, tolerance: f64) -> bool {
    let p = world.as_dvec3();
    match scene.case {
        MaterialCase::DielectricSphere | MaterialCase::PermeableSphere => {
            (p.length() - scene.radius).abs() <= tolerance
        }
        MaterialCase::DielectricBoundary | MaterialCase::PermeableBoundary => {
            let coordinate = p.dot(axis_vector(scene.interface_axis).as_dvec3());
            (coordinate - scene.interface_offset).abs() <= tolerance
        }
    }
}

fn axis_vector(axis: PlaneAxis) -> Vec3 {
    match axis {
        PlaneAxis::X => Vec3::X,
        PlaneAxis::Y => Vec3::Y,
        PlaneAxis::Z => Vec3::Z,
    }
}

fn in_plane_basis(axis: PlaneAxis) -> (Vec3, Vec3) {
    match axis {
        PlaneAxis::X => (Vec3::Y, Vec3::Z),
        PlaneAxis::Y => (Vec3::X, Vec3::Z),
        PlaneAxis::Z => (Vec3::X, Vec3::Y),
    }
}

fn family_line_color(family: MaterialFamily) -> [f32; 4] {
    let [r, g, b] = match family {
        MaterialFamily::Electric => MATERIAL_ELECTRIC_COLOR,
        MaterialFamily::Magnetic => MATERIAL_MAGNETIC_COLOR,
    };
    [
        f32::from(r) / 255.0,
        f32::from(g) / 255.0,
        f32::from(b) / 255.0,
        OUTLINE_ALPHA,
    ]
}

fn line(a: Vec3, b: Vec3, color: [f32; 4]) -> Option<MaterialLine> {
    if !a.is_finite() || !b.is_finite() {
        return None;
    }
    Some(MaterialLine {
        a: [a.x, a.y, a.z],
        b: [b.x, b.y, b.z],
        color,
    })
}

fn circle(center: Vec3, u: Vec3, v: Vec3, radius: f32, color: [f32; 4]) -> Vec<MaterialLine> {
    let mut lines = Vec::with_capacity(OUTLINE_SEGMENTS);
    if !center.is_finite() || !u.is_finite() || !v.is_finite() || !radius.is_finite() {
        return lines;
    }
    let mut previous = center + u * radius;
    for index in 1..=OUTLINE_SEGMENTS {
        let angle = std::f32::consts::TAU * index as f32 / OUTLINE_SEGMENTS as f32;
        let point = center + u * (radius * angle.cos()) + v * (radius * angle.sin());
        if let Some(segment) = line(previous, point, color) {
            lines.push(segment);
        }
        previous = point;
    }
    lines
}

fn sphere_world_outline(scene: &MaterialScene, color: [f32; 4]) -> Vec<MaterialLine> {
    let radius = scene.radius as f32;
    if !radius.is_finite() || radius <= 0.0 {
        return Vec::new();
    }
    let mut lines = Vec::new();
    lines.extend(circle(Vec3::ZERO, Vec3::X, Vec3::Y, radius, color));
    lines.extend(circle(Vec3::ZERO, Vec3::X, Vec3::Z, radius, color));
    lines.extend(circle(Vec3::ZERO, Vec3::Y, Vec3::Z, radius, color));
    let applied = material_applied(scene);
    let direction = if applied.is_finite() && applied.length() > 0.0 {
        applied.normalize()
    } else {
        Vec3::X
    };
    if let Some(radial) = line(Vec3::ZERO, direction * (0.5 * radius), color) {
        lines.push(radial);
    }
    lines
}

fn sphere_slice_outline(
    scene: &MaterialScene,
    frame: &PlaneFrame,
    color: [f32; 4],
) -> Vec<MaterialLine> {
    let radius = scene.radius as f32;
    if !radius.is_finite() || radius <= 0.0 {
        return Vec::new();
    }
    let distance = frame.normal.dot(frame.origin);
    if !distance.is_finite() || distance.abs() >= radius {
        return Vec::new();
    }
    let cut_radius = (radius * radius - distance * distance).max(0.0).sqrt();
    if !cut_radius.is_finite() || cut_radius <= 0.0 {
        return Vec::new();
    }
    circle(Vec3::ZERO, Vec3::X, Vec3::Y, cut_radius, color)
}

fn boundary_world_outline(
    scene: &MaterialScene,
    domain: &Domain,
    color: [f32; 4],
) -> Vec<MaterialLine> {
    let normal = axis_vector(scene.interface_axis);
    let (u, v) = in_plane_basis(scene.interface_axis);
    let size = domain.domain_size();
    let half_u = 0.55
        * match scene.interface_axis {
            PlaneAxis::X => size.y.abs(),
            PlaneAxis::Y => size.x.abs(),
            PlaneAxis::Z => size.x.abs(),
        };
    let half_v = 0.55
        * match scene.interface_axis {
            PlaneAxis::X => size.z.abs(),
            PlaneAxis::Y => size.z.abs(),
            PlaneAxis::Z => size.y.abs(),
        };
    if !half_u.is_finite() || !half_v.is_finite() {
        return Vec::new();
    }
    let mut center = domain.center();
    match scene.interface_axis {
        PlaneAxis::X => center.x = scene.interface_offset as f32,
        PlaneAxis::Y => center.y = scene.interface_offset as f32,
        PlaneAxis::Z => center.z = scene.interface_offset as f32,
    }
    if !center.is_finite() {
        return Vec::new();
    }
    let corners = [
        center + u * -half_u + v * -half_v,
        center + u * half_u + v * -half_v,
        center + u * half_u + v * half_v,
        center + u * -half_u + v * half_v,
    ];
    let mut lines = Vec::with_capacity(5);
    for index in 0..4 {
        if let Some(segment) = line(corners[index], corners[(index + 1) % 4], color) {
            lines.push(segment);
        }
    }
    let extent = size.x.abs().max(size.y.abs()).max(size.z.abs());
    if let Some(normal_line) = line(center, center + normal * (0.25 * extent), color) {
        lines.push(normal_line);
    }
    lines
}

fn boundary_slice_outline(
    scene: &MaterialScene,
    _domain: &Domain,
    frame: &PlaneFrame,
    color: [f32; 4],
) -> Vec<MaterialLine> {
    let normal = axis_vector(scene.interface_axis);
    let dot = normal.dot(frame.normal).abs();
    let parallel = (dot - 1.0).abs() <= 1e-6;
    if parallel {
        let coincident = (normal.dot(frame.origin) - scene.interface_offset as f32).abs()
            <= (EXACT_HIT_FACTOR * scene.interface_offset.abs().max(1.0)) as f32;
        if !coincident {
            return Vec::new();
        }
        return local_square(frame, color);
    }

    let a = normal.dot(frame.right);
    let b = normal.dot(frame.up);
    if !a.is_finite() || !b.is_finite() || (a.abs() < 1e-9 && b.abs() < 1e-9) {
        return Vec::new();
    }
    let c = scene.interface_offset as f32 - normal.dot(frame.origin);
    let (point, direction) = if a.abs() >= b.abs() {
        (Vec3::new(c / a, 0.0, 0.0), Vec3::new(-b, a, 0.0))
    } else {
        (Vec3::new(0.0, c / b, 0.0), Vec3::new(-b, a, 0.0))
    };
    if !point.is_finite() || !direction.is_finite() {
        return Vec::new();
    }
    let Some((start, end)) = clip_line_to_box(point, direction, frame.half_u, frame.half_v) else {
        return Vec::new();
    };
    line(start, end, color).into_iter().collect()
}

fn local_square(frame: &PlaneFrame, color: [f32; 4]) -> Vec<MaterialLine> {
    let corners = [
        Vec3::new(-frame.half_u, -frame.half_v, 0.0),
        Vec3::new(frame.half_u, -frame.half_v, 0.0),
        Vec3::new(frame.half_u, frame.half_v, 0.0),
        Vec3::new(-frame.half_u, frame.half_v, 0.0),
    ];
    let mut lines = Vec::with_capacity(4);
    for index in 0..4 {
        if let Some(segment) = line(corners[index], corners[(index + 1) % 4], color) {
            lines.push(segment);
        }
    }
    lines
}

fn clip_line_to_box(
    point: Vec3,
    direction: Vec3,
    half_u: f32,
    half_v: f32,
) -> Option<(Vec3, Vec3)> {
    if !half_u.is_finite() || !half_v.is_finite() || half_u < 0.0 || half_v < 0.0 {
        return None;
    }
    let mut t_min = f32::NEG_INFINITY;
    let mut t_max = f32::INFINITY;
    for (p, q) in [
        (-direction.x, point.x + half_u),
        (direction.x, half_u - point.x),
        (-direction.y, point.y + half_v),
        (direction.y, half_v - point.y),
    ] {
        if p.abs() <= f32::EPSILON {
            if q < 0.0 {
                return None;
            }
        } else {
            let t = q / p;
            if p < 0.0 {
                t_min = t_min.max(t);
            } else {
                t_max = t_max.min(t);
            }
        }
    }
    if t_min > t_max || !t_min.is_finite() || !t_max.is_finite() {
        return None;
    }
    let start = point + direction * t_min;
    let end = point + direction * t_max;
    if !start.is_finite() || !end.is_finite() {
        return None;
    }
    Some((
        Vec3::new(
            start.x.clamp(-half_u, half_u),
            start.y.clamp(-half_v, half_v),
            0.0,
        ),
        Vec3::new(
            end.x.clamp(-half_u, half_u),
            end.y.clamp(-half_v, half_v),
            0.0,
        ),
    ))
}

fn coincident_interface_slice(scene: &MaterialScene, plane: Option<&PlaneFrame>) -> bool {
    let (MaterialCase::DielectricBoundary | MaterialCase::PermeableBoundary, Some(frame)) =
        (scene.case, plane)
    else {
        return false;
    };
    let normal = axis_vector(scene.interface_axis);
    if (normal.dot(frame.normal).abs() - 1.0).abs() > 1e-6 {
        return false;
    }
    (normal.dot(frame.origin) - scene.interface_offset as f32).abs()
        <= (EXACT_HIT_FACTOR * scene.interface_offset.abs().max(1.0)) as f32
}
