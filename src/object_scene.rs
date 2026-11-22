//! Static primitive meshes shared by every object instance.
//!
//! A source object is drawn by reusing one of five pre-generated,
//! non-indexed triangle meshes and transforming it per instance. Meshes are
//! built once per process, carry outward unit normals, and use per-triangle
//! barycentric coordinates for wireframe shading. The module is pure geometry:
//! it has no renderer, app, GPU or persistence dependency.

use std::sync::OnceLock;

use serde::{Deserialize, Deserializer, Serialize};

/// How mapped source/induction bodies are drawn, persisted with the settings.
///
/// The two stable storage tokens are `"Solid"` and `"Wireframe"`. A missing
/// field loads [`ObjectStyle::Solid`]; an unknown stored name is repaired to
/// `Solid` with one warning, without failing the rest of the envelope.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize)]
pub enum ObjectStyle {
    /// Filled, lit bodies (the shipped default).
    #[default]
    Solid,
    /// Barycentric edge outlines over the same bodies.
    Wireframe,
}

impl ObjectStyle {
    /// Whether this style selects the shader's wireframe path.
    #[must_use]
    pub const fn is_wireframe(self) -> bool {
        matches!(self, Self::Wireframe)
    }
}

impl<'de> Deserialize<'de> for ObjectStyle {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_enum("ObjectStyle", &["Solid", "Wireframe"], ObjectStyleVisitor)
    }
}

/// The stored variant name, decoded without rejecting an unknown value.
struct ObjectStyleName(String);

impl<'de> Deserialize<'de> for ObjectStyleName {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        use serde::de::Visitor;

        struct NameVisitor;

        impl Visitor<'_> for NameVisitor {
            type Value = ObjectStyleName;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("an object style name")
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                Ok(ObjectStyleName(value.to_owned()))
            }
        }

        deserializer.deserialize_identifier(NameVisitor)
    }
}

/// Decode the unit variant, repairing an unknown token to [`ObjectStyle::Solid`]
/// with exactly one warning.
struct ObjectStyleVisitor;

impl<'de> serde::de::Visitor<'de> for ObjectStyleVisitor {
    type Value = ObjectStyle;

    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("Solid or Wireframe")
    }

    fn visit_enum<A>(self, data: A) -> Result<Self::Value, A::Error>
    where
        A: serde::de::EnumAccess<'de>,
    {
        use serde::de::VariantAccess;

        let (name, variant) = data.variant::<ObjectStyleName>()?;
        let style = match name.0.as_str() {
            "Solid" => ObjectStyle::Solid,
            "Wireframe" => ObjectStyle::Wireframe,
            other => {
                log::warn!("persisted object style {other:?} is unknown; using the Solid default");
                ObjectStyle::Solid
            }
        };
        variant.unit_variant()?;
        Ok(style)
    }
}

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

use glam::Vec3;

use crate::domain::Domain;
use crate::material::{
    MATERIAL_ELECTRIC_COLOR, MATERIAL_MAGNETIC_COLOR, MaterialCase, MaterialFamily, MaterialScene,
    material_family,
};
use crate::maxwell::{ConductorOutline, MaxwellCase, MaxwellFamily, MaxwellScene, maxwell_family};
use crate::plane::{PlaneAxis, PlaneFrame};
use crate::source_scene::{ELECTRIC_COLOR, MAGNETIC_COLOR, SourceDisplay, SourceScene};
use crate::sources::{FieldFamily, SourceKind};

/// Hard cap on instances emitted by one [`MeshScene`].
pub const MAX_OBJECTS: usize = 64;

/// Hard cap on triangles drawn by one [`MeshScene`].
pub const MAX_DRAWN_TRIANGLES: usize = 150_000;

/// Smallest domain extent used to derive object dimensions.
const MIN_EXTENT: f32 = 1e-6;

