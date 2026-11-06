// Vector Field Explorer — instanced arrow + line shaders.
//
// Two pipelines share the same bind group layout:
//   (0) Globals uniform      — view/proj, inverse, camera, light
//   (1) instance storage     — ArrowInstance or LineInstance array
//
// Arrow mode (vs_arrow): draws one tapered shaft + cone head per instance,
// geometry generated in the vertex shader from `vertex_index` (no vertex
// buffer). Local geometry extends along +Y and is rotated so +Y maps to the
// instance's unit direction. Half-lambert lit by `light_dir`.
//
// Line mode (vs_line): one camera-facing ribbon quad per segment, unlit.

struct Globals {
    view_proj: mat4x4<f32>,
    inv_view_proj: mat4x4<f32>,
    camera_pos: vec4<f32>,
    light_dir: vec4<f32>, // xyz: direction *toward* the light (normalized)
};

@group(0) @binding(0)
var<uniform> globals: Globals;

struct ArrowInstance {
    origin: vec4<f32>,   // xyz base position, w = 1.0
    dir_len: vec4<f32>,  // xyz unit direction, w = length
    color: vec4<f32>,    // rgba 0..1
};

@group(0) @binding(1)
var<storage, read> arrow_instances: array<ArrowInstance>;

struct LineInstance {
    a: vec4<f32>,       // xyz start, w = 1.0
    b: vec4<f32>,       // xyz end,   w = line width
    color: vec4<f32>,   // rgba 0..1
};

@group(0) @binding(1)
var<storage, read> line_instances: array<LineInstance>;

struct VertexOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) light: f32, // 0..1 half-lambert factor
};

// ---------------------------------------------------------------- arrow ----

// Geometry layout per instance (triangle list, no vertex buffer):
//   verts 0..24   : shaft, 4 side quads, radius tapers 0.055 -> 0.03, y 0..0.7
//   verts 24..48  : cone head, 4 side quads, radius 0.12 -> 0, y 0.7..1.0
//   verts 48..54  : cone base plate (fan of 2 triangles over 4 corners)
const ARROW_VERTS: u32 = 54u;

fn basis(axis: vec3<f32>) -> mat3x3<f32> {
    let a = normalize(axis);
    let ref_up = select(vec3<f32>(1.0, 0.0, 0.0), vec3<f32>(0.0, 1.0, 0.0), abs(a.y) < 0.99);
    let u = normalize(cross(a, ref_up));
    let v = cross(a, u);
    return mat3x3<f32>(u, a, v);
}

fn ring_point(theta: f32, y: f32, radius: f32) -> vec3<f32> {
    vec3<f32>(cos(theta) * radius, y, sin(theta) * radius)
}

// side quad corner sequence for triangle list: (a,0)-(b,0)-(a,1) / (b,0)-(b,1)-(a,1)
fn quad_vertex(j: u32) -> u32 {
    // corner index pattern r: 0:(a,lo) 1:(b,lo) 2:(a,hi) 3:(b,lo) 4:(b,hi) 5:(a,hi)
    let r = j % 6u;
    let corner = select(r - 3u, r % 3u, r < 3u); // 0..2 → a-lo/b-lo/a-hi then b-lo/b-hi/a-hi
    let _ = corner;
    return r;
}

