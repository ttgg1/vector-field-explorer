// Vector Field Explorer — instanced combined 3D scene shader (tickets 44, 45,
// 62 and 64).
//
// One instance per arrow OR particle dot (ticket 62's combined, depth-sorted
// stream). Geometry is generated in the vertex shader from `vertex_index` (no
// vertex buffer). Every instance is allocated MAX_ARROW_VERTS invocations; the
// shader derives the instance index from that budget, reads the `origin.w` kind
// tag and exits unused invocations before any ring/trig work. Arrow instances
// run the style branch selected by `Globals.arrow_style.x` (0 Cone, 1 Cylinder,
// 2 Flat, 3 Line); particle instances expand a camera-facing dot billboard.
// `arrow_style.y` is the radial side count shared by Cone and Cylinder, and
// `arrow_params` carries the head/shaft proportions. Half-lambert lit by
// Globals.light_dir.
//
// Bind group layout (same layout object is shared with the 2D arrow, line and
// dot pipelines):
//   (0) Globals uniform        — vertex+fragment
//   (1) instance storage array — vertex

struct Globals {
    view_proj: mat4x4<f32>,
    inv_view_proj: mat4x4<f32>,
    camera_pos: vec4<f32>,
    light_dir: vec4<f32>,    // xyz: direction *toward* the light (normalized)
    camera_right: vec4<f32>, // xyz: world-space camera right axis (w = 0)
    camera_up: vec4<f32>,    // xyz: world-space camera up axis (w = 0)
    arrow_style: vec4<u32>,  // x: 3D style id (in 3D), y: sides, z/w: unused
    arrow_params: vec4<f32>, // x: head length, y: head width, z: shaft radius, w: unused
};

@group(0) @binding(0)
var<uniform> globals: Globals;

struct ArrowInstance {
    origin: vec4<f32>,   // xyz base position, w = kind tag: 1.0 arrow, 0.0 particle
    dir_len: vec4<f32>,  // xyz unit direction (arrows) / unused (dots), w = length or radius
    color: vec4<f32>,    // rgba 0..1
};

@group(0) @binding(1)
var<storage, read> arrow_instances: array<ArrowInstance>;

struct VertexOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) light: f32, // 0..1 half-lambert factor (arrows; dots ignore it)
    @location(2) uv: vec2<f32>, // dot-local disc coordinate, |uv| <= 1 inside
    @location(3) @interpolate(flat) kind: f32, // 1.0 arrow, 0.0 particle
};

const TAU: f32 = 6.283185307179586;
// The Flat style's camera-facing ribbon with shaft and head.
const ARROW_VERTS_FLAT: u32 = 9u;
// The Line style's headless camera-facing ribbon quad.
const ARROW_VERTS_LINE: u32 = 6u;
// The Cone tapers its shaft to this fraction at the head junction.
const CONE_TIP_RATIO: f32 = 0.6;
// Ticket 62: one draw cannot vary vertices per instance, so every instance is
// allocated the widest arrow family's budget (a MAX_SIDES-side cone). Mirrors
// `MAX_ARROW_VERTS` in src/renderer.rs.
const MAX_ARROW_VERTS: u32 = 186u;
// The particle billboard's six vertices (two triangles).
const DOT_VERTS: u32 = 6u;

// Style ids: 0 Cone, 1 Cylinder, 2 Flat, 3 Line.
fn style_vertex_count() -> u32 {
    let style = globals.arrow_style.x;
    if (style == 2u) {
        return ARROW_VERTS_FLAT;
    }
    if (style == 3u) {
        return ARROW_VERTS_LINE;
    }
    let sides = max(globals.arrow_style.y, 4u);
    if (style == 1u) {
        // Cylinder: shaft 6*sides + far-end cap 3*(sides-2), no head.
        return 9u * sides - 6u;
    }
    // Cone: shaft 6*sides + head 3*sides + full plate 3*(sides-2).
    return 12u * sides - 6u;
}