/// One instanced object ready for the renderer.
///
/// `transform` is column-major: columns 0..=2 are the scaled right, axis and
/// forward basis vectors and column 3 is the world translation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MeshObject {
    /// Static body drawn at this instance.
    pub primitive: MeshPrimitive,
    /// Column-major model-to-world transform.
    pub transform: [[f32; 4]; 4],
    /// Linear sRGB colour in `0..=1`.
    pub color: [f32; 3],
    /// Opacity, always `1.0` for sources.
    pub opacity: f32,
    /// `-1` for a negative charge/current, otherwise `1`.
    pub sign: i8,
}

/// The clip plane that cuts object bodies on a 2D slice.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SliceClip {
    /// World-space unit normal facing the visible half-space.
    ///
    /// Taken as unit because [`PlaneFrame::new`] builds it from the
    /// cross product of two unit in-plane axes. Consumers must not assume
    /// this for a hand-built or foreign frame.
    pub normal: [f32; 3],
    /// Plane offset: `normal.dot(origin)`.
    pub offset: f32,
}

/// A mapped source scene plus its content fingerprint.
#[derive(Debug, Clone, PartialEq)]
pub struct MeshScene {
    objects: Vec<MeshObject>,
    generation: u64,
    clip: Option<SliceClip>,
}

impl MeshScene {
    /// An empty scene. Its generation is reserved (`0`) and never collides
    /// with a mapped scene.
    #[must_use]
    pub fn empty() -> Self {
        Self {
            objects: Vec::new(),
            generation: 0,
            clip: None,
        }
    }

    /// Map every visible, enabled source entry to one instance.
    ///
    /// Entries with a non-finite centre are skipped. Display filtering follows
    /// [`SourceDisplay`]; disabled entries emit nothing. At most the first
    /// [`MAX_OBJECTS`] instances that fit [`MAX_DRAWN_TRIANGLES`] are kept in
    /// entry order; both truncations drop the tail, never an earlier entry.
    /// An enabled set that yields no objects still maps the plane, so such a
    /// scene has `generation == 0` but `clip: Some(_)`.
    #[must_use]
    pub fn from_sources(scene: &SourceScene, domain: &Domain, plane: Option<&PlaneFrame>) -> Self {
        let extent = scene_extent(domain);
        let unit = 0.05 * extent;
        let mut objects = Vec::new();
        let mut drawn = 0usize;

        for entry in scene.entries.iter() {
            if !entry.enabled || !display_includes(scene.display, entry.source.kind.family()) {
                continue;
            }
            if objects.len() >= MAX_OBJECTS {
                break;
            }
            if !entry.source.center.is_finite() {
                continue;
            }
            let (primitive, scale) = object_mapping(entry.source.kind, extent, unit);
            let triangles = primitive.triangles();
            if drawn.saturating_add(triangles) > MAX_DRAWN_TRIANGLES {
                break;
            }
            objects.push(MeshObject {
                primitive,
                transform: object_transform(
                    entry.source.center.as_vec3(),
                    entry.source.axis.as_vec3(),
                    scale,
                ),
                color: family_color(entry.source.kind.family()),
                opacity: 1.0,
                sign: source_sign(entry.source.kind),
            });
            drawn += triangles;
        }

        let generation = fingerprint(&objects);

        Self {
            objects,
            generation,
            clip: slice_clip(plane),
        }
    }

