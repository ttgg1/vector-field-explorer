//! Pure source-marker glyph model and line geometry.
//!
//! This module turns the validated [`SourceScene`] into drawable marker
//! semantics: kind, family, signed strength, centre and axis. It never samples
//! the field. [`source_glyph_lines`] expands those markers into ordinary
//! [`LineInstanceGPU`] segments for the existing line pass, including the
//! projected/off-slice treatment of a selected 2D plane.

use glam::Vec3;

use crate::domain::Domain;
use crate::plane::PlaneFrame;
use crate::renderer::LineInstanceGPU;
use crate::source_scene::{
    ELECTRIC_COLOR, MAGNETIC_COLOR, SourceDisplay, SourceKindTag, SourceScene,
};
use crate::sources::{FieldFamily, SourceKind};

const ON_SLICE_ALPHA: f32 = 0.9;
const OFF_SLICE_ALPHA: f32 = 0.45;
const ON_SLICE_WIDTH: f32 = 0.005;
const OFF_SLICE_WIDTH: f32 = 0.0035;
const DASH_ON: f32 = 4.0;
const DASH_OFF: f32 = 3.0;
const CIRCLE_SEGMENTS: usize = 32;
const MAX_VISIBLE_TURNS: u32 = 12;
const HELIX_SEGMENTS_PER_TURN: usize = 16;
const MIN_EXTENT: f32 = 1e-6;

/// The sign of a source's charge or current.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GlyphSign {
    Positive,
    Negative,
}

/// One pure, drawable description of an enabled source.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SourceGlyph {
    /// Index of the entry in the source scene's list.
    pub index: usize,
    pub kind: SourceKindTag,
    pub family: FieldFamily,
    pub sign: GlyphSign,
    /// World position in 3D, or local plane coordinates in 2D.
    pub center: Vec3,
    /// World axis in 3D, or the projected, normalized in-plane axis in 2D.
    pub axis: Vec3,
    pub radius: f32,
    pub length: f32,
    pub turns: u32,
    pub domain_scale: f32,
    /// 2D only: the source lies off the selected plane.
    pub off_slice: bool,
}

/// Build one glyph per visible and enabled scene entry.
///
/// In 3D the glyph keeps the source's world centre and axis. With a plane, the
/// centre and axis are converted into the plane's local frame, the centre is
/// projected onto the slice, and sources farther from the slice than the
/// representable `f32` tolerance are marked [`SourceGlyph::off_slice`].
pub fn source_glyphs(
    scene: &SourceScene,
    domain: &Domain,
    plane: Option<&PlaneFrame>,
) -> Vec<SourceGlyph> {
    let extent = scene_extent(domain);
    let domain_scale = 0.05 * extent;
    let tolerance = 16.0 * f32::EPSILON * extent.max(1.0);
    let mut glyphs = Vec::new();

    for (index, entry) in scene.entries.iter().enumerate() {
        if !entry.enabled {
            continue;
        }
        let kind = SourceKindTag::of(&entry.source.kind);
        let family = kind.family();
        if !display_includes(scene.display, family) {
            continue;
        }

        let (center, axis, off_slice) = match plane {
            Some(frame) => {
                let local = frame.to_local(finite_vec3(entry.source.center.as_vec3()));
                let projected = frame.project(entry.source.axis.as_vec3());
                (
                    Vec3::new(local.x, local.y, 0.0),
                    normalized_axis(projected),
                    local.z.abs() > tolerance,
                )
            }
            None => (
                finite_vec3(entry.source.center.as_vec3()),
                normalized_axis(entry.source.axis.as_vec3()),
                false,
            ),
        };

        let (radius, length, turns) = glyph_dimensions(&entry.source.kind);
        glyphs.push(SourceGlyph {
            index,
            kind,
            family,
            sign: glyph_sign(&entry.source.kind),
            center,
            axis,
            radius,
            length,
            turns,
            domain_scale,
            off_slice,
        });
    }

    glyphs
}