// Basis whose columns are u (side axis), a (arrow axis), v (second side axis).
fn basis(axis: vec3<f32>) -> mat3x3<f32> {
    let a = normalize(axis);
    let up = select(vec3<f32>(0.0, 1.0, 0.0), vec3<f32>(1.0, 0.0, 0.0), abs(a.y) >= 0.99);
    let u = normalize(cross(a, up));
    let v = cross(a, u);
    return mat3x3<f32>(u, a, v);
}

fn ring_point(theta: f32, y: f32, radius: f32) -> vec3<f32> {
    return vec3<f32>(cos(theta) * radius, y, sin(theta) * radius);
}

// Outward normal of a shaft side whose radius runs from `bottom_radius` at
// y = 0 to `top_radius` at shaft_len. The surface gradient is
// radial_hat * shaft_len + y_hat * (bottom_radius - top_radius), so a taper
// leans the normal upward instead of leaving it horizontal.
fn shaft_normal(t: f32, bottom_radius: f32, top_radius: f32, shaft_len: f32) -> vec3<f32> {
    let radial = vec3<f32>(cos(t), 0.0, sin(t));
    return normalize(radial * shaft_len + vec3<f32>(0.0, bottom_radius - top_radius, 0.0));
}

// Corner `j` of the particle billboard quad in [-1,1] dot-local space:
//   (-1,-1) (1,-1) (1,1) / (-1,-1) (1,1) (-1,1)
// so the unit circle inscribed in the square is the dot's silhouette. Mirrors
// `DOT_QUAD_CORNERS` in src/particles.rs and the 2D dot shader.
fn quad_corner(j: u32) -> vec2<f32> {
    let min_x = (j == 0u) || (j == 3u) || (j == 5u);
    let min_y = (j == 0u) || (j == 1u) || (j == 3u);
    return vec2<f32>(select(1.0, -1.0, min_x), select(1.0, -1.0, min_y));
}