    /// Map one bounded Materials case to a faithful solid body.
    ///
    /// A sphere case is a filled [`MeshPrimitive::Sphere`] at the material
    /// radius. A planar-boundary case is a thin [`MeshPrimitive::Disc`] on the
    /// interface plane: it marks the interface, never a filled half-space.
    /// Family colour and unit opacity match the source mapping; `plane` adds the
    /// same [`SliceClip`] the source mapping uses. A sphere body depends only on
    /// its radius, so 2D and 3D share one generation for the same scene; a
    /// boundary disc is domain-derived, so a changed domain rebuilds it.
    #[must_use]
    pub fn from_material(
        scene: &MaterialScene,
        domain: &Domain,
        plane: Option<&PlaneFrame>,
    ) -> Self {
        let extent = scene_extent(domain);
        let color = material_color(scene.case);
        let bodies = match scene.case {
            MaterialCase::DielectricSphere | MaterialCase::PermeableSphere => {
                let radius = positive_dimension(scene.radius, 0.05 * extent);
                vec![body(
                    MeshPrimitive::Sphere,
                    Vec3::ZERO,
                    Vec3::Y,
                    Vec3::splat(radius),
                    color,
                )]
            }
            MaterialCase::DielectricBoundary | MaterialCase::PermeableBoundary => {
                let size = domain.domain_size();
                let (u_extent, v_extent) = match scene.interface_axis {
                    PlaneAxis::X => (size.y, size.z),
                    PlaneAxis::Y => (size.x, size.z),
                    PlaneAxis::Z => (size.x, size.y),
                };
                let half = 0.55 * u_extent.abs().max(v_extent.abs());
                let radius = if half.is_finite() && half > MIN_EXTENT {
                    half
                } else {
                    0.5 * extent
                };
                let mut center = domain.center();
                let normal = axis_vector(scene.interface_axis);
                match scene.interface_axis {
                    PlaneAxis::X => center.x = scene.interface_offset as f32,
                    PlaneAxis::Y => center.y = scene.interface_offset as f32,
                    PlaneAxis::Z => center.z = scene.interface_offset as f32,
                }
                vec![body(
                    MeshPrimitive::Disc,
                    center,
                    normal,
                    Vec3::new(radius, 0.02 * extent, radius),
                    color,
                )]
            }
        };
        Self::from_bodies(bodies, plane)
    }

    /// Map one bounded Maxwell case to the bodies its geometry actually names.
    ///
    /// Plates are thin discs at their separation, coaxial cases fill the inner
    /// conductor, the wire and solenoid use a cylinder/tube along `Z`, the image
    /// sphere is a conductor and the image plane is the real charge plus a thin
    /// grounded disc. Bodies with a domain-centred outline use
    /// [`Domain::center`] on the same axes as that outline. Thin shells and the
    /// square conductor have no faithful filled primitive, so they stay
    /// outline-only; see [`maxwell_style_note`].
    #[must_use]
    pub fn from_maxwell(scene: &MaxwellScene, domain: &Domain, plane: Option<&PlaneFrame>) -> Self {
        let extent = scene_extent(domain);
        let size = domain.domain_size();
        let center = domain.center();
        let thin = 0.02 * extent;
        let color = maxwell_color(scene.case);
        let bodies = match scene.case {
            MaxwellCase::ParallelPlates => {
                let separation = positive_dimension(scene.separation, extent);
                let radius = 0.55 * extent;
                let mut bodies = Vec::with_capacity(2);
                for sign in [-1.0_f32, 1.0] {
                    bodies.push(body(
                        MeshPrimitive::Disc,
                        Vec3::new(center.x, center.y, sign * 0.5 * separation),
                        Vec3::Z,
                        Vec3::new(radius, thin, radius),
                        color,
                    ));
                }
                bodies
            }
            MaxwellCase::CoaxialElectric | MaxwellCase::CoaxialMagnetic => {
                let radius = positive_dimension(scene.radius, 0.05 * extent);
                vec![cylinder_along_z(
                    Vec3::new(0.0, 0.0, center.z),
                    radius,
                    axial_length(size, extent),
                    color,
                )]
            }
            MaxwellCase::HollowSphere => Vec::new(),
            MaxwellCase::InfiniteWire => {
                let radius = positive_dimension(scene.radius, 0.05 * extent);
                vec![cylinder_along_z(
                    Vec3::new(0.0, 0.0, center.z),
                    radius,
                    axial_length(size, extent),
                    color,
                )]
            }
            MaxwellCase::Solenoid => {
                let radius = positive_dimension(scene.radius, 0.05 * extent);
                vec![body(
                    MeshPrimitive::Tube,
                    Vec3::new(0.0, 0.0, center.z),
                    Vec3::Z,
                    Vec3::new(radius, axial_length(size, extent), radius),
                    color,
                )]
            }
            MaxwellCase::ImageSphere => {
                let radius = positive_dimension(scene.radius, 0.05 * extent);
                vec![body(
                    MeshPrimitive::Sphere,
                    Vec3::ZERO,
                    Vec3::Y,
                    Vec3::splat(radius),
                    color,
                )]
            }
            MaxwellCase::ImagePlane => {
                let height = positive_dimension(scene.charge_height, extent);
                let plane_radius = 0.55 * extent;
                vec![
                    body(
                        MeshPrimitive::Sphere,
                        Vec3::new(0.0, 0.0, height),
                        Vec3::Y,
                        Vec3::splat(0.05 * extent),
                        color,
                    ),
                    body(
                        MeshPrimitive::Disc,
                        Vec3::new(center.x, center.y, 0.0),
                        Vec3::Z,
                        Vec3::new(plane_radius, thin, plane_radius),
                        color,
                    ),
                ]
            }
            MaxwellCase::Conductor2D => match (plane, scene.outline) {
                (Some(frame), ConductorOutline::Circle { radius }) => {
                    let radius = positive_dimension(radius, 0.05 * extent);
                    let local_center = frame.to_local(Vec3::ZERO);
                    let center = frame.to_world(Vec3::new(local_center.x, local_center.y, 0.0));
                    vec![body(
                        MeshPrimitive::Cylinder,
                        center,
                        frame.normal,
                        Vec3::new(radius / 0.5, 0.1 * extent, radius / 0.5),
                        color,
                    )]
                }
                // The square has no faithful box primitive and the circle is
                // undefined in 3D; both keep the existing outline.
                _ => Vec::new(),
            },
        };
        Self::from_bodies(bodies, plane)
    }