/// Expand glyphs into line instances for the existing line pass.
pub fn source_glyph_lines(glyphs: &[SourceGlyph], domain: &Domain) -> Vec<LineInstanceGPU> {
    let extent = scene_extent(domain);
    let on_width = ON_SLICE_WIDTH * extent;
    let off_width = OFF_SLICE_WIDTH * extent;
    let mut lines = Vec::new();

    for glyph in glyphs {
        let alpha = if glyph.off_slice {
            OFF_SLICE_ALPHA
        } else {
            ON_SLICE_ALPHA
        };
        let mut emitter = LineEmitter {
            lines: &mut lines,
            color: family_color(glyph.family, alpha),
            width: if glyph.off_slice { off_width } else { on_width },
            dash: glyph
                .off_slice
                .then_some((DASH_ON * glyph.domain_scale, DASH_OFF * glyph.domain_scale)),
        };
        let basis = Basis::new(glyph.axis);

        match glyph.kind {
            SourceKindTag::PointCharge => emit_point_cross(glyph, &basis, &mut emitter),
            SourceKindTag::FiniteLineCharge => emit_finite_line(glyph, &basis, &mut emitter),
            SourceKindTag::ChargedRing => {
                emit_circle(
                    glyph.center,
                    basis.right,
                    basis.up,
                    glyph.radius,
                    &mut emitter,
                );
            }
            SourceKindTag::ChargedDisc => {
                emit_disc(glyph, &basis, &mut emitter);
            }
            SourceKindTag::SolidSphere => emit_sphere(glyph, &basis, &mut emitter),
            SourceKindTag::StraightWire => emit_wire(glyph, &basis, extent, &mut emitter),
            SourceKindTag::CurrentLoop => emit_current_loop(glyph, &basis, &mut emitter),
            SourceKindTag::Solenoid => emit_solenoid(glyph, &basis, &mut emitter),
        }

        emit_sign_marker(glyph, &basis, &mut emitter);
    }

    lines
}

fn display_includes(display: SourceDisplay, family: FieldFamily) -> bool {
    matches!(
        (display, family),
        (SourceDisplay::Both, _)
            | (SourceDisplay::Electric, FieldFamily::Electric)
            | (SourceDisplay::Magnetic, FieldFamily::Magnetic)
    )
}

fn signed_strength(kind: &SourceKind) -> f64 {
    match *kind {
        SourceKind::PointCharge { charge }
        | SourceKind::FiniteLineCharge { charge, .. }
        | SourceKind::ChargedRing { charge, .. }
        | SourceKind::ChargedDisc { charge, .. }
        | SourceKind::SolidSphere { charge, .. } => charge,
        SourceKind::StraightWire { current }
        | SourceKind::CurrentLoop { current, .. }
        | SourceKind::Solenoid { current, .. } => current,
    }
}

fn glyph_sign(kind: &SourceKind) -> GlyphSign {
    if signed_strength(kind) < 0.0 {
        GlyphSign::Negative
    } else {
        GlyphSign::Positive
    }
}

fn sign_factor(sign: GlyphSign) -> f32 {
    match sign {
        GlyphSign::Positive => 1.0,
        GlyphSign::Negative => -1.0,
    }
}

fn glyph_dimensions(kind: &SourceKind) -> (f32, f32, u32) {
    match *kind {
        SourceKind::FiniteLineCharge { length, .. } => (0.0, finite_positive(length), 0),
        SourceKind::ChargedRing { radius, .. }
        | SourceKind::ChargedDisc { radius, .. }
        | SourceKind::SolidSphere { radius, .. }
        | SourceKind::CurrentLoop { radius, .. } => (finite_positive(radius), 0.0, 0),
        SourceKind::Solenoid {
            radius,
            length,
            turns,
            ..
        } => (finite_positive(radius), finite_positive(length), turns),
        SourceKind::PointCharge { .. } | SourceKind::StraightWire { .. } => (0.0, 0.0, 0),
    }
}

fn finite_positive(value: f64) -> f32 {
    let value = value as f32;
    if value.is_finite() && value > 0.0 {
        value
    } else {
        0.0
    }
}

fn finite_vec3(value: Vec3) -> Vec3 {
    if value.is_finite() { value } else { Vec3::ZERO }
}