@vertex
fn vs_scene(@builtin(vertex_index) vertex_index: u32) -> VertexOut {
    // Ticket 62: every instance owns a fixed MAX_ARROW_VERTS slice. Derive the
    // instance and local vertex from that budget, then read the kind tag.
    let inst_idx = vertex_index / MAX_ARROW_VERTS;
    let j = vertex_index % MAX_ARROW_VERTS;
    let inst = arrow_instances[inst_idx];

    let kind = inst.origin.w;
    let is_arrow = inst.origin.w > 0.5;
    var count = DOT_VERTS;
    if (is_arrow) {
        count = style_vertex_count();
    }
    // Cheap early-out before any basis/ring/trig work. All skipped vertices
    // land on one clip-space point outside the view volume, so unused
    // invocations cannot gain area.
    if (j >= count) {
        return VertexOut(vec4<f32>(0.0, 0.0, 2.0, 1.0), inst.color, 0.0, vec2<f32>(0.0), kind);
    }

    if (!is_arrow) {
        // Particle: the same camera-facing billboard as shaders.dot.wgsl. The
        // radius is in dir_len.w; the quad spans the camera's right/up axes.
        let corner = quad_corner(j);
        let radius = inst.dir_len.w;
        let world = inst.origin.xyz
            + (globals.camera_right.xyz * corner.x + globals.camera_up.xyz * corner.y) * radius;
        return VertexOut(
            globals.view_proj * vec4<f32>(world, 1.0),
            inst.color,
            1.0,
            corner,
            kind,
        );
    }

    let style = globals.arrow_style.x;
    let params = globals.arrow_params;
    let len = inst.dir_len.w;
    let shaft_len = (1.0 - params.x) * len;
    let head_len = params.x * len;
    let shaft_radius = params.z * len;
    let head_radius = params.y * len;

    // Flat: a camera-facing ribbon with a shaft and a head. It is built
    // directly in world space, so the arrow keeps a readable width from every
    // orbit angle; when the view direction is parallel to the arrow, the
    // camera-right axis is the fallback side vector.
    if (style == 2u) {
        let dir = normalize(inst.dir_len.xyz);
        let to_cam = globals.camera_pos.xyz - inst.origin.xyz;
        var side = cross(dir, to_cam);
        if (length(side) < 1e-4) {
            side = globals.camera_right.xyz;
        } else {
            side = normalize(side);
        }
        let base = inst.origin.xyz;
        let junction = base + dir * shaft_len;
        let tip = base + dir * len;

        var world: vec3<f32>;
        if (j < 6u) {
            // Shaft quad: r 0 (base,+) 1 (junction,+) 2 (base,-)
            //                 3 (junction,+) 4 (junction,-) 5 (base,-)
            let at_start = (j == 0u) || (j == 2u) || (j == 5u);
            let positive = (j == 0u) || (j == 1u) || (j == 3u);
            let along = select(junction, base, at_start);
            let sgn = select(-1.0, 1.0, positive);
            world = along + side * shaft_radius * sgn;
        } else if (j == 6u) {
            world = junction + side * head_radius;
        } else if (j == 7u) {
            world = junction - side * head_radius;
        } else {
            world = tip;
        }
        // Flat and unlit: a light factor of 1 resolves to the plain colour.
        return VertexOut(globals.view_proj * vec4<f32>(world, 1.0), inst.color, 1.0, vec2<f32>(0.0), kind);
    }

    // Line: a headless camera-facing ribbon over the full arrow length, built
    // like shaders.line.wgsl (camera-right fallback when the view is parallel).
    if (style == 3u) {
        let dir = normalize(inst.dir_len.xyz);
        let to_cam = globals.camera_pos.xyz - inst.origin.xyz;
        var side = cross(dir, to_cam);
        if (length(side) < 1e-4) {
            side = globals.camera_right.xyz;
        } else {
            side = normalize(side);
        }
        let base = inst.origin.xyz;
        let tip = base + dir * len;

        // Quad: r 0 (base,+) 1 (tip,+) 2 (base,-)
        //          3 (tip,+)  4 (tip,-) 5 (base,-)
        let at_start = (j == 0u) || (j == 2u) || (j == 5u);
        let positive = (j == 0u) || (j == 1u) || (j == 3u);
        let along = select(tip, base, at_start);
        let sgn = select(-1.0, 1.0, positive);
        let world = along + side * shaft_radius * sgn;
        // Flat and unlit, like the streamline line shader.
        return VertexOut(globals.view_proj * vec4<f32>(world, 1.0), inst.color, 1.0, vec2<f32>(0.0), kind);
    }

    let b = basis(inst.dir_len.xyz);
    var local: vec3<f32>;
    var normal: vec3<f32>;

    if (style == 0u) {
        // Cone: an n-sided tapered shaft, cone head and full plate fan.
        let sides = max(globals.arrow_style.y, 4u);
        let step = TAU / f32(sides);
        let shaft_verts = 6u * sides;
        let head_verts = 9u * sides;
        let top_radius = shaft_radius * CONE_TIP_RATIO;
        if (j < shaft_verts) {
            // Shaft side quads: two triangles per side.
            let quad = j / 6u;
            let r = j % 6u;
            let ta = f32(quad) * step;
            let tb = ta + step;
            let at_a = (r == 0u) || (r == 2u) || (r == 5u);
            let top = (r == 2u) || (r == 4u) || (r == 5u);
            let t = select(tb, ta, at_a);
            let y = select(shaft_len, 0.0, top);
            let radius = select(top_radius, shaft_radius, top);
            local = ring_point(t, y, radius);
            normal = shaft_normal(t, shaft_radius, top_radius, shaft_len);
        } else if (j < head_verts) {
            // Cone head: one real triangle per side (ticket 44's fix).
            let side = (j - shaft_verts) / 3u;
            let k = (j - shaft_verts) % 3u;
            let ta = f32(side) * step;
            let tb = ta + step;
            let tip = k == 2u;
            let t = select(tb, ta, k != 1u);
            let y = select(shaft_len, shaft_len + head_len, tip);
            let radius = select(head_radius, 0.0, tip);
            local = ring_point(t, y, radius);
            // Repair the ticket-44 normal bug: derive the facet normal from
            // this side's actual radial direction instead of a slope whose x
            // component was zero (which flattened every side to +Y).
            let tm = ta + 0.5 * step;
            let radial = vec3<f32>(cos(tm), 0.0, sin(tm));
            normal = normalize(radial * head_len + vec3<f32>(0.0, head_radius, 0.0));
        } else {
            // Base plate: the full fan of sides - 2 triangles over every ring
            // corner at y = shaft_len, so the underside is closed.
            let plate_j = j - head_verts;
            let tri = plate_j / 3u;
            let r = plate_j % 3u;
            let corner = select(select(tri + 2u, tri + 1u, r == 1u), 0u, r == 0u);
            let t = f32(corner) * step;
            local = ring_point(t, shaft_len, head_radius);
            normal = vec3<f32>(0.0, -1.0, 0.0);
        }
    } else {
        // Cylinder: a constant-radius n-sided shaft over the full arrow length
        // plus a flat fan cap at the far end. No head, taper or underside plate.
        let sides = max(globals.arrow_style.y, 4u);
        let step = TAU / f32(sides);
        let shaft_verts = 6u * sides;
        if (j < shaft_verts) {
            // Shaft side quads: two triangles per side, at constant radius.
            let quad = j / 6u;
            let r = j % 6u;
            let ta = f32(quad) * step;
            let tb = ta + step;
            let at_a = (r == 0u) || (r == 2u) || (r == 5u);
            let top = (r == 2u) || (r == 4u) || (r == 5u);
            let t = select(tb, ta, at_a);
            let y = select(len, 0.0, top);
            local = ring_point(t, y, shaft_radius);
            normal = shaft_normal(t, shaft_radius, shaft_radius, len);
        } else {
            // Far-end cap: the full fan of sides - 2 triangles. The corner
            // order is reversed relative to the cone's underside plate so the
            // geometric winding faces +Y, matching the declared normal.
            let cap_j = j - shaft_verts;
            let tri = cap_j / 3u;
            let r = cap_j % 3u;
            let corner = select(select(tri + 1u, tri + 2u, r == 1u), 0u, r == 0u);
            let t = f32(corner) * step;
            local = ring_point(t, len, shaft_radius);
            normal = vec3<f32>(0.0, 1.0, 0.0);
        }
    }

    let world = inst.origin.xyz + b * local;
    // b is orthonormal, so b * normal is the world-space normal (no inverse-transpose needed).
    let nl = clamp(dot(b * normal, globals.light_dir.xyz) * 0.5 + 0.5, 0.0, 1.0);

    return VertexOut(
        globals.view_proj * vec4<f32>(world, 1.0),
        inst.color,
        nl,
        vec2<f32>(0.0),
        kind,
    );
}