    /// Assemble bodies with the shared caps, slice clip and fingerprint.
    fn from_bodies(bodies: Vec<MeshObject>, plane: Option<&PlaneFrame>) -> Self {
        let mut objects = Vec::with_capacity(bodies.len().min(MAX_OBJECTS));
        let mut drawn = 0usize;
        for body in bodies {
            if objects.len() >= MAX_OBJECTS {
                break;
            }
            let triangles = body.primitive.triangles();
            if drawn.saturating_add(triangles) > MAX_DRAWN_TRIANGLES {
                break;
            }
            drawn += triangles;
            objects.push(body);
        }
        let generation = fingerprint(&objects);
        Self {
            objects,
            generation,
            clip: slice_clip(plane),
        }
    }

    /// The mapped instances, in deterministic source-entry order.
    #[must_use]
    pub fn objects(&self) -> &[MeshObject] {
        &self.objects
    }

    /// Triangles this scene draws across all instances.
    #[must_use]
    pub fn drawn_triangles(&self) -> usize {
        self.objects
            .iter()
            .map(|object| object.primitive.triangles())
            .sum()
    }

    /// Content fingerprint: stable for identical scenes and changed by any
    /// mapped input.
    #[must_use]
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// The slice clip plane, when mapped from a [`PlaneFrame`].
    #[must_use]
    pub fn clip(&self) -> Option<SliceClip> {
        self.clip
    }

    /// Whether a cached scene covering this content can stand in for `next`.
    ///
    /// The cache key is [`Self::generation`] plus the slice [`Self::clip`]:
    /// the fingerprint deliberately omits the clip, so the 2D and 3D mapping
    /// of the same sources share one generation even though only 2D is cut.
    #[must_use]
    pub fn cache_matches(&self, next: &Self) -> bool {
        self.generation == next.generation && self.clip == next.clip
    }
}

