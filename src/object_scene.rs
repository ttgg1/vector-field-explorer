//! Static primitive meshes shared by every object instance.
//!
//! A source object is drawn by reusing one of five pre-generated,
//! non-indexed triangle meshes and transforming it per instance. Meshes are
//! built once per process, carry outward unit normals, and use per-triangle
//! barycentric coordinates for wireframe shading. The module is pure geometry:
//! it has no renderer, app, GPU or persistence dependency.

use std::sync::OnceLock;

/// One reusable object body.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MeshPrimitive {
    /// Unit-radius sphere.
    Sphere,
    /// Radius `0.5`, height `1`, centred on `Y`, with both caps.
    Cylinder,
    /// Major radius `0.75`, tube radius `0.25`, centred on the origin.
    Torus,
    /// Helical solenoid body: radius `1`, height `1` on `+Y`, 12 visible turns.
    Tube,
    /// Radius `1`, flat in `XZ` (`Y = 0`), with two-sided normals.
    Disc,
}

impl MeshPrimitive {
    /// Every primitive, in stable index order.
    pub const ALL: [Self; 5] = [
        Self::Sphere,
        Self::Cylinder,
        Self::Torus,
        Self::Tube,
        Self::Disc,
    ];

    /// Number of triangles in this primitive's static mesh.
    #[must_use]
    pub const fn triangles(self) -> usize {
        match self {
            Self::Sphere => 768,
            Self::Cylinder => 96,
            Self::Torus => 768,
            Self::Tube => 1_152,
            Self::Disc => 48,
        }
    }

    /// Number of vertices, three per non-indexed triangle.
    #[must_use]
    pub const fn vertices(self) -> usize {
        self.triangles() * 3
    }

    /// Stable primitive index used by GPU instance sorting and hashing.
    #[must_use]
    pub const fn index(self) -> u32 {
        match self {
            Self::Sphere => 0,
            Self::Cylinder => 1,
            Self::Torus => 2,
            Self::Tube => 3,
            Self::Disc => 4,
        }
    }

    /// The primitive at a stable index, or `None` when out of range.
    #[must_use]
    pub const fn from_index(index: u32) -> Option<Self> {
        match index {
            0 => Some(Self::Sphere),
            1 => Some(Self::Cylinder),
            2 => Some(Self::Torus),
            3 => Some(Self::Tube),
            4 => Some(Self::Disc),
            _ => None,
        }
    }
}

/// One vertex of a static primitive mesh.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MeshVertex {
    /// Object-local position.
    pub position: [f32; 3],
    /// Outward unit normal in object-local space.
    pub normal: [f32; 3],
    /// One-hot corner marker: `[1,0,0]`, `[0,1,0]` or `[0,0,1]`.
    pub barycentric: [f32; 3],
}

/// A cached primitive mesh and its identity.
#[derive(Debug)]
pub struct PrimitiveMesh {
    /// Which body this mesh draws.
    pub primitive: MeshPrimitive,
    /// Non-indexed triangle vertices, three per triangle.
    pub vertices: &'static [MeshVertex],
}

/// Total triangles across all five static meshes.
pub const STATIC_TRIANGLES: usize = 2_832;

/// Total non-indexed vertices across all five static meshes.
pub const STATIC_VERTICES: usize = 8_496;

/// Bytes per [`MeshVertex`]: nine `f32` attributes.
pub const VERTEX_BYTES: usize = 36;

static SPHERE: OnceLock<PrimitiveMesh> = OnceLock::new();
static CYLINDER: OnceLock<PrimitiveMesh> = OnceLock::new();
static TORUS: OnceLock<PrimitiveMesh> = OnceLock::new();
static TUBE: OnceLock<PrimitiveMesh> = OnceLock::new();
static DISC: OnceLock<PrimitiveMesh> = OnceLock::new();

/// The process-wide static mesh for `primitive`.
#[must_use]
pub fn primitive_mesh(primitive: MeshPrimitive) -> &'static PrimitiveMesh {
    match primitive {
        MeshPrimitive::Sphere => SPHERE.get_or_init(|| make(primitive, sphere_mesh())),
        MeshPrimitive::Cylinder => CYLINDER.get_or_init(|| make(primitive, cylinder_mesh())),
        MeshPrimitive::Torus => TORUS.get_or_init(|| make(primitive, torus_mesh())),
        MeshPrimitive::Tube => TUBE.get_or_init(|| make(primitive, tube_mesh())),
        MeshPrimitive::Disc => DISC.get_or_init(|| make(primitive, disc_mesh())),
    }
}