@fragment
fn fs_scene(v: VertexOut) -> @location(0) vec4<f32> {
    // Derivatives run before the kind branch: divergent control flow makes
    // them implementation-defined. Arrow fragments carry uv = 0, so r/aa are
    // inert for them; dot fragments use them for the disc and its one-pixel rim.
    let r = length(v.uv);
    let aa = max(fwidth(r), 1e-5);

    if (v.kind < 0.5) {
        // Particle: the same round dot as shaders.dot.wgsl. The quad is the
        // disc's bounding square, so the fade runs inward from r == 1; nothing
        // outside the disc survives, and the alpha rim blends into whatever is
        // behind the dot.
        if (r > 1.0) {
            discard;
        }
        let coverage = 1.0 - smoothstep(1.0 - aa, 1.0, r);
        // Spherical normal in the camera basis; unit length by construction,
        // so no normalize (and none at the rim, where it would divide by zero).
        let nz = sqrt(max(1.0 - r * r, 0.0));
        let normal = globals.camera_right.xyz * v.uv.x
            + globals.camera_up.xyz * v.uv.y
            + cross(globals.camera_right.xyz, globals.camera_up.xyz) * nz;
        let nl = clamp(dot(normal, globals.light_dir.xyz) * 0.5 + 0.5, 0.0, 1.0);
        let lit = v.color.rgb * (0.35 + 0.65 * nl);
        return vec4<f32>(lit, v.color.a * coverage);
    }

    let lit = v.color.rgb * (0.35 + 0.65 * v.light);
    return vec4<f32>(lit, v.color.a);
}