/// Build a column-major model transform whose local `+Y` follows `axis`.
///
/// Columns are the scaled `right`, `axis` and `forward` basis vectors; the
/// translation is the last column. A zero, non-finite or unnormalizable axis
/// falls back to `Vec3::Z`. Non-finite centres and non-positive or non-finite
/// scales are clamped so the result is always finite with a positive
/// determinant.
#[must_use]
pub fn object_transform(center: Vec3, axis: Vec3, scale: Vec3) -> [[f32; 4]; 4] {
    let center = if center.is_finite() {
        center
    } else {
        Vec3::ZERO
    };
    let normal = normalized_axis(axis);
    let helper = if normal.x.abs() < 0.9 {
        Vec3::X
    } else {
        Vec3::Y
    };
    let right = finite_normalize(normal.cross(helper), Vec3::Y);
    let forward = right.cross(normal);
    let scale = Vec3::new(
        sanitize_scale(scale.x),
        sanitize_scale(scale.y),
        sanitize_scale(scale.z),
    );
    let right = right * scale.x;
    let normal = normal * scale.y;
    let forward = forward * scale.z;

    [
        [right.x, right.y, right.z, 0.0],
        [normal.x, normal.y, normal.z, 0.0],
        [forward.x, forward.y, forward.z, 0.0],
        [center.x, center.y, center.z, 1.0],
    ]
}

fn normalized_axis(axis: Vec3) -> Vec3 {
    finite_normalize(axis, Vec3::Z)
}

fn finite_normalize(value: Vec3, fallback: Vec3) -> Vec3 {
    if value.is_finite() && value.length_squared() > 0.0 {
        let normalized = value.normalize();
        if normalized.is_finite() {
            return normalized;
        }
    }
    fallback
}

fn sanitize_scale(value: f32) -> f32 {
    if value.is_finite() && value > 0.0 {
        value
    } else if value.is_finite() {
        MIN_EXTENT
    } else {
        1.0
    }
}

fn scene_extent(domain: &Domain) -> f32 {
    let size = domain.domain_size();
    let extent = size.x.max(size.y).max(size.z);
    if extent.is_finite() && extent > MIN_EXTENT {
        extent
    } else {
        MIN_EXTENT
    }
}

fn display_includes(display: SourceDisplay, family: FieldFamily) -> bool {
    matches!(
        (display, family),
        (SourceDisplay::Both, _)
            | (SourceDisplay::Electric, FieldFamily::Electric)
            | (SourceDisplay::Magnetic, FieldFamily::Magnetic)
    )
}

fn family_color(family: FieldFamily) -> [f32; 3] {
    let [red, green, blue] = match family {
        FieldFamily::Electric => ELECTRIC_COLOR,
        FieldFamily::Magnetic => MAGNETIC_COLOR,
    };
    [
        f32::from(red) / 255.0,
        f32::from(green) / 255.0,
        f32::from(blue) / 255.0,
    ]
}

fn source_sign(kind: SourceKind) -> i8 {
    let strength = match kind {
        SourceKind::PointCharge { charge }
        | SourceKind::FiniteLineCharge { charge, .. }
        | SourceKind::ChargedRing { charge, .. }
        | SourceKind::ChargedDisc { charge, .. }
        | SourceKind::SolidSphere { charge, .. } => charge,
        SourceKind::StraightWire { current }
        | SourceKind::CurrentLoop { current, .. }
        | SourceKind::Solenoid { current, .. } => current,
    };
    // A NaN strength fails every comparison, including `< 0.0`, so it maps to
    // `+1`; only a finite negative value flips the sign.
    if strength < 0.0 { -1 } else { 1 }
}

fn positive_dimension(value: f64, fallback: f32) -> f32 {
    let value = value as f32;
    if value.is_finite() && value > 0.0 {
        value
    } else {
        fallback
    }
}