fn normalized_axis(axis: Vec3) -> Vec3 {
    if axis.is_finite() && axis.length_squared() > 0.0 {
        axis.normalize()
    } else {
        Vec3::Z
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

fn family_color(family: FieldFamily, alpha: f32) -> [f32; 4] {
    let [red, green, blue] = match family {
        FieldFamily::Electric => ELECTRIC_COLOR,
        FieldFamily::Magnetic => MAGNETIC_COLOR,
    };
    [
        red as f32 / 255.0,
        green as f32 / 255.0,
        blue as f32 / 255.0,
        alpha,
    ]
}

/// A right-handed local frame around the glyph's axis.
struct Basis {
    right: Vec3,
    up: Vec3,
    normal: Vec3,
}

impl Basis {
    fn new(axis: Vec3) -> Self {
        let normal = normalized_axis(axis);
        let helper = if normal.x.abs() < 0.9 {
            Vec3::X
        } else {
            Vec3::Y
        };
        let right = normal.cross(helper);
        let right = if right.is_finite() && right.length_squared() > 0.0 {
            right.normalize()
        } else {
            Vec3::Y
        };
        let up = normal.cross(right);
        Self { right, up, normal }
    }
}

struct LineEmitter<'a> {
    lines: &'a mut Vec<LineInstanceGPU>,
    color: [f32; 4],
    width: f32,
    dash: Option<(f32, f32)>,
}

impl LineEmitter<'_> {
    fn segment(&mut self, a: Vec3, b: Vec3) {
        if !a.is_finite() || !b.is_finite() {
            return;
        }
        let delta = b - a;
        let length = delta.length();
        if !length.is_finite() || length <= f32::EPSILON {
            return;
        }
        match self.dash {
            Some((on, off)) if on.is_finite() && off.is_finite() && on > 0.0 && off > 0.0 => {
                let direction = delta / length;
                let mut cursor = 0.0;
                while cursor < length {
                    let end = (cursor + on).min(length);
                    self.push(a + direction * cursor, a + direction * end);
                    cursor = end + off;
                }
            }
            _ => self.push(a, b),
        }
    }

    fn push(&mut self, a: Vec3, b: Vec3) {
        if !a.is_finite() || !b.is_finite() {
            return;
        }
        self.lines.push(LineInstanceGPU {
            a: [a.x, a.y, a.z, 1.0],
            b: [b.x, b.y, b.z, self.width],
            color: self.color,
        });
    }
}

fn emit_point_cross(glyph: &SourceGlyph, basis: &Basis, emitter: &mut LineEmitter<'_>) {
    let arm = glyph.domain_scale;
    for direction in [basis.right, basis.up, basis.normal] {
        emitter.segment(
            glyph.center - direction * arm,
            glyph.center + direction * arm,
        );
    }
}

fn emit_finite_line(glyph: &SourceGlyph, basis: &Basis, emitter: &mut LineEmitter<'_>) {
    let half = glyph.length * 0.5;
    let start = glyph.center - basis.normal * half;
    let end = glyph.center + basis.normal * half;
    let tick = 0.35 * glyph.domain_scale;
    emitter.segment(start, end);
    emitter.segment(start - basis.right * tick, start + basis.right * tick);
    emitter.segment(end - basis.right * tick, end + basis.right * tick);
}

fn emit_circle(
    center: Vec3,
    first: Vec3,
    second: Vec3,
    radius: f32,
    emitter: &mut LineEmitter<'_>,
) {
    if !radius.is_finite() || radius <= 0.0 {
        return;
    }
    let mut previous = center + first * radius;
    for segment in 1..=CIRCLE_SEGMENTS {
        let angle = std::f32::consts::TAU * segment as f32 / CIRCLE_SEGMENTS as f32;
        let point = center + (first * angle.cos() + second * angle.sin()) * radius;
        emitter.segment(previous, point);
        previous = point;
    }
}

fn emit_disc(glyph: &SourceGlyph, basis: &Basis, emitter: &mut LineEmitter<'_>) {
    let radius = glyph.radius;
    emit_circle(glyph.center, basis.right, basis.up, radius, emitter);
    if radius.is_finite() && radius > 0.0 {
        emitter.segment(
            glyph.center - basis.right * radius,
            glyph.center + basis.right * radius,
        );
        emitter.segment(
            glyph.center - basis.up * radius,
            glyph.center + basis.up * radius,
        );
    }
}

