// Vector Field Explorer — instanced arrow shader.
//
// One tapered shaft + cone head per instance, geometry generated in the
// vertex shader from `vertex_index` (no vertex buffer). Local geometry
// extends along +Y and is rotated so +Y maps onto the instance's unit
// direction. Half-lambert lit by Globals.light_dir.
//
// Bind group layout (same layout object is shared with the line pipeline):
//   (0) Globals uniform        — vertex+fragment
//   (1) instance storage array — vertex

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

struct VertexOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) light: f32, // 0..1 half-lambert factor
};

// Geometry layout per instance (triangle list, no vertex buffer):
//   verts 0..24   : shaft, 4 side quads, radius tapers 0.055 -> 0.03, y 0..0.7
//   verts 24..48  : cone head, 4 side quads, radius 0.12 -> 0, y 0.7..1.0
//   verts 48..54  : cone base plate (fan of 2 triangles over 4 corners)
const ARROW_VERTS: u32 = 54u;

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

// Side-quad corner order for a triangle list: two triangles, six vertices.
// p1 = center of the local y axis (0.0 = bottom ring, 1.0 = top ring).
@vertex
fn vs_arrow(@builtin(vertex_index) vertex_index: u32) -> VertexOut {
    let inst_idx = vertex_index / ARROW_VERTS;
    let j = vertex_index % ARROW_VERTS;
    let inst = arrow_instances[inst_idx];

    let b = basis(inst.dir_len.xyz);
    let len = inst.dir_len.w;
    let shaft_len = 0.7 * len;
    let head_len = 0.3 * len;

    var local: vec3<f32>;
    var normal: vec3<f32>;

    if (j < 24u) {
        // shaft side quads
        let quad = j / 6u;
        let r = j % 6u;
        let ta = f32(quad) * (2.0 * 3.14159265 / 4.0);
        let tb = ta + 2.0 * 3.14159265 / 4.0;
        let at_a = (r == 0u) || (r == 2u) || (r == 5u);
        let top = (r == 2u) || (r == 4u) || (r == 5u);
        let t = select(tb, ta, at_a);
        let y = select(shaft_len, 0.0, top);
        let radius = select(0.03 * len, 0.055 * len, top);
        local = ring_point(t, y, radius);
        normal = vec3<f32>(cos(t), 0.0, sin(t));
    } else if (j < 48u) {
        // cone head side quads
        let quad = (j - 24u) / 6u;
        let r = (j - 24u) % 6u;
        let ta = f32(quad) * (2.0 * 3.14159265 / 4.0);
        let tb = ta + 2.0 * 3.14159265 / 4.0;
        let at_a = (r == 0u) || (r == 2u) || (r == 5u);
        let tip = (r == 2u) || (r == 3u) || (r == 4u);
        let t = select(tb, ta, at_a);
        let y = select(head_len, 0.0, tip) + shaft_len;
        let radius = select(0.0, 0.12 * len, tip);
        local = ring_point(t, y, radius);
        let slope = normalize(vec3<f32>(0.0, head_len, 0.12 * len));
        normal = normalize(vec3<f32>(cos(t) * slope.x, slope.y, sin(t) * slope.x));
    } else {
        // cone base plate: 2 triangles over 4 ring corners at y = shaft_len
        let tri = (j - 48u) / 3u;
        let r = (j - 48u) % 3u;
        let corner0 = select(2u, 0u, tri == 0u);
        let corner = corner0 + r;
        let t = f32(corner) * (2.0 * 3.14159265 / 4.0);
        local = ring_point(t, shaft_len, 0.12 * len);
        normal = vec3<f32>(0.0, -1.0, 0.0);
    }

    let world = inst.origin.xyz + b * local;
    // b is orthonormal, so b * normal is the world-space normal (no inverse-transpose needed).
    let nl = clamp(dot(b * normal, globals.light_dir.xyz) * 0.5 + 0.5, 0.0, 1.0);

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