fn slice_clip(plane: Option<&PlaneFrame>) -> Option<SliceClip> {
    plane.map(|frame| SliceClip {
        normal: [frame.normal.x, frame.normal.y, frame.normal.z],
        offset: frame.normal.dot(frame.origin),
    })
}

/// One unit-opacity body at `center` with local `+Y` along `axis`.
fn body(
    primitive: MeshPrimitive,
    center: Vec3,
    axis: Vec3,
    scale: Vec3,
    color: [f32; 3],
) -> MeshObject {
    MeshObject {
        primitive,
        transform: object_transform(center, axis, scale),
        color,
        opacity: 1.0,
        sign: 1,
    }
}

/// A `Y`-axial cylinder scaled to a radius and a `Z` length, centred at `center`.
fn cylinder_along_z(center: Vec3, radius: f32, length: f32, color: [f32; 3]) -> MeshObject {
    body(
        MeshPrimitive::Cylinder,
        center,
        Vec3::Z,
        Vec3::new(radius / 0.5, length, radius / 0.5),
        color,
    )
}

/// The conductor spans the visible domain along its axis, with an extent floor.
fn axial_length(size: Vec3, extent: f32) -> f32 {
    let value = size.z.abs();
    if value.is_finite() && value > MIN_EXTENT {
        value
    } else {
        extent
    }
}

fn axis_vector(axis: PlaneAxis) -> Vec3 {
    match axis {
        PlaneAxis::X => Vec3::X,
        PlaneAxis::Y => Vec3::Y,
        PlaneAxis::Z => Vec3::Z,
    }
}

fn color_from_rgb([red, green, blue]: [u8; 3]) -> [f32; 3] {
    [
        f32::from(red) / 255.0,
        f32::from(green) / 255.0,
        f32::from(blue) / 255.0,
    ]
}

fn material_color(case: MaterialCase) -> [f32; 3] {
    color_from_rgb(match material_family(case) {
        MaterialFamily::Electric => MATERIAL_ELECTRIC_COLOR,
        MaterialFamily::Magnetic => MATERIAL_MAGNETIC_COLOR,
    })
}

fn maxwell_color(case: MaxwellCase) -> [f32; 3] {
    color_from_rgb(match maxwell_family(case) {
        MaxwellFamily::Electric => ELECTRIC_COLOR,
        MaxwellFamily::Magnetic => MAGNETIC_COLOR,
    })
}

/// Why a Maxwell case draws no faithful filled body, if it does not.
///
/// `None` means the mapped scene is faithful in both styles. Unsupported cases
/// keep the existing `maxwell_outline` geometry; the panel shows this text next
/// to the style selector so an outline-only case is never mistaken for a bug.
#[must_use]
pub fn maxwell_style_note(
    scene: &MaxwellScene,
    plane: Option<&PlaneFrame>,
) -> Option<&'static str> {
    match scene.case {
        MaxwellCase::HollowSphere => Some(
            "Hollow sphere is an ideal thin shell; Solid keeps its outline instead of a filled conductor.",
        ),
        MaxwellCase::CoaxialElectric | MaxwellCase::CoaxialMagnetic => Some(
            "The outer conductor is a thin shell; only the inner conductor is drawn as a filled body.",
        ),
        MaxwellCase::ImageSphere => {
            Some("The image charge is a construction; only the real conducting sphere is drawn.")
        }
        MaxwellCase::ImagePlane => Some(
            "The image charge is a construction; only the real charge and the grounded plane are drawn.",
        ),
        MaxwellCase::Conductor2D => match (&scene.outline, plane) {
            (ConductorOutline::Square { .. }, _) => Some(
                "A square conductor cross-section has no faithful box primitive; only its outline is drawn.",
            ),
            (ConductorOutline::Circle { .. }, None) => {
                Some("Conductor2D is defined on a 2D slice; switch to 2D to see its cross-section.")
            }
            (ConductorOutline::Circle { .. }, Some(_)) => None,
        },
        MaxwellCase::ParallelPlates | MaxwellCase::InfiniteWire | MaxwellCase::Solenoid => None,
    }
}