fn make(primitive: MeshPrimitive, vertices: Vec<MeshVertex>) -> PrimitiveMesh {
    let vertices: &'static [MeshVertex] = Box::leak(vertices.into_boxed_slice());
    PrimitiveMesh {
        primitive,
        vertices,
    }
}

/// Append one triangle with fixed per-corner barycentrics.
fn push_triangle(out: &mut Vec<MeshVertex>, positions: [[f32; 3]; 3], normals: [[f32; 3]; 3]) {
    const BARYCENTRIC: [[f32; 3]; 3] = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
    for corner in 0..3 {
        out.push(MeshVertex {
            position: positions[corner],
            normal: normals[corner],
            barycentric: BARYCENTRIC[corner],
        });
    }
}

const SPHERE_SEGMENTS: usize = 24;
const SPHERE_RINGS: usize = 16;

fn sphere_mesh() -> Vec<MeshVertex> {
    let mut out = Vec::with_capacity(MeshPrimitive::Sphere.vertices());
    let mut rings = [[[0.0_f32; 3]; SPHERE_SEGMENTS]; SPHERE_RINGS];
    for (ring_index, ring) in rings.iter_mut().enumerate() {
        let theta = std::f32::consts::PI * (ring_index as f32 + 0.5) / SPHERE_RINGS as f32;
        let (sin_theta, cos_theta) = theta.sin_cos();
        for (segment, point) in ring.iter_mut().enumerate() {
            let phi = std::f32::consts::TAU * segment as f32 / SPHERE_SEGMENTS as f32;
            let (sin_phi, cos_phi) = phi.sin_cos();
            *point = [sin_theta * cos_phi, cos_theta, sin_theta * sin_phi];
        }
    }

    for pair in rings.windows(2) {
        let upper = pair[0];
        let lower = pair[1];
        for segment in 0..SPHERE_SEGMENTS {
            let next = (segment + 1) % SPHERE_SEGMENTS;
            let a = upper[segment];
            let b = upper[next];
            let c = lower[next];
            let d = lower[segment];
            // Normals on a unit sphere point along the position.
            push_triangle(&mut out, [a, b, d], [a, b, d]);
            push_triangle(&mut out, [b, c, d], [b, c, d]);
        }
    }

    let north = [0.0, 1.0, 0.0];
    let south = [0.0, -1.0, 0.0];
    let first = rings[0];
    let last = rings[SPHERE_RINGS - 1];
    for segment in 0..SPHERE_SEGMENTS {
        let next = (segment + 1) % SPHERE_SEGMENTS;
        let top_a = first[segment];
        let top_b = first[next];
        push_triangle(&mut out, [north, top_b, top_a], [north, top_b, top_a]);
        let bottom_a = last[segment];
        let bottom_b = last[next];
        push_triangle(
            &mut out,
            [south, bottom_a, bottom_b],
            [south, bottom_a, bottom_b],
        );
    }

    out
}

const CYLINDER_SIDES: usize = 24;
const CYLINDER_RADIUS: f32 = 0.5;
const CYLINDER_HALF_HEIGHT: f32 = 0.5;