@vertex
fn vs_arrow(@builtin(vertex_index) vertex_index: u32) -> VertexOut {
    let inst_idx = vertex_index / ARROW_VERTS;
    let j = vertex_index % ARROW_VERTS;
    let inst = arrow_instances[inst_idx];

    let b = basis(inst.dir_len.xyz);
    let len = inst.dir_len.w;

    var local: vec3<f32>;
    var normal: vec3<f32>;
    let shaft_len = 0.7 * len;
    let head_len = (1.0 - 0.7) * len;

    if (j < 24u) {
        // shaft side quads
        let quad = j / 6u;
        let r = j % 6u;
        let ta = f32(quad) * (2.0 * 3.14159265 / 4.0);
        let tb = ta + 2.0 * 3.14159265 / 4.0;
        var corner_hi = (r == 2u) || (r == 4u) || (r == 5u);
        let a_corner = (r == 0u) || (r == 2u) || (r == 5u);
        let t = select(tb, ta, a_corner);
        let y = select(shaft_len, 0.0, corner_hi);
        let radius = select(0.03 * len, 0.055 * len, corner_hi);
        local = ring_point(t, y, radius);
        normal = vec3<f32>(cos(t), 0.0, sin(t));
        corner_hi = corner_hi; // silence unused-var lint on some backends
    } else if (j < 48u) {
        // cone head side quads
        let quad = (j - 24u) / 6u;
        let r = (j - 24u) % 6u;
        let ta = f32(quad) * (2.0 * 3.14159265 / 4.0);
        let tb = ta + 2.0 * 3.14159265 / 4.0;
        let a_corner = (r == 0u) || (r == 2u) || (r == 5u);
        let tip = (r == 2u) || (r == 3u) || (r == 4u);
        let t = select(tb, ta, a_corner);
        let y = select(head_len, 0.0, tip) + shaft_len;
        let radius = select(0.0, 0.12 * len, tip);
        local = ring_point(t, y, radius);
        let slope = normalize(vec3<f32>(0.0, head_len, 0.12 * len));
        normal = normalize(vec3<f32>(cos(t) * slope.x, slope.y, sin(t) * slope.x));
    } else {
        // cone base plate: 2 triangles over 4 ring corners at y = shaft_len
        let tri = (j - 48u) / 3u;
        let r = (j - 48u) % 3u;
        let quad_origin = select(2u, 0u, tri == 0u);
        let corner = quad_origin + r;
        let t = f32(corner) * (2.0 * 3.14159265 / 4.0);
        local = ring_point(t, shaft_len, 0.12 * len);
        normal = vec3<f32>(0.0, -1.0, 0.0);
    }

    let world = inst.origin.xyz + b * local;
    let nl = clamp(dot(normal, globals.light_dir.xyz) * 0.5 + 0.5, 0.0, 1.0);

    return VertexOut(
        globals.view_proj * vec4<f32>(world, 1.0),
        inst.color,
        nl,
    );
}

@fragment
fn fs_main(v: VertexOut) -> @location(0) vec4<f32> {
    let lit = v.color.rgb * (0.35 + 0.65 * v.light);
    return vec4<f32>(lit, v.color.a);
}

// ----------------------------------------------------------------- line ----

@vertex
fn vs_line(@builtin(vertex_index) vertex_index: u32) -> VertexOut {
    let inst_idx = vertex_index / 6u;
    let j = vertex_index % 6u;
    let inst = line_instances[inst_idx];

    let dir = inst.b.xyz - inst.a.xyz;
    let d = normalize(dir);
    let to_cam = normalize(globals.camera_pos.xyz - inst.a.xyz);
    let side = normalize(cross(d, to_cam));
    let half_w = inst.b.w * 0.5;

    var corner: vec3<f32>;
    // r: 0 (a,+) 1 (b,+) 2 (a,-) / 3 (b,+) 4 (b,-) 5 (a,-)
    let at_a = (j == 0u) || (j == 2u) || (j == 5u);
    let positive = (j == 0u) || (j == 1u) || (j == 3u);
    let p = select(inst.b.xyz, inst.a.xyz, at_a);
    let sgn = select(-1.0, 1.0, positive);
    // degenerate segments fall back to a zero-size quad
    if (length(dir) < 1e-8) {
        corner = p;
    } else {
        corner = p + side * half_w * sgn;
    }

    return VertexOut(
        globals.view_proj * vec4<f32>(corner, 1.0),
        inst.color,
        1.0,
    );
}

@fragment
fn fs_line(v: VertexOut) -> @location(0) vec4<f32> {
    return v.color;
}