/// Primitive and local-axis scale for one source kind.
///
/// The primitive meshes are `Y`-axial, so `scale.y` is the length along the
/// source axis and `scale.x`/`scale.z` are radial. Ring and current-loop bodies
/// use the torus primitive (major `0.75`, minor `0.25`), scaled so the major
/// radius and the vertical minor radius are set independently:
/// `scale.x = radius / 0.75` gives a radial minor radius of `radius / 3`, while
/// `scale.y = (0.02 * extent) / 0.25` gives a vertical minor radius of exactly
/// `0.02 * extent`. A single per-instance linear transform cannot vary the
/// minor radius with direction, so the tube reads proportionally thicker
/// radially as the major radius grows.
fn object_mapping(kind: SourceKind, extent: f32, unit: f32) -> (MeshPrimitive, Vec3) {
    let thin = 0.02 * extent;
    match kind {
        SourceKind::PointCharge { .. } => (MeshPrimitive::Sphere, Vec3::splat(unit)),
        SourceKind::FiniteLineCharge { length, .. } => {
            let radius = 0.02 * extent;
            (
                MeshPrimitive::Cylinder,
                Vec3::new(
                    radius / 0.5,
                    positive_dimension(length, extent),
                    radius / 0.5,
                ),
            )
        }
        SourceKind::ChargedRing { radius, .. } => {
            let radius = positive_dimension(radius, unit);
            (
                MeshPrimitive::Torus,
                Vec3::new(radius / 0.75, thin / 0.25, radius / 0.75),
            )
        }
        SourceKind::ChargedDisc { radius, .. } => {
            let radius = positive_dimension(radius, unit);
            // The disc is a zero-thickness fan at local `Y = 0`, so `scale.y`
            // has no geometric effect; keep `thin` as the reserved value the
            // shared mapping table names rather than a physical thickness.
            (MeshPrimitive::Disc, Vec3::new(radius, thin, radius))
        }
        SourceKind::SolidSphere { radius, .. } => {
            let radius = positive_dimension(radius, unit);
            (MeshPrimitive::Sphere, Vec3::splat(radius))
        }
        SourceKind::StraightWire { .. } => {
            let radius = 0.012 * extent;
            let length = 2.2 * extent;
            (
                MeshPrimitive::Cylinder,
                Vec3::new(radius / 0.5, length, radius / 0.5),
            )
        }
        SourceKind::CurrentLoop { radius, .. } => {
            let radius = positive_dimension(radius, unit);
            (
                MeshPrimitive::Torus,
                Vec3::new(radius / 0.75, thin / 0.25, radius / 0.75),
            )
        }
        SourceKind::Solenoid { radius, length, .. } => {
            let radius = positive_dimension(radius, unit);
            let length = positive_dimension(length, extent);
            (MeshPrimitive::Tube, Vec3::new(radius, length, radius))
        }
    }
}

/// FNV-1a over every mapped instance field. The empty scene reserves `0`.
fn fingerprint(objects: &[MeshObject]) -> u64 {
    if objects.is_empty() {
        return 0;
    }
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for object in objects {
        hash_u32(&mut hash, object.primitive.index());
        hash_u32(&mut hash, u32::from(object.sign as u8));
        hash_u32(&mut hash, object.opacity.to_bits());
        for channel in object.color {
            hash_u32(&mut hash, channel.to_bits());
        }
        for column in object.transform {
            for value in column {
                hash_u32(&mut hash, value.to_bits());
            }
        }
    }
    if hash == 0 { 1 } else { hash }
}

fn hash_u32(hash: &mut u64, value: u32) {
    for byte in value.to_le_bytes() {
        *hash ^= u64::from(byte);
        *hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
}