fn cylinder_mesh() -> Vec<MeshVertex> {
    let mut out = Vec::with_capacity(MeshPrimitive::Cylinder.vertices());

    for side in 0..CYLINDER_SIDES {
        let theta = std::f32::consts::TAU * side as f32 / CYLINDER_SIDES as f32;
        let next_theta = std::f32::consts::TAU * (side + 1) as f32 / CYLINDER_SIDES as f32;
        let (sin_theta, cos_theta) = theta.sin_cos();
        let (sin_next, cos_next) = next_theta.sin_cos();
        let normal_a = [cos_theta, 0.0, sin_theta];
        let normal_b = [cos_next, 0.0, sin_next];
        let bottom_a = [
            CYLINDER_RADIUS * cos_theta,
            -CYLINDER_HALF_HEIGHT,
            CYLINDER_RADIUS * sin_theta,
        ];
        let bottom_b = [
            CYLINDER_RADIUS * cos_next,
            -CYLINDER_HALF_HEIGHT,
            CYLINDER_RADIUS * sin_next,
        ];
        let top_a = [
            CYLINDER_RADIUS * cos_theta,
            CYLINDER_HALF_HEIGHT,
            CYLINDER_RADIUS * sin_theta,
        ];
        let top_b = [
            CYLINDER_RADIUS * cos_next,
            CYLINDER_HALF_HEIGHT,
            CYLINDER_RADIUS * sin_next,
        ];
        push_triangle(
            &mut out,
            [bottom_a, top_a, top_b],
            [normal_a, normal_a, normal_b],
        );
        push_triangle(
            &mut out,
            [bottom_a, top_b, bottom_b],
            [normal_a, normal_b, normal_b],
        );
    }

    let top_center = [0.0, CYLINDER_HALF_HEIGHT, 0.0];
    let bottom_center = [0.0, -CYLINDER_HALF_HEIGHT, 0.0];
    for side in 0..CYLINDER_SIDES {
        let theta = std::f32::consts::TAU * side as f32 / CYLINDER_SIDES as f32;
        let next_theta = std::f32::consts::TAU * (side + 1) as f32 / CYLINDER_SIDES as f32;
        let top_rim_a = [
            CYLINDER_RADIUS * theta.cos(),
            CYLINDER_HALF_HEIGHT,
            CYLINDER_RADIUS * theta.sin(),
        ];
        let top_rim_b = [
            CYLINDER_RADIUS * next_theta.cos(),
            CYLINDER_HALF_HEIGHT,
            CYLINDER_RADIUS * next_theta.sin(),
        ];
        let bottom_rim_a = [top_rim_a[0], -CYLINDER_HALF_HEIGHT, top_rim_a[2]];
        let bottom_rim_b = [top_rim_b[0], -CYLINDER_HALF_HEIGHT, top_rim_b[2]];
        // Cap normals are axial, so the fan is flat-shaded.
        push_triangle(
            &mut out,
            [top_center, top_rim_b, top_rim_a],
            [[0.0, 1.0, 0.0]; 3],
        );
        push_triangle(
            &mut out,
            [bottom_center, bottom_rim_a, bottom_rim_b],
            [[0.0, -1.0, 0.0]; 3],
        );
    }

    out
}

const TORUS_MAJOR_SEGMENTS: usize = 32;
const TORUS_TUBE_SEGMENTS: usize = 12;
const TORUS_MAJOR_RADIUS: f32 = 0.75;
const TORUS_MINOR_RADIUS: f32 = 0.25;

fn torus_mesh() -> Vec<MeshVertex> {
    let mut positions = [[[0.0_f32; 3]; TORUS_TUBE_SEGMENTS]; TORUS_MAJOR_SEGMENTS];
    let mut normals = positions;
    for (major, (position_ring, normal_ring)) in
        positions.iter_mut().zip(normals.iter_mut()).enumerate()
    {
        let major_angle = std::f32::consts::TAU * major as f32 / TORUS_MAJOR_SEGMENTS as f32;
        let (sin_major, cos_major) = major_angle.sin_cos();
        for (minor, segment) in position_ring.iter_mut().enumerate() {
            let minor_angle = std::f32::consts::TAU * minor as f32 / TORUS_TUBE_SEGMENTS as f32;
            let (sin_minor, cos_minor) = minor_angle.sin_cos();
            let radial = TORUS_MAJOR_RADIUS + TORUS_MINOR_RADIUS * cos_minor;
            *segment = [
                radial * cos_major,
                TORUS_MINOR_RADIUS * sin_minor,
                radial * sin_major,
            ];
            normal_ring[minor] = [cos_minor * cos_major, sin_minor, cos_minor * sin_major];
        }
    }

    let mut out = Vec::with_capacity(MeshPrimitive::Torus.vertices());
    for major in 0..TORUS_MAJOR_SEGMENTS {
        let next_major = (major + 1) % TORUS_MAJOR_SEGMENTS;
        for minor in 0..TORUS_TUBE_SEGMENTS {
            let next_minor = (minor + 1) % TORUS_TUBE_SEGMENTS;
            let a = positions[major][minor];
            let b = positions[next_major][minor];
            let c = positions[next_major][next_minor];
            let d = positions[major][next_minor];
            let na = normals[major][minor];
            let nb = normals[next_major][minor];
            let nc = normals[next_major][next_minor];
            let nd = normals[major][next_minor];
            push_triangle(&mut out, [a, d, c], [na, nd, nc]);
            push_triangle(&mut out, [a, c, b], [na, nc, nb]);
        }
    }

    out
}

const DISC_SEGMENTS: usize = 24;
const DISC_RADIUS: f32 = 1.0;