fn emit_sphere(glyph: &SourceGlyph, basis: &Basis, emitter: &mut LineEmitter<'_>) {
    let radius = glyph.radius;
    emit_circle(glyph.center, basis.right, basis.up, radius, emitter);
    emit_circle(glyph.center, basis.up, basis.normal, radius, emitter);
    emit_circle(glyph.center, basis.normal, basis.right, radius, emitter);
}

fn emit_wire(glyph: &SourceGlyph, basis: &Basis, extent: f32, emitter: &mut LineEmitter<'_>) {
    let half = 1.1 * extent;
    let sign = sign_factor(glyph.sign);
    emitter.segment(
        glyph.center - basis.normal * half,
        glyph.center + basis.normal * half,
    );

    let push = 0.2 * extent;
    let side = 0.12 * extent;
    for offset in [-0.6, 0.6] {
        let base = glyph.center + basis.normal * (offset * extent);
        let tip = base + basis.normal * (sign * push);
        let back = base - basis.normal * (sign * push * 0.5);
        emitter.segment(back + basis.right * side, tip);
        emitter.segment(back - basis.right * side, tip);
    }
}

fn emit_current_loop(glyph: &SourceGlyph, basis: &Basis, emitter: &mut LineEmitter<'_>) {
    let radius = glyph.radius;
    emit_circle(glyph.center, basis.right, basis.up, radius, emitter);
    if !radius.is_finite() || radius <= 0.0 {
        return;
    }

    let sign = sign_factor(glyph.sign);
    let size = 0.45 * glyph.domain_scale;
    for angle in [0.0_f32, std::f32::consts::PI] {
        let radial = basis.right * angle.cos() + basis.up * angle.sin();
        let point = glyph.center + radial * radius;
        let tangent = basis.normal.cross(radial) * sign;
        let tip = point + tangent * size;
        let back = point - tangent * (size * 0.5);
        let side = radial * (size * 0.6);
        emitter.segment(back + side, tip);
        emitter.segment(back - side, tip);
    }
}

fn emit_solenoid(glyph: &SourceGlyph, basis: &Basis, emitter: &mut LineEmitter<'_>) {
    let radius = glyph.radius;
    let length = glyph.length;
    let half = length * 0.5;
    let sign = sign_factor(glyph.sign);

    let turns = glyph.turns.min(MAX_VISIBLE_TURNS);
    if turns > 0 && radius > 0.0 && length > 0.0 {
        let steps = turns as usize * HELIX_SEGMENTS_PER_TURN;
        let mut previous = solenoid_point(glyph.center, basis, radius, length, turns, sign, 0);
        for step in 1..=steps {
            let point = solenoid_point(glyph.center, basis, radius, length, turns, sign, step);
            emitter.segment(previous, point);
            previous = point;
        }
    }

    emit_circle(
        glyph.center + basis.normal * half,
        basis.right,
        basis.up,
        radius,
        emitter,
    );
    emit_circle(
        glyph.center - basis.normal * half,
        basis.right,
        basis.up,
        radius,
        emitter,
    );

    if radius > 0.0 {
        let point = glyph.center + basis.normal * half + basis.right * radius;
        let tangent = basis.up * sign;
        let size = 0.45 * glyph.domain_scale;
        let tip = point + tangent * size;
        let back = point - tangent * (size * 0.5);
        let side = basis.right * (size * 0.6);
        emitter.segment(back + side, tip);
        emitter.segment(back - side, tip);
    }
}

fn solenoid_point(
    center: Vec3,
    basis: &Basis,
    radius: f32,
    length: f32,
    turns: u32,
    sign: f32,
    step: usize,
) -> Vec3 {
    let steps = turns as usize * HELIX_SEGMENTS_PER_TURN;
    let t = step as f32 / steps as f32;
    let angle = sign * std::f32::consts::TAU * turns as f32 * t;
    center
        + basis.normal * (length * (t - 0.5))
        + (basis.right * angle.cos() + basis.up * angle.sin()) * radius
}

fn emit_sign_marker(glyph: &SourceGlyph, basis: &Basis, emitter: &mut LineEmitter<'_>) {
    let center = glyph.center + basis.right * (1.6 * glyph.domain_scale);
    let half = 0.5 * glyph.domain_scale;
    emitter.segment(center - basis.right * half, center + basis.right * half);
    if glyph.sign == GlyphSign::Positive {
        emitter.segment(center - basis.up * half, center + basis.up * half);
    }
}