fn disc_mesh() -> Vec<MeshVertex> {
    let mut out = Vec::with_capacity(MeshPrimitive::Disc.vertices());
    let center = [0.0, 0.0, 0.0];
    for segment in 0..DISC_SEGMENTS {
        let theta = std::f32::consts::TAU * segment as f32 / DISC_SEGMENTS as f32;
        let next_theta = std::f32::consts::TAU * (segment + 1) as f32 / DISC_SEGMENTS as f32;
        let a = [DISC_RADIUS * theta.cos(), 0.0, DISC_RADIUS * theta.sin()];
        let b = [
            DISC_RADIUS * next_theta.cos(),
            0.0,
            DISC_RADIUS * next_theta.sin(),
        ];
        // One fan per side so both +Y and -Y faces are drawable.
        push_triangle(&mut out, [center, b, a], [[0.0, 1.0, 0.0]; 3]);
        push_triangle(&mut out, [center, a, b], [[0.0, -1.0, 0.0]; 3]);
    }

    out
}

const TUBE_TURNS: usize = 12;
const TUBE_STEPS_PER_TURN: usize = 6;
const TUBE_SIDES: usize = 8;
const TUBE_PATH_RADIUS: f32 = 1.0;
const TUBE_THICKNESS: f32 = 0.05;
const TUBE_HALF_HEIGHT: f32 = 0.5;
const TUBE_PATH_HALF_HEIGHT: f32 = TUBE_HALF_HEIGHT - TUBE_THICKNESS;

fn tube_mesh() -> Vec<MeshVertex> {
    let steps = TUBE_TURNS * TUBE_STEPS_PER_TURN;
    let mut out = Vec::with_capacity(MeshPrimitive::Tube.vertices());

    for step in 0..steps {
        let start = step as f32 / steps as f32;
        let end = (step + 1) as f32 / steps as f32;
        let theta_start = std::f32::consts::TAU * TUBE_TURNS as f32 * start;
        let theta_end = std::f32::consts::TAU * TUBE_TURNS as f32 * end;
        let (sin_start, cos_start) = theta_start.sin_cos();
        let (sin_end, cos_end) = theta_end.sin_cos();
        let center_start = [
            TUBE_PATH_RADIUS * cos_start,
            -TUBE_PATH_HALF_HEIGHT + 2.0 * TUBE_PATH_HALF_HEIGHT * start,
            TUBE_PATH_RADIUS * sin_start,
        ];
        let center_end = [
            TUBE_PATH_RADIUS * cos_end,
            -TUBE_PATH_HALF_HEIGHT + 2.0 * TUBE_PATH_HALF_HEIGHT * end,
            TUBE_PATH_RADIUS * sin_end,
        ];
        let radial_start = [cos_start, 0.0, sin_start];
        let radial_end = [cos_end, 0.0, sin_end];

        for side in 0..TUBE_SIDES {
            let phi = std::f32::consts::TAU * side as f32 / TUBE_SIDES as f32;
            let next_phi = std::f32::consts::TAU * (side + 1) as f32 / TUBE_SIDES as f32;
            let (sin_phi, cos_phi) = phi.sin_cos();
            let (sin_next, cos_next) = next_phi.sin_cos();

            let normal_a = [
                cos_phi * radial_start[0],
                sin_phi,
                cos_phi * radial_start[2],
            ];
            let normal_b = [cos_phi * radial_end[0], sin_phi, cos_phi * radial_end[2]];
            let normal_c = [cos_next * radial_end[0], sin_next, cos_next * radial_end[2]];
            let normal_d = [
                cos_next * radial_start[0],
                sin_next,
                cos_next * radial_start[2],
            ];

            let a = [
                center_start[0] + TUBE_THICKNESS * normal_a[0],
                center_start[1] + TUBE_THICKNESS * normal_a[1],
                center_start[2] + TUBE_THICKNESS * normal_a[2],
            ];
            let b = [
                center_end[0] + TUBE_THICKNESS * normal_b[0],
                center_end[1] + TUBE_THICKNESS * normal_b[1],
                center_end[2] + TUBE_THICKNESS * normal_b[2],
            ];
            let c = [
                center_end[0] + TUBE_THICKNESS * normal_c[0],
                center_end[1] + TUBE_THICKNESS * normal_c[1],
                center_end[2] + TUBE_THICKNESS * normal_c[2],
            ];
            let d = [
                center_start[0] + TUBE_THICKNESS * normal_d[0],
                center_start[1] + TUBE_THICKNESS * normal_d[1],
                center_start[2] + TUBE_THICKNESS * normal_d[2],
            ];

            push_triangle(&mut out, [a, d, c], [normal_a, normal_d, normal_c]);
            push_triangle(&mut out, [a, c, b], [normal_a, normal_c, normal_b]);
        }
    }

    out
}
